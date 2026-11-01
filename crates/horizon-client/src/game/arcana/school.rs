//! Server adapter and public visual snapshots for the complete Fire school.
use super::*;
use arcana_core::{
    fire_school::{self, Effect, FireSchoolWorld, Owner},
    network::FireStatus,
};

pub(in crate::game) const CHANNEL: &str = "arcana:fire_school_v1";
pub(super) struct Active {
    pub id: u64,
    pub effect: Effect,
}
pub(super) struct Lease {
    pub id: u64,
    pub may_fly: bool,
    pub flying: bool,
    pub speed: f32,
}
#[derive(Default)]
pub(super) struct Actor {
    pub effects: Vec<Active>,
    pub lease: Option<Lease>,
    pub primary_after: u64,
    pub serial: u64,
}
impl Actor {
    pub fn status(&self) -> FireStatus {
        FireStatus {
            shield: self
                .effects
                .iter()
                .any(|e| e.effect.spell_id == "flame_shield"),
            avatar: self.effects.iter().any(|e| {
                e.effect.spell_id == "fire_avatar"
                    && self.lease.as_ref().is_some_and(|l| l.id == e.id)
            }),
            phoenix: self.effects.iter().any(|e| {
                e.effect.spell_id == "phoenix_form"
                    && e.effect.stage < 2
                    && self.lease.as_ref().is_some_and(|l| l.id == e.id)
            }),
            flight: self.lease.is_some(),
        }
    }
    pub fn next_bolt(&mut self) -> u64 {
        self.serial += 1;
        (1 << 63) | self.serial
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Body {
    pub uuid: [i32; 4],
    pub flying: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Visual {
    pub owner: u32,
    pub id: u64,
    pub dimension: String,
    pub effect: Effect,
    pub expired: bool,
    #[serde(default)]
    pub body: Option<Body>,
}
pub(super) struct Viewed {
    pub visual: Visual,
    pub received: Instant,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub outbox: Vec<Visual>,
    pub views: HashMap<(u32, u64), Viewed>,
    pub terrain: HashMap<String, super::terrain::State>,
    pub builders: HashMap<u128, super::terrain::Policy>,
    pub own_fire_until: HashMap<u128, u64>,
    pub public_fire: HashMap<String, super::terrain::Viewed>,
    pub audio_loaded: bool,
    pub audio_cues: HashMap<(u32, u64, u8), Instant>,
    pub movement: super::movement::Mode,
}

impl Game<'_> {
    pub(in crate::game) fn arcana_form_mouse(
        &mut self,
        button: winit::event::MouseButton,
        pressed: bool,
    ) -> bool {
        use winit::event::MouseButton;
        if self.screen != crate::game::Screen::Playing || self.remote.is_none() {
            return false;
        }
        let consume = self.arcana.snapshot.as_ref().is_some_and(|s| {
            button == MouseButton::Left && (s.fire.avatar || s.fire.phoenix) && s.fire.flight
                || button == MouseButton::Right && s.fire.phoenix
        });
        if consume && pressed {
            self.arcana_send(if button == MouseButton::Left {
                Action::Primary
            } else {
                Action::Secondary
            });
        }
        if !consume
            && button == MouseButton::Left
            && pressed
            && let Some(at) = self.arcana_pick_fire()
        {
            self.arcana_send(Action::Extinguish { at });
            return true;
        }
        consume
    }
    pub(super) fn arcana_school_permits(&self, actor: &super::Actor, id: &str) -> bool {
        if !fire_school::supported(id) {
            return true;
        }
        if actor.school.effects.len() >= fire_school::EFFECTS_PER_OWNER {
            return false;
        }
        let total = actor.school.effects.len()
            + self
                .arcana
                .actors
                .values()
                .filter(|a| a.dimension == self.dim.id())
                .map(|a| a.school.effects.len())
                .sum::<usize>();
        total < fire_school::EFFECTS_PER_LEVEL
            && (!matches!(id, "flame_shield" | "fire_avatar" | "phoenix_form")
                || !actor.school.effects.iter().any(|e| e.effect.spell_id == id))
    }
    pub(in crate::game) fn arcana_has_flight(&self, uuid: u128) -> bool {
        self.arcana
            .actors
            .get(&uuid)
            .is_some_and(|a| a.school.lease.is_some())
    }
    pub(in crate::game) fn arcana_previous_flying(&self, uuid: u128) -> Option<bool> {
        self.arcana
            .actors
            .get(&uuid)?
            .school
            .lease
            .as_ref()
            .map(|l| l.flying)
    }
    fn arcana_school_owner(&self, actor: &super::Actor, tick: u64) -> Option<Owner> {
        let p = self.player.as_ref()?;
        let (hand, direction) = self.arcana_context()?;
        Some(Owner {
            id: u64::from(self.lan_acting.unwrap_or(0)),
            position: core(p.position),
            eye: DVec3::from_array(self.eye_position()?),
            hand,
            direction,
            velocity: core(p.velocity),
            channeling: actor
                .caster
                .status(tick)
                .is_some_and(|c| c.spell_id == "inferno_beam" && c.phase == Phase::Channeling),
        })
    }
    fn arcana_school_visual(&mut self, id: u64, effect: &Effect, expired: bool) {
        let uuid = self.acting_player_uuid();
        // ElementBodyLayer follows active form effects, independently of the
        // temporary flight lease (another form may have replaced that lease).
        let body = !expired
            && arcana_core::forms::Kind::from_spell(&effect.spell_id, effect.stage).is_some();
        let body = body.then(|| Body {
            uuid,
            flying: self.player.as_ref().is_some_and(|p| p.flying),
        });
        self.arcana.school.outbox.push(Visual {
            owner: self.lan_acting.unwrap_or(0),
            id,
            dimension: self.dim.id().into(),
            effect: effect.clone(),
            expired,
            body,
        });
    }
    pub(super) fn arcana_release_school(
        &mut self,
        actor: &mut super::Actor,
        event: &CastEvent,
    ) -> bool {
        if !fire_school::supported(&event.spell_id) {
            return false;
        }
        let tick = self.world.game_time.max(0) as u64;
        let Some(context) = self.arcana_school_owner(actor, tick) else {
            return true;
        };
        let spell = catalog().get(&event.spell_id).unwrap();
        let owner = self.acting_player_uuid();
        self.arcana_begin_hit(if spell.id == "inferno_beam" {
            super::pvp::Rule::once()
        } else {
            super::pvp::Rule::stable()
        });
        let result = Effect::release(&mut PilotWorld { game: self, owner }, context, spell, tick);
        match result {
            Ok((effect, outcome)) => {
                self.arcana_finish_hit(actor, outcome.xp);
                if matches!(spell.id.as_str(), "fire_avatar" | "phoenix_form") {
                    self.arcana_start_flight(actor, event.cast_id);
                }
                if spell.id == "flame_shield" || spell.id == "phoenix_form" {
                    self.add_player_effect(horizon_client::effects::EffectInstance::new(
                        "fire_resistance",
                        spell.duration as i32,
                        0,
                    ));
                }
                if spell.id == "flame_shield" {
                    self.add_player_effect(horizon_client::effects::EffectInstance::new(
                        "resistance",
                        spell.duration as i32,
                        0,
                    ));
                }
                self.arcana_school_visual(event.cast_id, &effect, false);
                log::info!(
                    "Arcana fire school release: spell={}, xp={}",
                    spell.id,
                    outcome.xp
                );
                actor.school.effects.push(Active {
                    id: event.cast_id,
                    effect,
                });
            }
            Err(error) => {
                self.arcana_finish_hit(actor, 0);
                log::warn!("Arcana fire school release rejected: {error}");
            }
        }
        true
    }
    pub(super) fn arcana_tick_school(&mut self, actor: &mut super::Actor, tick: u64) {
        let Some(context) = self.arcana_school_owner(actor, tick) else {
            return;
        };
        let owner = self.acting_player_uuid();
        for mut active in std::mem::take(&mut actor.school.effects) {
            let spell = catalog().get(&active.effect.spell_id).unwrap();
            let rule = match spell.id.as_str() {
                "flame_wall" | "fire_blast" | "phoenix_form" => super::pvp::Rule::per_hit(),
                "inferno_beam" | "fire_ring" | "fire_avatar" => super::pvp::Rule::once(),
                _ => super::pvp::Rule::stable(),
            };
            self.arcana_begin_hit(rule);
            let result =
                active
                    .effect
                    .tick(&mut PilotWorld { game: self, owner }, context, spell, tick);
            let expired;
            match result {
                Ok(out) => {
                    self.arcana_finish_hit(actor, out.xp);
                    expired = out.finished;
                    for meteor in out.meteors {
                        let total = actor.bolts.len()
                            + self
                                .arcana
                                .actors
                                .values()
                                .filter(|a| a.dimension == self.dim.id())
                                .map(|a| a.bolts.len())
                                .sum::<usize>();
                        if total >= 192 || actor.bolts.len() >= 48 {
                            break;
                        }
                        let id = actor.school.next_bolt();
                        if let Ok(mut bolt) =
                            arcana_core::starter::Bolt::new(id, spell, meteor.position, -DVec3::Y)
                        {
                            bolt.velocity = meteor.velocity;
                            actor.bolts.push(starters::ActiveBolt { born: tick, bolt });
                        }
                    }
                    if out.xp > 0 || out.impact {
                        log::info!(
                            "Arcana fire school tick: spell={}, age={}, xp={}, impact={}",
                            spell.id,
                            active.effect.age,
                            out.xp,
                            out.impact
                        );
                    }
                    if out.impact
                        && spell.id == "phoenix_form"
                        && actor
                            .school
                            .lease
                            .as_ref()
                            .is_some_and(|l| l.id == active.id)
                    {
                        self.arcana_end_flight(actor);
                    }
                }
                Err(error) => {
                    self.arcana_finish_hit(actor, 0);
                    log::warn!("Arcana fire school tick rejected: {error}");
                    expired = true;
                }
            }
            if expired || tick.is_multiple_of(4) {
                self.arcana_school_visual(active.id, &active.effect, expired);
            }
            if expired {
                if actor
                    .school
                    .lease
                    .as_ref()
                    .is_some_and(|l| l.id == active.id)
                {
                    self.arcana_end_flight(actor);
                }
            } else {
                actor.school.effects.push(active);
            }
        }
        if actor.school.lease.is_some() {
            self.arcana
                .school
                .own_fire_until
                .insert(owner_uuid(owner), tick + 160);
        }
        if let Some(p) = self.player.as_mut() {
            p.arcana_shield = actor.school.status().shield;
            p.arcana_fire_immune = actor.school.lease.is_some()
                || self
                    .arcana
                    .school
                    .own_fire_until
                    .get(&owner_uuid(owner))
                    .is_some_and(|until| *until > tick);
            if p.arcana_fire_immune {
                p.fire_ticks = -20;
                p.fall_distance = 0.0;
            }
        }
    }
    pub(super) fn arcana_stop_school(&mut self, actor: &mut super::Actor) {
        let owner = owner_uuid(self.acting_player_uuid());
        self.arcana.school.own_fire_until.remove(&owner);
        for active in actor.school.effects.drain(..) {
            self.arcana_school_visual(active.id, &active.effect, true);
        }
        self.arcana_end_flight(actor);
        if let Some(p) = self.player.as_mut() {
            p.arcana_shield = false;
            p.arcana_fire_immune = false;
        }
    }
    fn arcana_start_flight(&mut self, actor: &mut super::Actor, id: u64) {
        let Some(p) = self.player.as_mut() else {
            return;
        };
        if let Some(old) = actor.school.lease.as_mut() {
            old.id = id;
        } else {
            actor.school.lease = Some(Lease {
                id,
                may_fly: p.may_fly,
                flying: p.flying,
                speed: p.flying_speed,
            });
        }
        p.may_fly = true;
        p.flying = true;
        p.flying_speed = 0.075;
        p.fall_distance = 0.0;
        // MagicStatuses.fly: leave the ground before vanilla's landing rule
        // can disable the just-granted flight. Keep the original lease flags.
        if p.ground.on_ground {
            p.ground.on_ground = false;
            p.velocity.y += 0.3;
            p.health.sync_velocity = true;
        }
        self.arcana_abilities(true, true, 0.075);
    }
    fn arcana_end_flight(&mut self, actor: &mut super::Actor) {
        let Some(lease) = actor.school.lease.take() else {
            return;
        };
        let may_fly = lease.may_fly || self.creative || self.spectator();
        let flying = may_fly && lease.flying;
        if let Some(p) = self.player.as_mut() {
            p.may_fly = may_fly;
            p.flying = flying;
            p.flying_speed = lease.speed;
            p.fall_distance = 0.0;
        }
        self.arcana_abilities(may_fly, flying, lease.speed);
        self.add_player_effect(horizon_client::effects::EffectInstance::new(
            "slow_falling",
            160,
            0,
        ));
    }
    fn arcana_abilities(&mut self, may_fly: bool, flying: bool, speed: f32) {
        self.guest_packets.push(C::PlayerAbilities(
            horizon_protocol::packets::play::PlayerAbilities {
                flags: if self.creative { 1 | 8 } else { 0 }
                    | if may_fly { 4 } else { 0 }
                    | if flying { 2 } else { 0 },
                flying_speed: speed,
                walking_speed: 0.1,
            },
        ));
    }
    pub(super) fn arcana_school_action(
        &mut self,
        actor: &mut super::Actor,
        action: &Action,
        tick: u64,
    ) {
        if !self.arcana_available() {
            return;
        }
        if super::movement::cast_mode(actor.caster.status(tick).as_ref(), tick)
            == super::movement::Mode::Rooted
        {
            return;
        }
        match action {
            Action::Dash => {
                if actor
                    .state
                    .passive_fire_dash(actor.caster.idle(), actor.ritual.active().is_none())
                {
                    let id = actor.school.next_bolt();
                    let event = CastEvent {
                        spell_id: "fire_dash".into(),
                        cast_id: id,
                        start_tick: tick,
                        server_tick: tick,
                        phase: Phase::Release,
                    };
                    self.arcana_release_school(actor, &event);
                }
            }
            Action::Primary
                if (actor.school.status().avatar || actor.school.status().phoenix)
                    && actor.school.lease.is_some()
                    && tick >= actor.school.primary_after =>
            {
                let total = actor.bolts.len()
                    + self
                        .arcana
                        .actors
                        .values()
                        .filter(|a| a.dimension == self.dim.id())
                        .map(|a| a.bolts.len())
                        .sum::<usize>();
                if actor.bolts.len() >= 48 || total >= 192 {
                    return;
                }
                let Some(eye) = self.eye_position().map(DVec3::from_array) else {
                    return;
                };
                let Some((_, direction)) = self.arcana_context() else {
                    return;
                };
                if !actor.state.spend_mana(6.0) {
                    return;
                }
                let id = actor.school.next_bolt();
                if let Ok(bolt) =
                    arcana_core::starter::Bolt::empowered(id, eye + direction * 0.8, direction)
                {
                    actor.school.primary_after = tick + 16;
                    actor.bolts.push(starters::ActiveBolt { born: tick, bolt });
                }
            }
            Action::Secondary => {
                if let Some((_, direction)) = self.arcana_context() {
                    for active in &mut actor.school.effects {
                        if !actor
                            .school
                            .lease
                            .as_ref()
                            .is_some_and(|l| l.id == active.id)
                        {
                            continue;
                        }
                        if active.effect.dive(direction) {
                            if let Some(p) = self.player.as_mut() {
                                p.velocity = native(direction * 4.0);
                                p.health.sync_velocity = true;
                            }
                            self.arcana_school_visual(active.id, &active.effect, false);
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn arcana_school_client_tick(&mut self) {
        self.arcana_prepare_audio();
        self.arcana
            .school
            .audio_cues
            .retain(|_, at| at.elapsed().as_secs() < 150);
        self.arcana
            .school
            .views
            .retain(|_, v| v.received.elapsed().as_secs_f32() < 1.0);
        if self.screen == crate::game::Screen::Playing
            && self.remote.is_some()
            && self.ticks.is_multiple_of(4)
            && (self.keys.contains(&winit::keyboard::KeyCode::ControlLeft)
                || self.keys.contains(&winit::keyboard::KeyCode::ControlRight))
        {
            self.arcana_send(Action::Dash);
        }
    }
    pub(super) fn arcana_school_flush(&mut self, host: &LanHost) {
        self.arcana_fire_terrain_flush(host);
        for visual in self.arcana.school.outbox.drain(..) {
            if let Ok(data) = serde_json::to_vec(&visual) {
                let packet = C::CustomPayload(CustomPayload {
                    channel: CHANNEL.into(),
                    data,
                });
                for guest in host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.placed && g.dimension == visual.dimension)
                {
                    guest.send(&packet);
                }
            }
        }
    }
    pub(in crate::game) fn arcana_receive_school(&mut self, bytes: &[u8]) {
        if bytes.len() > MAX_PACKET {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Visual>(bytes) else {
            return;
        };
        if !fire_school::supported(&v.effect.spell_id)
            || !v.effect.position.is_finite()
            || !v.effect.direction.is_finite()
            || !v.effect.end.is_finite()
            || v.effect.age > 2440
            || v.effect.life > 2440
            || v.body.as_ref().is_some_and(|_| {
                arcana_core::forms::Kind::from_spell(&v.effect.spell_id, v.effect.stage).is_none()
            })
        {
            return;
        }
        let key = (v.owner, v.id);
        if !v.expired {
            self.arcana_fire_sound(v.owner, v.id, 1, v.effect.position);
            if v.effect.spell_id == "phoenix_form" && v.effect.stage == 2 {
                self.arcana_fire_sound(v.owner, v.id, 2, v.effect.position);
            }
            if matches!(
                v.effect.spell_id.as_str(),
                "inferno_beam" | "fire_storm" | "fire_avatar" | "flame_wall"
            ) {
                self.arcana_fire_sound(
                    v.owner,
                    v.id,
                    10 + (v.effect.age / 60) as u8,
                    v.effect.position,
                );
            }
        }
        if v.expired {
            self.arcana.school.views.remove(&key);
        } else if self.arcana.school.views.len() < 256
            || self.arcana.school.views.contains_key(&key)
        {
            self.arcana.school.views.insert(
                key,
                Viewed {
                    visual: v,
                    received: Instant::now(),
                },
            );
        }
    }
}

impl FireSchoolWorld for PilotWorld<'_, '_> {
    fn surface(&self, near: DVec3) -> Result<DVec3, String> {
        let p = near.floor().as_ivec3();
        for dy in (-5..=4).rev() {
            let at = DVec3::new(f64::from(p.x), f64::from(p.y + dy), f64::from(p.z));
            if !self.loaded(at) {
                continue;
            }
            let current = self
                .game
                .res
                .facts()
                .get(self.game.world.block(p.x, p.y + dy, p.z));
            let below = self
                .game
                .res
                .facts()
                .get(self.game.world.block(p.x, p.y + dy - 1, p.z));
            if self
                .game
                .res
                .facts()
                .is_air(self.game.world.block(p.x, p.y + dy, p.z))
                && !self
                    .game
                    .res
                    .facts()
                    .is_air(self.game.world.block(p.x, p.y + dy - 1, p.z))
                && current.fluid == horizon_client::blocks::Fluid::None
                && below.fluid == horizon_client::blocks::Fluid::None
                && !matches!(
                    below.name.as_ref(),
                    "minecraft:fire" | "minecraft:soul_fire"
                )
            {
                return Ok(at + DVec3::new(0.5, 0.0, 0.5));
            }
        }
        if !self.loaded(near) {
            return Err("unloaded fire surface".into());
        }
        Ok(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
    }
    fn ground(&self, near: DVec3) -> Result<DVec3, String> {
        if !self.loaded(near) {
            return Err("unloaded fire ground".into());
        }
        let p = near.floor().as_ivec3();
        for y in (horizon_client::chunk::MIN_Y
            ..=p.y
                .min(horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT - 1))
            .rev()
        {
            let state = self.game.world.block(p.x, y, p.z);
            let mut scene = SceneCollisions {
                world: &self.game.world,
                binding: self.game.collision,
                subject: None,
            };
            if self
                .game
                .collision
                .collision_shape(&mut scene, state, [p.x, y, p.z])
                .is_ok_and(|s| !s.is_empty())
            {
                return Ok(DVec3::new(
                    f64::from(p.x) + 0.5,
                    f64::from(y + 1),
                    f64::from(p.z) + 0.5,
                ));
            }
        }
        Ok(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5))
    }
    fn loaded(&self, p: DVec3) -> bool {
        let at = p.floor().as_ivec3();
        p.is_finite()
            && self.game.world.chunks.contains_key(&(at.x >> 4, at.z >> 4))
            && self.game.border.contains(p.x, p.z)
            && at.y >= horizon_client::chunk::MIN_Y
            && at.y < horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT
    }
    fn build_ceiling(&self) -> f64 {
        f64::from(horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT)
    }
    fn ignite_cell(&mut self, p: DVec3) -> bool {
        self.game.arcana_ignite_cell(owner_uuid(self.owner), p)
    }
    fn can_move_owner(&self, _: DVec3, to: DVec3) -> bool {
        let Some(p) = self.game.player.as_ref() else {
            return false;
        };
        let mut scene = SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        scene
            .unobstructed(p.dimensions.bounds(native(to)))
            .unwrap_or(false)
    }
    fn teleport_owner(&mut self, to: DVec3) {
        self.game
            .teleport_to(crate::game::commands::Target::Player, to.to_array(), None);
    }
    fn set_owner_velocity(&mut self, v: DVec3) {
        if let Some(p) = self.game.player.as_mut() {
            p.velocity = native(v);
            p.health.sync_velocity = true;
            p.fall_distance = 0.0;
        }
    }
    fn random(&mut self) -> f64 {
        use horizon_core::random::RandomSource;
        self.game.world.random.next_double()
    }
    fn target(&self, id: u64) -> Option<Target> {
        self.hostile_targets(
            0,
            Bounds {
                min: DVec3::splat(-30_000_000.0),
                max: DVec3::splat(30_000_000.0),
            },
        )
        .into_iter()
        .find(|t| t.id == id)
    }
}
