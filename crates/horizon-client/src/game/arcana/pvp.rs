//! Players live in LanHost, outside the current as_guest borrow. Queue their
//! operations, resolve them through the normal player combat path after that
//! borrow ends, then award XP only for the confirmed result.
use super::*;
use arcana_core::starter::DamageStyle;
use std::collections::HashSet;
const PLAYER: u64 = 1 << 48;
#[derive(Clone)]
pub(super) struct Player {
    uuid: u128,
    conn: u32,
    name: String,
    dimension: String,
    target: Target,
    eligible: bool,
}
#[derive(Clone, Copy)]
pub(super) enum Reward {
    Once,
    PerHit,
    Stable,
}
#[derive(Clone, Copy)]
pub(super) struct Rule {
    pub reward: Reward,
    pub gated: bool,
    pub terrain: bool,
    pub force_reward: bool,
}
impl Rule {
    pub fn once() -> Self {
        Self {
            reward: Reward::Once,
            gated: false,
            terrain: false,
            force_reward: true,
        }
    }
    pub fn per_hit() -> Self {
        Self {
            reward: Reward::PerHit,
            gated: false,
            terrain: false,
            force_reward: false,
        }
    }
    pub fn stable() -> Self {
        Self {
            reward: Reward::Stable,
            gated: false,
            terrain: false,
            force_reward: false,
        }
    }
}
#[derive(Clone)]
pub(super) enum Operation {
    Damage(f64, DamageStyle),
    Ignite(u32, bool),
    Clear,
    Slow(u32),
    Impulse(DVec3),
}
struct Pending {
    target: Player,
    operation: Operation,
}
struct Group {
    owner: u128,
    owner_name: String,
    dimension: String,
    from: [f64; 3],
    rule: Rule,
    core_xp: u32,
    mob_damage: u32,
    mob_force: bool,
    terrain: bool,
    operations: Vec<Pending>,
}
#[derive(Default)]
pub(super) struct Runtime {
    players: Vec<Player>,
    current: Option<Group>,
    pending: Vec<Group>,
}

