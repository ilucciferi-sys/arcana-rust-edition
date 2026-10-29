//! SpellEngine IGNITE/PUSH. Selection and rewards follow the original server
//! rules; rendering never chooses a victim or repeats a hit.
use crate::{
    DVec3, Spell,
    solar::{Bounds, Target},
    starter::{DamageStyle, StarterWorld},
};
use serde::{Deserialize, Serialize};

pub trait FireWorld: StarterWorld {
    /// Host must place owned arcane fire through its world/permission policy.
    /// Returning false is a refusal, never substitute unrestricted vanilla fire.
    fn ignite_ray(&mut self, eye: DVec3, direction: DVec3, range: f64) -> bool;
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ignition {
    pub position: DVec3,
    pub target: Option<u64>,
    pub damaged: bool,
    pub surface_hit: bool,
    pub surface_ignited: bool,
    pub xp: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Push {
    pub affected: u32,
    pub damaged: u32,
    pub pushed: u32,
    pub xp: u32,
}
fn valid(
    spell: &Spell,
    id: &str,
    mechanic: &str,
    position: DVec3,
    eye: DVec3,
    direction: DVec3,
) -> Result<(), String> {
    if spell.id != id
        || spell.mechanic != mechanic
        || !position.is_finite()
        || !eye.is_finite()
        || !direction.is_finite()
        || (direction.length_squared() - 1.0).abs() > 1e-5
    {
        return Err("invalid authoritative fire context".into());
    }
    Ok(())
}
fn fire_hit(
    world: &mut impl FireWorld,
    caster: u64,
    target: Target,
    power: f64,
    impulse: Option<DVec3>,
) -> (bool, bool) {
    let damaged = world.damage_starter(caster, target.id, power, DamageStyle::Magic);
    // SpellEngine.hit burns even when LivingEntity.hurt rejects the damage.
    world.ignite_at_least(target.id, 80);
    let pushed = impulse.is_some() && !target.knockback_immune;
    if let Some(impulse) = impulse.filter(|_| pushed) {
        world.impulse(target.id, impulse);
    }
    if damaged || pushed {
        world.thaw(target.id, 0.08);
    }
    (damaged, pushed)
}
pub fn ignite(
    world: &mut impl FireWorld,
    caster: u64,
    spell: &Spell,
    position: DVec3,
    eye: DVec3,
    direction: DVec3,
) -> Result<Ignition, String> {
    valid(spell, "ignition", "IGNITE", position, eye, direction)?;
    let maximum = eye + direction * spell.range;
    let block = world.clip_blocks(eye, maximum)?;
    let end = block.unwrap_or(maximum);
    let query = Bounds {
        min: eye.min(end) - DVec3::ONE,
        max: eye.max(end) + DVec3::ONE,
    };
    let target = world
        .hostile_targets(caster, query)
        .into_iter()
        .filter(|t| {
            let bounds = Bounds {
                min: t.bounds.min - DVec3::splat(0.35),
                max: t.bounds.max + DVec3::splat(0.35),
            };
            bounds.intersect(eye, end).is_some()
        })
        .min_by(|a, b| {
            a.position
                .distance_squared(position)
                .total_cmp(&b.position.distance_squared(position))
        });
    let damaged = target.is_some_and(|target| fire_hit(world, caster, target, spell.power, None).0);
    let surface_ignited = block.is_some() && world.ignite_ray(eye, direction, spell.range);
    Ok(Ignition {
        position: target.map_or(end, |t| t.position),
        target: target.map(|t| t.id),
        damaged,
        surface_hit: block.is_some(),
        surface_ignited,
        xp: if damaged || surface_ignited { 4 } else { 0 },
    })
}
pub fn push(
    world: &mut impl FireWorld,
    caster: u64,
    spell: &Spell,
    position: DVec3,
    eye: DVec3,
    direction: DVec3,
) -> Result<Push, String> {
    valid(spell, "fire_push", "PUSH", position, eye, direction)?;
    let radius = spell.radius;
    let query = Bounds {
        min: position - DVec3::splat(radius),
        max: position + DVec3::splat(radius),
    };
    let mut result = Push {
        affected: 0,
        damaged: 0,
        pushed: 0,
        xp: 0,
    };
    for target in world.hostile_targets(caster, query) {
        let offset = target.position - position;
        if offset.length_squared() > radius * radius
            || offset.normalize_or_zero().dot(direction) < 0.6
            || !matches!(world.clip_blocks(eye, target.bounds.center()), Ok(None))
        {
            continue;
        }
        let (damaged, pushed) = fire_hit(
            world,
            caster,
            target,
            spell.power,
            Some(direction * 0.9 + DVec3::Y * 0.2),
        );
        result.affected += 1;
        result.damaged += u32::from(damaged);
        result.pushed += u32::from(pushed);
    }
    // One award for the whole cast, including a successful impulse without damage.
    if result.damaged > 0 || result.pushed > 0 {
        result.xp = 4;
    }
    Ok(result)
}
