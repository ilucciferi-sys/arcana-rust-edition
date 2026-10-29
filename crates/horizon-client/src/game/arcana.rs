//! Opt-in migration. Elemental awakening and combat run on the server.
//! PvP, necromantic awakening and legacy-save migration remain unavailable.
mod fire;
mod persistence;
mod rituals;
mod starters;
mod visuals;
use super::{Game, lan::LanHost};
use arcana_core::{
    Catalog, DVec3, School,
    casting::{CastEvent, CastPermission, Caster, Phase},
    network::{Action, MAX_PACKET, MagicSnapshot, PROTOCOL, Replica, ServerEvent},
    solar::{self, Bounds, CombatWorld, SolarOutcome, Target},
    state::MagicState,
};
use horizon_client::{
    collisions::SceneCollisions,
    mobs::{Damage, DamageKind, Who},
};
use horizon_core::{
    geometry::{Vec3, traverse_blocks},
    position::BlockPos,
};
use horizon_protocol::packets::{
    configuration::CustomPayload,
    play::{Clientbound as C, Serverbound as S},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub(super) const ACTION_CHANNEL: &str = "arcana:action_v1";
pub(super) const EVENT_CHANNEL: &str = "arcana:event_v1";
pub(super) const DEMO_CHANNEL: &str = "arcana:demo_v1";
pub(super) const BOOTSTRAP_CHANNEL: &str = "arcana:bootstrap_v1";
pub(super) const SNAPSHOT_CHANNEL: &str = "arcana:snapshot_v1";
pub(super) const STARTER_CHANNEL: &str = "arcana:starter_v1";
pub(super) const RITUAL_CHANNEL: &str = rituals::CHANNEL;
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog::bundled().expect("validated original Arcana catalog"))
}
#[derive(Default)]
struct Actor {
    state: MagicState,
    caster: Caster,
    tick: Option<u64>,
    dimension: String,
    dirty: bool,
    bolts: Vec<starters::ActiveBolt>,
    ritual: arcana_core::ritual::Tracker,
    notice: Option<(String, u64)>,
}
#[derive(Serialize, Deserialize)]
struct Wire {
    owner: u32,
    dimension: String,
    event: ServerEvent,
}
struct Playback {
    owner: u32,
    cast_id: u64,
    dimension: String,
    solar: SolarOutcome,
    age: f32,
    received: Instant,
}
pub(super) struct Runtime {
    actors: HashMap<u128, Actor>,
    outbox: Vec<Wire>,
    session: u64,
    sequence: u64,
    replica: Option<Replica>,
    playback: Option<Playback>,
    load_errors: HashMap<u128, String>,
    snapshot: Option<MagicSnapshot>,
    snapshot_received: Instant,
    bootstrap_requested: bool,
    starter_outbox: Vec<starters::StarterVisual>,
    starter_views: HashMap<(u32, u64), starters::Viewed>,
    ritual_outbox: Vec<rituals::Visual>,
    ritual_views: HashMap<u32, rituals::Viewed>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            actors: HashMap::new(),
            outbox: Vec::new(),
            session: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            sequence: 0,
            replica: None,
            playback: None,
            load_errors: HashMap::new(),
            snapshot: None,
            snapshot_received: Instant::now(),
            bootstrap_requested: false,
            starter_outbox: Vec::new(),
            starter_views: HashMap::new(),
            ritual_outbox: Vec::new(),
            ritual_views: HashMap::new(),
        }
    }
}
fn core(v: Vec3) -> DVec3 {
    DVec3::new(v.x, v.y, v.z)
}
fn native(v: DVec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

impl Game<'_> {
    pub(super) fn arcana_send(&mut self, action: Action) {
        if let Ok(data) = serde_json::to_vec(&action) {
            self.send(S::CustomPayload(CustomPayload {
                channel: ACTION_CHANNEL.into(),
                data,
            }));
        }
    }
    pub(super) fn arcana_send_demo(&mut self) {
        self.send(S::CustomPayload(CustomPayload {
            channel: DEMO_CHANNEL.into(),
            data: Vec::new(),
        }));
    }
    pub(super) fn arcana_grant_demo(&mut self) {
        let Some(uuid) = self.lan_acting_uuid else {
            return;
        };
        if !self.arcana_ensure_actor(uuid) {
            return;
        }
        let actor = self.arcana.actors.get_mut(&uuid).unwrap();
        // This is an explicit development grant, never an awakening substitute.
        let initial = actor.state.school().is_none();
        if initial {
            actor.state.awaken(School::Fire, catalog());
        }
        if actor.state.level() < 5 {
            actor.state.set_level(5, catalog());
        }
        actor.state.fill_mana();
        actor.state.clear_cooldowns();
        if initial {
            actor.state.select("solar_spear");
        }
        actor.dirty = true;
        self.arcana_save_actor(uuid);
        log::info!("Arcana pilot: operator granted level 5 demo resources");
    }
    pub(super) fn arcana_bootstrap(&mut self) {
        let Some(uuid) = self.lan_acting_uuid else {
            return;
        };
        let school = match std::env::var("ARCANA_DEMO_SCHOOL").as_deref() {
            Ok("fire") => School::Fire,
            Ok("water") => School::Water,
            Ok("earth") => School::Earth,
            Ok("air") => School::Air,
            _ => return,
        };
        if !self.arcana_ensure_actor(uuid) {
            return;
        }
        let actor = self.arcana.actors.get_mut(&uuid).unwrap();
        if actor.state.school().is_none() {
            actor.state.awaken(school, catalog());
            actor.state.set_level(5, catalog());
            actor.state.fill_mana();
            if school == School::Fire {
                actor.state.select("solar_spear");
            }
            actor.dirty = true;
            self.arcana_save_actor(uuid);
        }
    }
    fn arcana_context(&self) -> Option<(DVec3, DVec3)> {
        let (eye, direction) = self.aim()?;
        let direction = DVec3::from_array(direction);
        // Approximate the casting palm until the original skeletal clips are ported.
        let right = direction.cross(DVec3::Y).normalize_or_zero();
        Some((
            DVec3::from_array(eye) + direction * 0.55 + right * 0.3 - DVec3::Y * 0.3,
            direction,
        ))
    }
    fn arcana_available(&self) -> bool {
        self.player
            .as_ref()
            .is_some_and(|p| !p.health.dead_or_dying())
            && !self.spectator()
            && self.sleeping.is_none()
            && self.riding.is_none()
    }
    pub(super) fn arcana_action(&mut self, action: Action) {
        let Some(uuid) = self.lan_acting_uuid else {
            return;
        };
        if !self.arcana_ensure_actor(uuid) {
            return;
        }
        let mut actor = self.arcana.actors.remove(&uuid).unwrap();
        let tick = self.world.game_time.max(0) as u64;
        if matches!(action, Action::Sync) {
            if actor
                .tick
                .is_none_or(|last| tick.saturating_sub(last) >= 10)
            {
                self.arcana_send_snapshot(&actor, None);
            }
            self.arcana.actors.insert(uuid, actor);
            return;
        }
        let previous = actor.state.clone();
        let mut notice = None;
        match action {
            Action::Sync => {}
            Action::Cast { spell_id } => {
                let context = self.arcana_context();
                let permission = CastPermission {
                    available: self.arcana_available(),
                    ritual_active: actor.ritual.active().is_some(),
                    direction_valid: context.is_some_and(|(_, d)| {
                        d.is_finite() && (d.length_squared() - 1.0).abs() < 1e-5
                    }),
                };
                match actor
                    .caster
                    .request(&mut actor.state, catalog(), &spell_id, tick, permission)
                {
                    Ok(event) => {
                        actor.dimension = self.dim.id().into();
                        if let Some((start, direction)) = context {
                            if spell_id != "solar_spear" {
                                self.arcana_starter_visual(
                                    event.cast_id,
                                    &spell_id,
                                    start,
                                    direction,
                                    DVec3::ZERO,
                                    0,
                                    starters::VisualStage::Preparation,
                                );
                            } else {
                                let end =
                                    start + direction * catalog().get(&spell_id).unwrap().range;
                                self.arcana_emit(
                                    event,
                                    Some(SolarOutcome {
                                        caster: u64::from(self.lan_acting.unwrap_or(0)),
                                        start,
                                        end,
                                        impacted: false,
                                        xp_awarded: 0,
                                        beam_lifetime_ticks: 30,
                                    }),
                                );
                            }
                        }
                    }
                    Err(reason) => {
                        log::info!("Arcana cast rejected: {reason:?}");
                        notice = Some(
                            match reason {
                                arcana_core::casting::Rejection::Cooldown => {
                                    "Заклинание перезаряжается"
                                }
                                arcana_core::casting::Rejection::NoMana => "Недостаточно маны",
                                arcana_core::casting::Rejection::Busy => "Завершите текущий каст",
                                arcana_core::casting::Rejection::WrongSchool => {
                                    "Нужна другая школа магии"
                                }
                                arcana_core::casting::Rejection::Locked => {
                                    "Заклинание ещё не открыто"
                                }
                                arcana_core::casting::Rejection::UnsupportedMechanic => {
                                    "Это заклинание ещё переносится"
                                }
                                _ => "Сейчас применить заклинание нельзя",
                            }
                            .into(),
                        );
                    }
                }
            }
            Action::Cancel => {
                if let Some(e) = actor.caster.cancel(&mut actor.state, tick, false) {
                    self.arcana_emit(e, None);
                }
            }
            Action::Select { spell_id } => {
                if catalog().get(&spell_id).is_some_and(|s| s.executable()) {
                    actor.state.select(&spell_id);
                }
            }
        }
        actor.dirty |= actor.state != previous;
        let changed = actor.dirty;
        self.arcana_send_snapshot(&actor, notice);
        self.arcana.actors.insert(uuid, actor);
        if changed {
            self.arcana_save_actor(uuid);
        }
    }
    pub(super) fn arcana_tick(&mut self, alive: bool) {
        let Some(uuid) = self.lan_acting_uuid else {
            return;
        };
        if !self.arcana_ensure_actor(uuid) {
            return;
        }
        let mut actor = self.arcana.actors.remove(&uuid).unwrap();
        let tick = self.world.game_time.max(0) as u64;
        if actor.tick == Some(tick) {
            self.arcana.actors.insert(uuid, actor);
            return;
        }
        let first_tick = actor.tick.is_none();
        actor.tick = Some(tick);
        let mut checkpoint = false;
        let previous = actor.state.clone();
        checkpoint |= self.arcana_tick_ritual(&mut actor, alive);
        if !alive || !self.arcana_available() || actor.dimension != self.dim.id() {
            for active in actor.bolts.drain(..) {
                self.arcana_starter_visual(
                    active.bolt.id,
                    active.bolt.kind.spell_id(),
                    active.bolt.position,
                    DVec3::ZERO,
                    DVec3::ZERO,
                    active.bolt.age,
                    starters::VisualStage::Expired,
                );
            }
            if let Some(e) = actor.caster.cancel(&mut actor.state, tick, true) {
                self.arcana_emit(e, None);
                checkpoint = true;
            }
            actor.dimension = self.dim.id().into();
        } else {
            actor.state.tick(!actor.caster.idle());
            if let Ok(events) = actor.caster.advance(tick) {
                for event in events {
                    checkpoint = true;
                    let mut outcome = None;
                    if event.phase == Phase::Release
                        && let Some((start, direction)) = self.arcana_context()
                    {
                        if self.arcana_release_fire(&mut actor, &event, direction)
                            || self.arcana_release_starter(&mut actor, &event, start, direction)
                        {
                            continue;
                        }
                        let caster = u64::from(self.lan_acting.unwrap_or(0));
                        let owner = self.acting_player_uuid();
                        let mut world = PilotWorld { game: self, owner };
                        // Terrain destruction is intentionally unavailable in the pilot;
                        // the production adapter needs Arcana's breakSoft protection checks.
                        match solar::execute(
                            &mut world,
                            caster,
                            catalog().get(&event.spell_id).unwrap(),
                            start,
                            direction,
                            false,
                        ) {
                            Ok(result) => {
                                actor.state.add_xp(result.xp_awarded, catalog());
                                log::info!(
                                    "Arcana solar release: impact={}, end={:?}, xp={}",
                                    result.impacted,
                                    result.end,
                                    result.xp_awarded
                                );
                                outcome = Some(result);
                            }
                            Err(e) => log::warn!("Arcana solar world rejected release: {e}"),
                        }
                    }
                    self.arcana_emit(event, outcome);
                }
            }
            self.arcana_tick_starters(&mut actor, tick);
        }
        actor.dirty |= actor.state != previous || checkpoint;
        if first_tick || tick.is_multiple_of(10) || checkpoint {
            self.arcana_send_snapshot(&actor, None);
        }
        self.arcana.actors.insert(uuid, actor);
        if checkpoint || tick.is_multiple_of(100) {
            self.arcana_save_actor(uuid);
        }
    }
    fn arcana_send_snapshot(&mut self, actor: &Actor, notice: Option<String>) {
        let tick = self.world.game_time.max(0) as u64;
        let snapshot = MagicSnapshot {
            protocol: PROTOCOL,
            server_tick: tick,
            state: actor.state.clone(),
            cast: actor.caster.status(tick),
            ritual: actor.ritual.active().cloned(),
            notice: notice.or_else(|| {
                actor
                    .notice
                    .as_ref()
                    .filter(|(_, until)| tick <= *until)
                    .map(|(text, _)| text.clone())
            }),
        };
        if let Ok(data) = serde_json::to_vec(&snapshot) {
            self.guest_packets.push(C::CustomPayload(CustomPayload {
                channel: SNAPSHOT_CHANNEL.into(),
                data,
            }));
        }
    }
    pub(super) fn arcana_receive_snapshot(&mut self, data: &[u8]) {
        if let Ok(snapshot) = MagicSnapshot::decode(data, catalog()) {
            self.arcana.snapshot = Some(snapshot);
            self.arcana.snapshot_received = Instant::now();
        }
    }
    pub(super) fn arcana_client_tick(&mut self) {
        self.arcana.ritual_views.retain(|_, v| {
            v.received.elapsed().as_secs_f32() < if v.message.finished { 1.5 } else { 1.0 }
        });
        self.arcana.starter_views.retain(|_, v| {
            v.received.elapsed().as_secs_f32()
                < match v.message.stage {
                    starters::VisualStage::FirePush => 4.0,
                    starters::VisualStage::Ignition => 1.2,
                    starters::VisualStage::FireballImpact => 1.8,
                    _ => 1.0,
                }
        });
        if self.remote.is_none() || self.player.is_none() || self.remote_loading() {
            return;
        }
        if !self.arcana.bootstrap_requested && std::env::var_os("ARCANA_DEMO_SCHOOL").is_some() {
            self.send(S::CustomPayload(CustomPayload {
                channel: BOOTSTRAP_CHANNEL.into(),
                data: Vec::new(),
            }));
            self.arcana.bootstrap_requested = true;
        }
        if self.arcana.snapshot.is_none() && self.ticks.is_multiple_of(20) {
            self.arcana_send(Action::Sync);
        }
    }
    pub(super) fn arcana_cast_selected(&mut self) {
        let selected = self
            .arcana
            .snapshot
            .as_ref()
            .and_then(|s| s.state.selected())
            .unwrap_or("solar_spear")
            .to_owned();
        self.arcana_send(Action::Cast { spell_id: selected });
    }
    pub(super) fn arcana_next_spell(&mut self) {
        let Some(snapshot) = &self.arcana.snapshot else {
            return;
        };
        let available: Vec<_> = catalog()
            .spells()
            .iter()
            .filter(|s| s.executable() && snapshot.state.equipped(&s.id))
            .map(|s| s.id.as_str())
            .collect();
        if available.is_empty() {
            return;
        }
        let index = available
            .iter()
            .position(|s| Some(*s) == snapshot.state.selected())
            .map_or(0, |i| (i + 1) % available.len());
        self.arcana_send(Action::Select {
            spell_id: available[index].into(),
        });
    }
    pub(super) fn arcana_status(&self) -> serde_json::Value {
        let projectiles = self.integrated.as_ref().and_then(|server| {
            server.call(|game| {
                game.arcana
                    .actors
                    .values()
                    .map(|a| a.bolts.len())
                    .sum::<usize>()
            })
        });
        serde_json::json!({"snapshot":self.arcana.snapshot,"blocked_players":self.arcana.load_errors.len(),"ritual_visuals":self.arcana.ritual_views.len(),"spell_visuals":self.arcana.starter_views.values().map(|v| &v.message).collect::<Vec<_>>(),"server_projectiles":projectiles})
    }
    pub(super) fn arcana_hud(&self, hud: &mut horizon_client::hud::HudBatch) {
        if self.screen != super::Screen::Playing || self.ui.hide_gui {
            return;
        }
        let Some(snapshot) = &self.arcana.snapshot else {
            return;
        };
        let Some(school) = snapshot.state.school() else {
            self.arcana_ritual_hud(hud, snapshot);
            return;
        };
        let school_name = match school {
            School::Fire => "Огонь",
            School::Water => "Вода",
            School::Earth => "Земля",
            School::Air => "Воздух",
            School::Necromancy => "Некромантия",
        };
        let color = match school {
            School::Fire => horizon_client::hud::rgba(255, 150, 55, 255),
            School::Water => horizon_client::hud::rgba(75, 200, 245, 255),
            School::Earth => horizon_client::hud::rgba(196, 169, 100, 255),
            School::Air => horizon_client::hud::rgba(215, 224, 232, 255),
            School::Necromancy => horizon_client::hud::rgba(70, 155, 90, 255),
        };
        let atlas = &self.gui.atlas;
        let hud_start = hud.vertices.len();
        let w = 260.0f32.min(hud.width - 16.0);
        hud.fill(
            atlas,
            7.0,
            7.0,
            w,
            65.0,
            horizon_client::hud::rgba(10, 15, 23, 205),
        );
        hud.fill(atlas, 7.0, 7.0, 2.0, 65.0, color);
        hud.text(
            atlas,
            &format!(
                "{school_name} · ур. {} · XP {}",
                snapshot.state.level(),
                snapshot.state.xp()
            ),
            14.0,
            12.0,
            color,
            true,
        );
        hud.fill(
            atlas,
            14.0,
            25.0,
            w - 14.0,
            4.0,
            horizon_client::hud::rgba(30, 45, 65, 255),
        );
        hud.fill(
            atlas,
            14.0,
            25.0,
            (w - 14.0) * (snapshot.state.mana() / snapshot.state.max_mana()) as f32,
            4.0,
            color,
        );
        let selected = snapshot.state.selected().unwrap_or("");
        let name = catalog()
            .get(selected)
            .map_or("Выберите заклинание", |s| s.name.as_str());
        hud.text(
            atlas,
            &format!(
                "Мана {:.0}/{:.0} · {name}",
                snapshot.state.mana(),
                snapshot.state.max_mana()
            ),
            14.0,
            33.0,
            u32::MAX,
            true,
        );
        let ticks = snapshot
            .state
            .cooldown(selected)
            .saturating_sub((self.arcana.snapshot_received.elapsed().as_secs_f32() * 20.0) as u32);
        let status = if let Some(cast) = &snapshot.cast {
            let spell = catalog().get(&cast.spell_id);
            let age = snapshot.server_tick.saturating_sub(cast.start_tick) as f32
                + self.arcana.snapshot_received.elapsed().as_secs_f32() * 20.0;
            if let Some(s) = spell
                && age < s.cast_time as f32
            {
                format!("Подготовка {:.1} с", (s.cast_time as f32 - age) / 20.0)
            } else {
                "Восстановление".into()
            }
        } else if ticks > 0 {
            format!("КД {:.1} с", ticks as f32 / 20.0)
        } else {
            "Готово".into()
        };
        hud.text(
            atlas,
            &format!("V — каст · R — выбор · {status}"),
            14.0,
            46.0,
            horizon_client::hud::rgba(195, 207, 221, 255),
            true,
        );
        if let Some(notice) = &snapshot.notice {
            hud.text(
                atlas,
                notice,
                14.0,
                59.0,
                horizon_client::hud::rgba(255, 190, 120, 255),
                true,
            );
        }
        hud.scale_since(hud_start, 7.0, 7.0, 0.65, 0.65);
    }
    fn arcana_emit(&mut self, cast: CastEvent, solar: Option<SolarOutcome>) {
        self.arcana.sequence += 1;
        self.arcana.outbox.push(Wire {
            owner: self.lan_acting.unwrap_or(0),
            dimension: self.dim.id().into(),
            event: ServerEvent {
                protocol: PROTOCOL,
                session: self.arcana.session,
                event_id: self.arcana.sequence,
                cast,
                solar,
            },
        });
    }
    pub(super) fn arcana_flush(&mut self, host: &LanHost) {
        for visual in self.arcana.ritual_outbox.drain(..) {
            if let Ok(data) = serde_json::to_vec(&visual) {
                let packet = C::CustomPayload(CustomPayload {
                    channel: rituals::CHANNEL.into(),
                    data,
                });
                for guest in host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.placed && g.dimension == visual.rite.dimension)
                {
                    guest.send(&packet);
                }
            }
        }
        for visual in self.arcana.starter_outbox.drain(..) {
            if let Ok(data) = serde_json::to_vec(&visual) {
                let packet = C::CustomPayload(CustomPayload {
                    channel: STARTER_CHANNEL.into(),
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
        for wire in self.arcana.outbox.drain(..) {
            if let Ok(data) = serde_json::to_vec(&wire) {
                let packet = C::CustomPayload(CustomPayload {
                    channel: EVENT_CHANNEL.into(),
                    data,
                });
                for guest in host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.placed && g.dimension == wire.dimension)
                {
                    guest.send(&packet);
                }
            }
        }
    }
    pub(super) fn arcana_receive(&mut self, data: &[u8]) {
        if data.len() > MAX_PACKET {
            return;
        }
        let Ok(wire) = serde_json::from_slice::<Wire>(data) else {
            return;
        };
        let Ok(bytes) = serde_json::to_vec(&wire.event) else {
            return;
        };
        if ServerEvent::decode(&bytes).is_err() {
            return;
        }
        let replica = self
            .arcana
            .replica
            .get_or_insert_with(|| Replica::new(wire.event.session));
        if !replica.accept(&wire.event) {
            return;
        }
        let cast = &wire.event.cast;
        if cast.phase == Phase::Cancelled {
            self.arcana
                .starter_views
                .remove(&(wire.owner, cast.cast_id));
            if self
                .arcana
                .playback
                .as_ref()
                .is_some_and(|p| p.owner == wire.owner && p.cast_id == cast.cast_id)
            {
                self.arcana.playback = None;
            }
        } else if let Some(solar) = wire.event.solar {
            self.arcana.playback = Some(Playback {
                owner: wire.owner,
                cast_id: cast.cast_id,
                dimension: wire.dimension,
                solar,
                age: (cast.server_tick - cast.start_tick) as f32 / 20.0,
                received: Instant::now(),
            });
        }
    }
    pub(super) fn arcana_lance(&self, camera: glam::DVec3) -> Option<horizon_vfx::Lance> {
        let p = self.arcana.playback.as_ref()?;
        if p.dimension != self.dim.id() {
            return None;
        }
        let quality = match std::env::var("ARCANA_QUALITY").as_deref() {
            Ok("performance") => horizon_vfx::Quality::Performance,
            Ok("cinematic") => horizon_vfx::Quality::Cinematic,
            _ => horizon_vfx::Quality::High,
        };
        let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
        let effect = horizon_vfx::Lance {
            start: (p.solar.start - camera).as_vec3(),
            end: (p.solar.end - camera).as_vec3(),
            age_seconds: p.age + p.received.elapsed().as_secs_f32(),
            preparation_seconds: 3.5,
            impacted: p.solar.impacted,
            quality,
            accessibility: if reduced {
                horizon_vfx::Accessibility {
                    flashes: 0.0,
                    camera_shake: 0.0,
                    fov_pulse: 0.0,
                }
            } else {
                Default::default()
            },
        };
        effect.visible().then_some(effect)
    }
}
struct PilotWorld<'g, 'r> {
    game: &'g mut Game<'r>,
    owner: [i32; 4],
}
impl CombatWorld for PilotWorld<'_, '_> {
    fn clip_blocks(&self, start: DVec3, end: DVec3) -> Result<Option<DVec3>, String> {
        let delta = end - start;
        for step in 0..=(delta.length().ceil() as u32 * 2) {
            let p = start + delta * (f64::from(step) / (delta.length().ceil() * 2.0).max(1.0));
            if !self
                .game
                .world
                .chunks
                .contains_key(&((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4))
            {
                return Err("unloaded beam segment".into());
            }
        }
        let from = native(start);
        let to = native(end);
        let mut scene = SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        let result = traverse_blocks(from, to, 1024, |p: BlockPos| {
            let state = self.game.world.block(p.x, p.y, p.z);
            if self.game.res.facts().is_air(state) {
                return None;
            }
            self.game
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?
                .clip(from, to, p)
        })
        .map_err(|e| format!("beam traversal: {e:?}"))?;
        Ok(result.map(|h| core(h.location)))
    }
    fn hostile_targets(&self, _: u64, bounds: Bounds) -> Vec<Target> {
        let allies = self.game.player_allies();
        self.game
            .mobs
            .iter()
            .filter(|m| m.alive() && !allies.contains(&m.id) && m.extra.owner != Some(self.owner))
            .filter_map(|m| {
                let a = m.bounds();
                let b = Bounds {
                    min: core(a.min_position()),
                    max: core(a.max_position()),
                };
                if b.max.cmplt(bounds.min).any() || b.min.cmpgt(bounds.max).any() {
                    return None;
                }
                Some(Target {
                    id: u64::from(m.id),
                    bounds: b,
                    position: core(m.position),
                    knockback_immune: false,
                })
            })
            .collect()
    }
    fn damage(&mut self, _: u64, target: u64, amount: f64) -> bool {
        let from = self
            .game
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        let Some(m) = self
            .game
            .mobs
            .iter_mut()
            .find(|m| u64::from(m.id) == target)
        else {
            return false;
        };
        m.damage(
            amount as f32,
            Damage {
                kind: DamageKind::Magic,
                attacker: Some(Who::Player),
                from,
            },
        )
    }
    fn ignite(&mut self, target: u64, ticks: u32) {
        if let Some(m) = self
            .game
            .mobs
            .iter_mut()
            .find(|m| u64::from(m.id) == target)
        {
            m.fire_ticks = ticks as i32;
        }
    }
    fn impulse(&mut self, target: u64, v: DVec3) {
        if let Some(m) = self
            .game
            .mobs
            .iter_mut()
            .find(|m| u64::from(m.id) == target)
        {
            m.velocity = Vec3::new(m.velocity.x + v.x, m.velocity.y + v.y, m.velocity.z + v.z);
            m.needs_sync = true;
        }
    }
    fn break_soft_sphere(&mut self, _: u64, _: DVec3, _: f64) {
        unreachable!("terrain modification is disabled in the migration pilot");
    }
}
