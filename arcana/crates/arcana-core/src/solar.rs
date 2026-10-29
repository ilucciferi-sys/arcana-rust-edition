//! FireSpells.solarSpear + ReworkSpells.blast, with a host-owned world boundary.
use crate::{DVec3, Spell};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub min: DVec3,
    pub max: DVec3,
}
impl Bounds {
    pub fn center(self) -> DVec3 {
        (self.min + self.max) * 0.5
    }
    pub fn intersect(self, start: DVec3, end: DVec3) -> Option<DVec3> {
        let delta = end - start;
        let mut near: f64 = 0.0;
        let mut far: f64 = 1.0;
        for axis in 0..3 {
            if delta[axis].abs() < 1e-12 {
                if start[axis] < self.min[axis] || start[axis] > self.max[axis] {
                    return None;
                }
            } else {
                let a = (self.min[axis] - start[axis]) / delta[axis];
                let b = (self.max[axis] - start[axis]) / delta[axis];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return None;
                }
            }
        }
        Some(start + delta * near)
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Target {
    pub id: u64,
    pub bounds: Bounds,
    pub position: DVec3,
    pub knockback_immune: bool,
}

pub trait CombatWorld {
    /// Must reject unloaded/out-of-border segments. Fluids do not collide.
    fn clip_blocks(&self, start: DVec3, end: DVec3) -> Result<Option<DVec3>, String>;
    /// Host must enforce self/team/PvP/creative/intangibility/summon ownership.
    fn hostile_targets(&self, caster: u64, bounds: Bounds) -> Vec<Target>;
    fn damage(&mut self, caster: u64, target: u64, amount: f64) -> bool;
    fn ignite(&mut self, target: u64, ticks: u32);
    fn impulse(&mut self, target: u64, velocity: DVec3);
    /// Apply only breakSoft permissions and loaded-chunk/event checks, never raw writes.
    fn break_soft_sphere(&mut self, caster: u64, center: DVec3, radius: f64);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarOutcome {
    pub caster: u64,
    pub start: DVec3,
    pub end: DVec3,
    pub impacted: bool,
    pub xp_awarded: u32,
    pub beam_lifetime_ticks: u32,
}

pub fn execute(
    world: &mut impl CombatWorld,
    caster: u64,
    spell: &Spell,
    hand: DVec3,
    direction: DVec3,
    griefing: bool,
) -> Result<SolarOutcome, String> {
    if spell.id != "solar_spear"
        || spell.mechanic != "SUN_SPEAR"
        || !hand.is_finite()
        || !direction.is_finite()
        || (direction.length_squared() - 1.0).abs() > 1e-5
    {
        return Err("invalid authoritative cast context".into());
    }
    let maximum = hand + direction * spell.range;
    let block = world.clip_blocks(hand, maximum)?;
    let mut end = block.unwrap_or(maximum);
    // Host implementations cannot accidentally place a beam beyond the requested segment.
    let along = (end - hand).dot(direction);
    if !end.is_finite()
        || along < 0.0
        || along > spell.range + 1e-6
        || (hand + direction * along - end).length_squared() > 1e-8
    {
        return Err("invalid block intersection".into());
    }
    let mut impacted = block.is_some();
    let query = Bounds {
        min: hand.min(end) - DVec3::ONE,
        max: hand.max(end) + DVec3::ONE,
    };
    for target in world.hostile_targets(caster, query) {
        if let Some(hit) = target.bounds.intersect(hand, end)
            && hit.distance_squared(hand) < end.distance_squared(hand)
        {
            end = hit;
            impacted = true;
        }
    }
    let mut xp_awarded = 0;
    if impacted {
        let radius = spell.radius;
        let blast = Bounds {
            min: end - DVec3::splat(radius),
            max: end + DVec3::splat(radius),
        };
        for target in world.hostile_targets(caster, blast) {
            let distance = target.bounds.center().distance(end);
            if distance > radius {
                continue;
            }
            let scale = (1.0 - distance / radius).max(0.1);
            if world.damage(caster, target.id, spell.power * scale) {
                xp_awarded += 4;
            }
            if !target.knockback_immune {
                let push = (target.position - end).normalize_or_zero() + DVec3::Y * 0.25;
                let force = 1.2 * scale;
                world.impulse(
                    target.id,
                    DVec3::new(
                        push.x * force,
                        0.12 + push.y.max(0.0) * force * 0.35,
                        push.z * force,
                    ),
                );
            }
            world.ignite(target.id, 200);
        }
        if griefing {
            world.break_soft_sphere(caster, end, radius.min(6.0));
        }
    }
    Ok(SolarOutcome {
        caster,
        start: hand,
        end,
        impacted,
        xp_awarded,
        beam_lifetime_ticks: 30,
    })
}
