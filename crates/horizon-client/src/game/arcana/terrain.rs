//! ArcaneFire is an owned server cell overlay: no collision, persistent owner,
//! protected placement, wet/rain/support removal, fire contact and aging.
//! Horizon's immutable vanilla registry is kept intact.
use super::*;
use arcana_core::{
    fire_school::FireSchoolWorld,
    fire_terrain::{self, OwnedFire, Position, Terrain},
};
use horizon_core::random::RandomSource;
use std::{fs, io::Write, path::PathBuf};
pub(in crate::game) const CHANNEL: &str = "arcana:owned_fire_v1";
#[derive(Clone)]
pub(super) struct Policy {
    pub build: bool,
    pub spawn: Option<([i32; 3], i32)>,
    pub fire_position: Option<DVec3>,
}
#[derive(Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    fire_ignites: bool,
    spell_griefing: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            fire_ignites: true,
            spell_griefing: true,
        }
    }
}
pub(super) struct State {
    pub fire: OwnedFire,
    pub dirty: bool,
    needs_sync: bool,
    last_tick: Option<u64>,
    last_sent: u64,
    pub failed: bool,
    path: PathBuf,
    config: Config,
}
impl State {
    fn load(game: &Game<'_>) -> Self {
        let folder = game.world_root.join("arcana");
        let path = folder.join(format!(
            "owned-fire-{}.json",
            game.dim.id().replace(':', "_")
        ));
        let mut failed = false;
        let fire = match fs::read_to_string(&path) {
            Ok(s) => match OwnedFire::load_json(&s) {
                Ok(f) => f,
                Err(e) => {
                    log::error!("Arcana fire journal: {e}");
                    failed = true;
                    OwnedFire::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => OwnedFire::default(),
            Err(e) => {
                log::error!("Arcana fire journal: {e}");
                failed = true;
                OwnedFire::default()
            }
        };
        let config = match fs::read_to_string(folder.join("fire-config.json")) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                log::error!("Arcana fire config: {e}");
                Config {
                    fire_ignites: false,
                    spell_griefing: false,
                }
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => {
                log::error!("Arcana fire config: {e}");
                Config {
                    fire_ignites: false,
                    spell_griefing: false,
                }
            }
        };
        Self {
            fire,
            dirty: false,
            needs_sync: true,
            last_tick: None,
            last_sent: 0,
            failed,
            path,
            config,
        }
    }
    pub fn save(&mut self) -> std::io::Result<()> {
        if !self.dirty || self.failed {
            return Ok(());
        }
        fs::create_dir_all(self.path.parent().unwrap())?;
        let temporary = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&temporary)?;
        file.write_all(self.fire.save_json()?.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(temporary, &self.path)?;
        self.dirty = false;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Page {
    dimension: String,
    tick: u64,
    part: u16,
    parts: u16,
    cells: Vec<Position>,
}
pub(super) struct Viewed {
    pub cells: Vec<Position>,
    pub received: Instant,
    tick: u64,
    pending: HashMap<u16, Vec<Position>>,
}
struct World<'a, 'r> {
    game: &'a mut Game<'r>,
    config: Config,
}
impl Terrain for World<'_, '_> {
    fn block(&self, [x, y, z]: Position) -> Option<fire_terrain::Block> {
        if !(horizon_client::chunk::MIN_Y
            ..horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT)
            .contains(&y)
            || !self.game.world.chunks.contains_key(&(x >> 4, z >> 4))
            || !self
                .game
                .border
                .contains(f64::from(x) + 0.5, f64::from(z) + 0.5)
        {
            return None;
        }
        let state = self.game.world.block(x, y, z);
        let f = self.game.res.facts().get(state);
        Some(fire_terrain::Block {
            air: self.game.res.facts().is_air(state),
            fluid: f.fluid != horizon_client::blocks::Fluid::None,
            fire: matches!(f.name.as_ref(), "minecraft:fire" | "minecraft:soul_fire"),
            flammable: self.game.res.behaviours().data(state).burn_odds > 0,
        })
    }
    fn allowed(&self, owner: u128, at: Position) -> bool {
        let policy = self.game.arcana.school.builders.get(&owner);
        policy.is_some_and(|p| {
            p.build
                && p.spawn.is_none_or(|(spawn, radius)| {
                    at[0].abs_diff(spawn[0]).max(at[2].abs_diff(spawn[2])) > radius as u32
                })
        }) && self.block(at).is_some()
    }
    fn raining(&self, at: Position) -> bool {
        self.game.raining_at(BlockPos::new(at[0], at[1], at[2]))
    }
    fn fire_tick(&self, at: Position) -> bool {
        let radius = self
            .game
            .ui
            .rules
            .int("minecraft:fire_spread_radius_around_player");
        // Same corner distance and spectator exclusion as LevelWorld's
        // ServerLevel.canSpreadFireAround adapter.
        let origin = DVec3::new(f64::from(at[0]), f64::from(at[1]), f64::from(at[2]));
        radius == -1
            || self.game.arcana.school.builders.values().any(|p| {
                p.fire_position.is_some_and(|position| {
                    position.distance_squared(origin) < f64::from(radius) * f64::from(radius)
                })
            })
    }
    fn owner_present(&self, owner: u128) -> bool {
        self.game.arcana.school.builders.contains_key(&owner)
    }
    fn burn_flammable(&mut self, owner: u128, at: Position) -> bool {
        if !self.config.fire_ignites || !self.allowed(owner, at) {
            return false;
        }
        let f = self
            .game
            .res
            .facts()
            .get(self.game.world.block(at[0], at[1], at[2]));
        if f.block_entity || !self.block(at).is_some_and(|b| b.flammable) {
            return false;
        }
        let air = self.game.world.air;
        self.game.place_state(at, air, false).unwrap_or(false)
    }
    fn one_in_three(&mut self) -> bool {
        self.game.world.random.next_int_bound(3) == Ok(0)
    }
}
impl Game<'_> {
    pub(super) fn arcana_pick_fire(&self) -> Option<Position> {
        let (eye, direction) = self.aim()?;
        let start = DVec3::from_array(eye);
        let end = start + DVec3::from_array(direction) * self.reach();
        let fire = self.arcana.school.public_fire.get(self.dim.id())?;
        if fire.received.elapsed().as_secs_f32() > 2.5 {
            return None;
        }
        fire.cells
            .iter()
            .filter_map(|at| {
                let min = DVec3::new(f64::from(at[0]), f64::from(at[1]), f64::from(at[2]));
                (Bounds {
                    min,
                    max: min + DVec3::ONE,
                })
                .intersect(start, end)
                .map(|hit| (*at, hit.distance_squared(start)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(at, _)| at)
    }
    pub(super) fn arcana_extinguish(&mut self, at: Position) {
        if !self.arcana_available() {
            return;
        }
        let Some(eye) = self.eye_position().map(DVec3::from_array) else {
            return;
        };
        let point = DVec3::new(
            f64::from(at[0]) + 0.5,
            f64::from(at[1]) + 0.5,
            f64::from(at[2]) + 0.5,
        );
        if point.distance_squared(eye) > (self.reach() + 0.87).powi(2) {
            return;
        }
        let owner = self.acting_player_uuid();
        if !matches!(
            (PilotWorld { game: self, owner }).clip_blocks(eye, point),
            Ok(None)
        ) {
            return;
        }
        let mut state = self.arcana_take_fire();
        if (World {
            game: self,
            config: Config::default(),
        })
        .allowed(owner_uuid(owner), at)
            && state.fire.extinguish(at)
        {
            state.dirty = true;
            state.needs_sync = true;
        }
        self.arcana
            .school
            .terrain
            .insert(self.dim.id().into(), state);
    }
    pub(in crate::game) fn arcana_prepare_school(&mut self, host: &LanHost) {
        self.arcana_prepare_pvp(host);
        self.arcana.school.builders.clear();
        for guest in host.hub.guests.iter().filter(|g| g.placed && !g.dead) {
            let protected = self.options.dedicated.is_some()
                && host.hub.spawn_protection > 0
                && !host.hub.ops.is_empty()
                && !host.hub.ops.contains_key(&guest.uuid)
                && guest.dimension == "minecraft:overworld";
            self.arcana.school.builders.insert(
                guest.uuid,
                Policy {
                    build: matches!(guest.game_mode, 0 | 1),
                    fire_position: (!guest.spectator() && guest.dimension == self.dim.id())
                        .then(|| DVec3::from_array(guest.position)),
                    spawn: if protected {
                        self.level
                            .world_spawn
                            .map(|s| (s, host.hub.spawn_protection))
                    } else {
                        None
                    },
                },
            );
        }
    }
    fn arcana_take_fire(&mut self) -> State {
        self.arcana
            .school
            .terrain
            .remove(self.dim.id())
            .unwrap_or_else(|| State::load(self))
    }
    pub(super) fn arcana_ignite_cell(&mut self, owner: u128, point: DVec3) -> bool {
        if !point.is_finite() {
            return false;
        }
        let tick = self.world.game_time.max(0) as u64;
        let mut state = self.arcana_take_fire();
        let result = if state.config.fire_ignites && !state.failed {
            state.fire.ignite(
                &World {
                    game: self,
                    config: Config {
                        fire_ignites: state.config.fire_ignites,
                        spell_griefing: state.config.spell_griefing,
                    },
                },
                owner,
                point.floor().as_ivec3().to_array(),
                tick,
            )
        } else {
            false
        };
        if result {
            self.arcana_terrain_reward();
            state.dirty = true;
            state.needs_sync = true;
            self.arcana.school.own_fire_until.insert(owner, tick + 160);
        }
        self.arcana
            .school
            .terrain
            .insert(self.dim.id().into(), state);
        result
    }
    pub(super) fn arcana_ignite_patch(&mut self, owner: u128, at: DVec3, radius: f64) -> u32 {
        let mut placed = 0;
        let p = at.floor();
        for x in -(radius as i32)..=radius as i32 {
            for z in -(radius as i32)..=radius as i32 {
                if f64::from(x * x + z * z) > radius * radius {
                    continue;
                }
                if let Ok(point) = (PilotWorld {
                    game: self,
                    owner: owner_words(owner),
                })
                .surface(p + DVec3::new(f64::from(x), 0.0, f64::from(z)))
                {
                    placed += u32::from(self.arcana_ignite_cell(owner, point));
                }
            }
        }
        placed
    }
    pub(super) fn arcana_tick_fire_terrain(&mut self) {
        let tick = self.world.game_time.max(0) as u64;
        let mut state = self.arcana_take_fire();
        if state.last_tick != Some(tick) && !state.failed {
            state.last_tick = Some(tick);
            let config = Config {
                fire_ignites: state.config.fire_ignites,
                spell_griefing: state.config.spell_griefing,
            };
            let changed = state.fire.tick(&mut World { game: self, config }, tick);
            state.dirty |= changed;
            state.needs_sync |= changed;
            for mob in &mut self.mobs {
                let b = mob.bounds();
                let bounds = Bounds {
                    min: core(b.min_position()),
                    max: core(b.max_position()),
                };
                if !state.fire.touching(bounds).is_empty() {
                    mob.fire_ticks = mob.fire_ticks.max(160);
                    mob.damage(1.0, Damage::of(DamageKind::InFire));
                }
            }
        }
        let owner = owner_uuid(self.acting_player_uuid());
        if let Some(p) = self.player.as_ref() {
            let b = p.bounds();
            let bounds = Bounds {
                min: core(b.min_position()),
                max: core(b.max_position()),
            };
            let feet = core(p.position).floor().as_ivec3().to_array();
            if state.fire.protects(owner, feet) {
                self.arcana.school.own_fire_until.insert(owner, tick + 160);
            }
            if !state.fire.touching(bounds).is_empty() {
                let immune = self
                    .arcana
                    .school
                    .own_fire_until
                    .get(&owner)
                    .is_some_and(|u| *u > tick);
                if let Some(p) = self.player.as_mut() {
                    p.arcana_fire_immune |= immune;
                    if !immune {
                        p.fire_ticks = p.fire_ticks.max(160);
                    }
                }
                if !immune {
                    self.hurt_player_by("minecraft:in_fire", 1.0);
                }
            }
        }
        self.arcana
            .school
            .terrain
            .insert(self.dim.id().into(), state);
    }
    pub(super) fn arcana_fire_terrain_flush(&mut self, host: &LanHost) {
        let tick = self.world.game_time.max(0) as u64;
        let Some(state) = self.arcana.school.terrain.get_mut(self.dim.id()) else {
            return;
        };
        if state.needs_sync || tick.saturating_sub(state.last_sent) >= 20 {
            state.needs_sync = false;
            state.last_sent = tick;
            let cells: Vec<_> = state.fire.cells().map(|c| c.at).collect();
            let parts = cells.len().div_ceil(128).max(1) as u16;
            for part in 0..parts {
                let start = usize::from(part) * 128;
                let end = (start + 128).min(cells.len());
                let page = Page {
                    dimension: self.dim.id().into(),
                    tick,
                    part,
                    parts,
                    cells: cells[start..end].to_vec(),
                };
                if let Ok(data) = serde_json::to_vec(&page) {
                    let packet = C::CustomPayload(CustomPayload {
                        channel: CHANNEL.into(),
                        data,
                    });
                    for g in host
                        .hub
                        .guests
                        .iter()
                        .filter(|g| g.placed && g.dimension == self.dim.id())
                    {
                        g.send(&packet);
                    }
                }
            }
        }
        if tick.is_multiple_of(100)
            && let Err(e) = state.save()
        {
            log::error!("Arcana owned fire save: {e}");
        }
    }
    pub(in crate::game) fn arcana_receive_fire_terrain(&mut self, bytes: &[u8]) {
        if bytes.len() > MAX_PACKET {
            return;
        }
        let Ok(page) = serde_json::from_slice::<Page>(bytes) else {
            return;
        };
        if page.parts == 0
            || page.parts > 32
            || page.part >= page.parts
            || page.cells.len() > 128
            || page
                .cells
                .iter()
                .flatten()
                .any(|v| v.unsigned_abs() > 30_000_000)
        {
            return;
        }
        let view = self
            .arcana
            .school
            .public_fire
            .entry(page.dimension)
            .or_insert_with(|| Viewed {
                cells: vec![],
                received: Instant::now(),
                tick: page.tick,
                pending: HashMap::new(),
            });
        if page.tick < view.tick {
            return;
        }
        if page.tick != view.tick {
            view.tick = page.tick;
            view.pending.clear();
        }
        view.pending.insert(page.part, page.cells);
        if view.pending.len() == usize::from(page.parts) {
            view.cells = view.pending.values().flatten().copied().collect();
            view.received = Instant::now();
        }
    }
    pub(super) fn arcana_break_soft(&mut self, owner: u128, at: DVec3, radius: f64) {
        let state = self.arcana_take_fire();
        let enabled = state.config.spell_griefing && !state.failed;
        self.arcana
            .school
            .terrain
            .insert(self.dim.id().into(), state);
        if !enabled {
            return;
        }
        let center = at.floor().as_ivec3();
        let r = radius.min(6.0) as i32;
        for x in -r..=r {
            for y in -r..=r {
                for z in -r..=r {
                    if x * x + y * y + z * z > r * r {
                        continue;
                    }
                    let p = (center + glam::IVec3::new(x, y, z)).to_array();
                    if !(World {
                        game: self,
                        config: Config::default(),
                    })
                    .allowed(owner, p)
                    {
                        continue;
                    }
                    let state = self.world.block(p[0], p[1], p[2]);
                    let f = self.res.facts().get(state);
                    if self.res.facts().is_air(state)
                        || f.hardness < 0.0
                        || f.hardness > 5.0
                        || f.block_entity
                    {
                        continue;
                    }
                    if let Err(e) = self.destroy_block_dropping(p) {
                        log::warn!("Arcana breakSoft: {e:?}");
                    }
                }
            }
        }
    }
}
