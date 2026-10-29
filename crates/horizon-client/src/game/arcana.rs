//! Opt-in migration pilot. The demo grant is operator-only. Rituals, PvP,
//! world modification and legacy-save migration are not enabled in this slice.
use super::{Game, lan::LanHost};
use arcana_core::{
    Catalog, DVec3, School,
    casting::{CastEvent, CastPermission, Caster, Phase},
    network::{Action, MAX_PACKET, PROTOCOL, Replica, ServerEvent},
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
        let actor = self.arcana.actors.entry(uuid).or_default();
        // This is an explicit development grant, never an awakening substitute.
        if actor.state.school().is_none() {
            actor.state.awaken(School::Fire, catalog());
        }
        actor.state.set_level(5, catalog());
        actor.state.fill_mana();
        actor.state.clear_cooldowns();
        log::info!("Arcana pilot: operator granted level 5 demo resources");
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
        let mut actor = self.arcana.actors.remove(&uuid).unwrap_or_default();
        let tick = self.world.game_time.max(0) as u64;
        match action {
            Action::Cast { spell_id } => {
                let context = self.arcana_context();
                let permission = CastPermission {
                    available: self.arcana_available(),
                    ritual_active: false,
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
                            let end = start + direction * catalog().get(&spell_id).unwrap().range;
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
                    Err(reason) => log::info!("Arcana pilot cast rejected: {reason:?}"),
                }
            }
            Action::Cancel => {
                if let Some(e) = actor.caster.cancel(&mut actor.state, tick, false) {
                    self.arcana_emit(e, None);
                }
            }
            Action::Select { spell_id } => {
                actor.state.select(&spell_id);
            }
        }
        self.arcana.actors.insert(uuid, actor);
    }
    pub(super) fn arcana_tick(&mut self, alive: bool) {
        let Some(uuid) = self.lan_acting_uuid else {
            return;
        };
        let Some(mut actor) = self.arcana.actors.remove(&uuid) else {
            return;
        };
        let tick = self.world.game_time.max(0) as u64;
        if actor.tick == Some(tick) {
            self.arcana.actors.insert(uuid, actor);
            return;
        }
        actor.tick = Some(tick);
        if !alive || !self.arcana_available() || actor.dimension != self.dim.id() {
            if let Some(e) = actor.caster.cancel(&mut actor.state, tick, true) {
                self.arcana_emit(e, None);
            }
        } else {
            actor.state.tick(!actor.caster.idle());
            if let Ok(events) = actor.caster.advance(tick) {
                for event in events {
                    let mut outcome = None;
                    if event.phase == Phase::Release
                        && let Some((start, direction)) = self.arcana_context()
                    {
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
        }
        self.arcana.actors.insert(uuid, actor);
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
            .filter(|m| {
                m.alive()
                    && !allies.contains(&m.id)
                    && m.extra.owner != Some(self.owner)
                    && !m.invulnerable_flag
            })
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