impl Game<'_> {
    pub(in crate::game) fn arcana_reflect_strike(
        &mut self,
        host: &mut LanHost,
        victim: super::super::lan_pvp::Victim,
        attacker: Option<u32>,
    ) {
        use super::super::lan_pvp::Victim;
        let (shielded, name) = match victim {
            Victim::Host => (
                self.player.as_ref().is_some_and(|p| p.arcana_shield),
                self.acting_player_name().to_owned(),
            ),
            Victim::Guest(conn) => host.hub.guest(conn).map_or((false, String::new()), |g| {
                (
                    g.motion.as_ref().is_some_and(|p| p.arcana_shield) && !g.creative(),
                    g.name.clone(),
                )
            }),
        };
        if !shielded || !self.ui.rules.bool("pvp") {
            return;
        }
        let source_name = attacker
            .and_then(|conn| host.hub.guest(conn))
            .map(|g| g.name.as_str())
            .unwrap_or(self.acting_player_name());
        if self
            .score
            .board
            .players_team(&name)
            .is_some_and(|t| t.players.contains(source_name))
        {
            return;
        }
        let burn = |game: &mut Self| {
            if !game.creative
                && !game.spectator()
                && let Some(p) = game.player.as_mut().filter(|p| !p.health.dead_or_dying())
            {
                p.fire_ticks = p.fire_ticks.max(60);
            }
        };
        if let Some(conn) = attacker {
            self.as_guest(host, conn, burn);
        } else {
            burn(self);
        }
    }
    pub(in crate::game) fn arcana_reflect_mob(&mut self, attacker: Option<u32>, damage_type: &str) {
        if !self.player.as_ref().is_some_and(|p| p.arcana_shield)
            || self.motion.health.damage.id(damage_type).is_some_and(|id| {
                self.motion
                    .health
                    .damage
                    .in_tag(id, "minecraft:bypasses_invulnerability")
            })
        {
            return;
        }
        let Some(id) = attacker else {
            return;
        };
        let allies = self.player_allies();
        let owner = self.acting_player_uuid();
        if let Some(m) = self.mobs.iter_mut().find(|m| {
            m.id == id && m.alive() && !allies.contains(&id) && m.extra.owner != Some(owner)
        }) {
            m.fire_ticks = m.fire_ticks.max(60);
            m.needs_sync = true;
        }
    }
    pub(super) fn arcana_prepare_pvp(&mut self, host: &LanHost) {
        self.arcana.pvp.players = host
            .hub
            .guests
            .iter()
            .filter(|g| g.placed)
            .map(|g| {
                let b = crate::game::lan_mobs::guest_bounds(g);
                Player {
                    uuid: g.uuid,
                    conn: g.conn,
                    name: g.name.clone(),
                    dimension: g.dimension.clone(),
                    eligible: !g.dead && g.health > 0.0 && !g.creative() && !g.spectator(),
                    target: Target {
                        id: PLAYER | u64::from(g.conn),
                        position: DVec3::from_array(g.position),
                        bounds: Bounds {
                            min: core(b.min_position()),
                            max: core(b.max_position()),
                        },
                        knockback_immune: false,
                    },
                }
            })
            .collect();
    }
    fn arcana_pvp_can_hit(&self, owner: u128, owner_name: &str, target: &Player) -> bool {
        target.uuid != owner
            && target.eligible
            && target.dimension == self.dim.id()
            && self.ui.rules.bool("pvp")
            && !self
                .score
                .board
                .players_team(owner_name)
                .is_some_and(|t| t.players.contains(&target.name))
    }
    pub(super) fn arcana_player_targets(&self, owner: u128, bounds: Bounds) -> Vec<Target> {
        let name = self.acting_player_name();
        self.arcana
            .pvp
            .players
            .iter()
            .filter(|p| self.arcana_pvp_can_hit(owner, name, p))
            .filter(|p| {
                !p.target.bounds.max.cmplt(bounds.min).any()
                    && !p.target.bounds.min.cmpgt(bounds.max).any()
            })
            .map(|p| p.target)
            .collect()
    }
    pub(super) fn arcana_begin_hit(&mut self, rule: Rule) {
        debug_assert!(self.arcana.pvp.current.is_none());
        self.arcana.pvp.current = Some(Group {
            owner: self.lan_acting_uuid.unwrap_or_default(),
            owner_name: self.acting_player_name().into(),
            dimension: self.dim.id().into(),
            from: self
                .player
                .as_ref()
                .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]),
            rule,
            core_xp: 0,
            mob_damage: 0,
            mob_force: false,
            terrain: false,
            operations: vec![],
        });
    }
    pub(super) fn arcana_finish_hit(&mut self, actor: &mut Actor, xp: u32) {
        if let Some(mut group) = self.arcana.pvp.current.take()
            && !group.operations.is_empty()
        {
            group.core_xp = xp;
            self.arcana.pvp.pending.push(group);
            return;
        }
        actor.state.add_xp(xp, catalog());
    }
    pub(super) fn arcana_mob_damage(&mut self, success: bool) {
        if success && let Some(g) = self.arcana.pvp.current.as_mut() {
            g.mob_damage += 1;
        }
    }
    pub(super) fn arcana_mob_force(&mut self) {
        if let Some(g) = self.arcana.pvp.current.as_mut() {
            g.mob_force = true;
        }
    }
    pub(super) fn arcana_terrain_reward(&mut self) {
        if let Some(g) = self.arcana.pvp.current.as_mut().filter(|g| g.rule.terrain) {
            g.terrain = true;
        }
    }
    pub(super) fn arcana_stable_reward(&mut self) {
        if let Some(g) = self.arcana.pvp.current.as_mut() {
            g.terrain = true;
        }
    }
    pub(super) fn arcana_queue_player(&mut self, id: u64, operation: Operation) -> Option<bool> {
        if id & PLAYER == 0 {
            return None;
        }
        let Some(target) = self
            .arcana
            .pvp
            .players
            .iter()
            .find(|p| p.target.id == id)
            .cloned()
        else {
            return Some(false);
        };
        let owner = self.lan_acting_uuid.unwrap_or_default();
        if !self.arcana_pvp_can_hit(owner, self.acting_player_name(), &target) {
            return Some(false);
        }
        let success = match operation {
            Operation::Damage(amount, _) => amount > 0.0,
            _ => true,
        };
        if !success {
            return Some(false);
        }
        if self.arcana.pvp.current.is_none() {
            self.arcana_begin_hit(Rule::stable());
        }
        if let Some(group) = self.arcana.pvp.current.as_mut() {
            group.operations.push(Pending { target, operation });
        }
        Some(true)
    }
    pub(super) fn arcana_apply_player_hits(&mut self, host: &mut LanHost) {
        if let Some(group) = self
            .arcana
            .pvp
            .current
            .take()
            .filter(|g| !g.operations.is_empty())
        {
            self.arcana.pvp.pending.push(group);
        }
        let mut awards: HashMap<u128, u32> = HashMap::new();
        for group in std::mem::take(&mut self.arcana.pvp.pending) {
            let mut damaged = HashSet::new();
            let mut force = false;
            let mut reflect = false;
            for pending in &group.operations {
                let Some(g) = host.hub.guest(pending.target.conn) else {
                    continue;
                };
                if g.uuid != pending.target.uuid
                    || g.dead
                    || g.health <= 0.0
                    || g.creative()
                    || g.spectator()
                    || g.dimension != group.dimension
                    || !self.ui.rules.bool("pvp")
                    || self
                        .score
                        .board
                        .players_team(&group.owner_name)
                        .is_some_and(|t| t.players.contains(&g.name))
                {
                    continue;
                }
                if group.rule.gated
                    && !matches!(pending.operation, Operation::Damage(..))
                    && !damaged.contains(&pending.target.uuid)
                {
                    continue;
                }
                let operation = pending.operation.clone();
                let from = group.from;
                let result = self
                    .guest_hit_by(host, pending.target.conn, |game| match operation {
                        Operation::Damage(amount, style) => {
                            reflect |= game.player.as_ref().is_some_and(|p| p.arcana_shield);
                            let kind = match style {
                                DamageStyle::Magic => "minecraft:indirect_magic",
                                DamageStyle::Thrown => "minecraft:thrown",
                            };
                            game.hurt_player_from(amount as f32, Some(from), kind, Some(None))
                        }
                        Operation::Ignite(ticks, at_least) => {
                            if let Some(p) = game.player.as_mut() {
                                p.fire_ticks = if at_least {
                                    p.fire_ticks.max(ticks as i32)
                                } else {
                                    ticks as i32
                                };
                            }
                            false
                        }
                        Operation::Clear => {
                            if let Some(p) = game.player.as_mut() {
                                p.fire_ticks = -20;
                            }
                            false
                        }
                        Operation::Slow(ticks) => {
                            game.add_player_effect(horizon_client::effects::EffectInstance::new(
                                "slowness",
                                ticks as i32,
                                0,
                            ));
                            false
                        }
                        Operation::Impulse(v) => {
                            if let Some(p) = game.player.as_mut() {
                                p.velocity = native(core(p.velocity) + v);
                                p.health.sync_velocity = true;
                            }
                            true
                        }
                    })
                    .unwrap_or(false);
                if result {
                    match pending.operation {
                        Operation::Damage(..) => {
                            damaged.insert(pending.target.uuid);
                        }
                        Operation::Impulse(..) => force = true,
                        _ => {}
                    }
                }
            }
            if reflect {
                let conn = host
                    .hub
                    .guests
                    .iter()
                    .find(|g| g.uuid == group.owner && !g.dead)
                    .map(|g| g.conn);
                if let Some(conn) = conn {
                    self.guest_hit_by(host, conn, |game| {
                        if !game.creative
                            && !game.spectator()
                            && let Some(p) = game.player.as_mut()
                        {
                            p.fire_ticks = p.fire_ticks.max(60);
                        }
                    });
                }
            }
            let xp = match group.rule.reward {
                Reward::Stable => group.core_xp,
                Reward::Once => {
                    if group.mob_damage > 0
                        || group.rule.force_reward && (group.mob_force || force)
                        || group.terrain
                        || !damaged.is_empty()
                    {
                        4
                    } else {
                        0
                    }
                }
                Reward::PerHit => 4 * (group.mob_damage + damaged.len() as u32),
            };
            *awards.entry(group.owner).or_default() += xp;
            log::info!(
                "Arcana PvP: confirmed_damage={}, force={}, xp={}",
                damaged.len(),
                force,
                xp
            );
        }
        for (owner, xp) in awards {
            if let Some(actor) = self.arcana.actors.get_mut(&owner) {
                actor.state.add_xp(xp, catalog());
                actor.dirty = true;
            }
            self.arcana_save_actor(owner);
            if let Some(conn) = host
                .hub
                .guests
                .iter()
                .find(|g| g.uuid == owner)
                .map(|g| g.conn)
            {
                self.as_guest(host, conn, |game| {
                    if let Some(actor) = game.arcana.actors.remove(&owner) {
                        game.arcana_send_snapshot(&actor, None);
                        game.arcana.actors.insert(owner, actor);
                    }
                });
            }
        }
    }
}
