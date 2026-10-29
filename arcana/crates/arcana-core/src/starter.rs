//! Original starter projectiles and AirSpells.gust. Simulation uses fixed 20 Hz
//! ticks; rendering never decides a collision, impulse or damage application.
use crate::{
    DVec3, Spell,
    solar::{Bounds, CombatWorld},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoltKind {
    Spark,
    Water,
    Stone,
    Fireball,
}
impl BoltKind {
    pub fn for_spell(id: &str) -> Option<Self> {
        match id {
            "spark" => Some(Self::Spark),
            "water_bolt" => Some(Self::Water),
            "stone_bolt" => Some(Self::Stone),
            "fireball" => Some(Self::Fireball),
            _ => None,
        }
    }
    pub fn spell_id(self) -> &'static str {
        match self {
            Self::Spark => "spark",
            Self::Water => "water_bolt",
            Self::Stone => "stone_bolt",
            Self::Fireball => "fireball",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bolt {
    pub id: u64,
    pub kind: BoltKind,
    pub position: DVec3,
    pub previous: DVec3,
    pub velocity: DVec3,
    pub age: u32,
    pub travelled: f64,
    pub range: f64,
    pub power: f64,
    #[serde(default)]
    finished: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub enum BoltStep {
    Flying,
    Impact {
        position: DVec3,
        target: Option<u64>,
        xp: u32,
    },
    Expired,
}
#[derive(Debug, Clone, Copy)]
pub struct MovingProjectile {
    pub id: u64,
    pub bounds: Bounds,
    pub position: DVec3,
}
#[derive(Debug, Clone, Copy)]
pub enum DamageStyle {
    Magic,
    Thrown,
}
pub trait StarterWorld: CombatWorld {
    fn damage_starter(&mut self, caster: u64, target: u64, amount: f64, style: DamageStyle)
    -> bool;
    fn ignite_at_least(&mut self, target: u64, ticks: u32);
    fn clear_fire(&mut self, target: u64);
    fn slow(&mut self, target: u64, ticks: u32);
    fn thaw(&mut self, target: u64, amount: f32);
    /// A host can refuse terrain ignition using its fire/permission policy.
    fn ignite_surface(&mut self, point: DVec3, incoming: DVec3);
    fn knockback_resistance(&self, target: u64) -> f64;
    /// Only moving, hostile arrows (not tridents), snowballs and eggs.
    fn hostile_projectiles(&self, caster: u64, bounds: Bounds) -> Vec<MovingProjectile>;
    fn discard_projectile(&mut self, id: u64);
    /// Owned fire only, subject to the host's world/placement policy.
    fn ignite_patch(&mut self, _: DVec3, _: f64) -> u32 {
        0
    }
}
impl Bolt {
    pub fn new(id: u64, spell: &Spell, position: DVec3, direction: DVec3) -> Result<Self, String> {
        let kind = BoltKind::for_spell(&spell.id).ok_or("not a starter projectile")?;
        if !position.is_finite()
            || !direction.is_finite()
            || (direction.length_squared() - 1.0).abs() > 1e-5
        {
            return Err("invalid projectile context".into());
        }
        Ok(Self {
            id,
            kind,
            position,
            previous: position,
            velocity: direction * spell.speed,
            age: 0,
            travelled: 0.0,
            range: spell.range,
            power: spell.power,
            finished: false,
        })
    }
    pub fn tick(
        &mut self,
        world: &mut impl StarterWorld,
        caster: u64,
        caster_position: DVec3,
    ) -> Result<BoltStep, String> {
        if self.finished {
            return Ok(BoltStep::Expired);
        }
        self.age += 1;
        if self.age > 160 || self.travelled > self.range {
            self.finished = true;
            return Ok(BoltStep::Expired);
        }
        let from = self.position;
        let maximum = from + self.velocity;
        let block = world.clip_blocks(from, maximum)?;
        let mut end = block.unwrap_or(maximum);
        let mut victim = None;
        let area = Bounds {
            min: from.min(end) - DVec3::ONE,
            max: from.max(end) + DVec3::ONE,
        };
        for target in world.hostile_targets(caster, area) {
            let expanded = Bounds {
                min: target.bounds.min - DVec3::splat(0.3),
                max: target.bounds.max + DVec3::splat(0.3),
            };
            if let Some(hit) = expanded.intersect(from, end)
                && hit.distance_squared(from) < end.distance_squared(from)
            {
                victim = Some(target);
                end = hit;
            }
        }
        self.previous = from;
        if victim.is_some() || block.is_some() {
            self.finished = true;
            self.position = end;
            if self.kind == BoltKind::Fireball {
                let blast = crate::fire::fireball_impact(world, caster, end, self.power)?;
                return Ok(BoltStep::Impact {
                    position: end,
                    target: victim.map(|t| t.id),
                    xp: blast.xp,
                });
            }
            let mut xp = 0;
            if let Some(target) = victim {
                let power = match self.kind {
                    BoltKind::Spark => 4.0,
                    BoltKind::Water => 3.0,
                    BoltKind::Stone => 6.0,
                    BoltKind::Fireball => unreachable!(),
                };
                let style = if self.kind == BoltKind::Stone {
                    DamageStyle::Thrown
                } else {
                    DamageStyle::Magic
                };
                if world.damage_starter(caster, target.id, power, style) {
                    xp = 4;
                    match self.kind {
                        BoltKind::Spark => {
                            world.ignite_at_least(target.id, 60);
                            world.thaw(target.id, 0.035);
                        }
                        BoltKind::Water => {
                            world.clear_fire(target.id);
                            world.slow(target.id, 35);
                            if !target.knockback_immune {
                                standard_impulse(
                                    world,
                                    target.id,
                                    target.position - caster_position,
                                    0.65,
                                );
                            }
                        }
                        BoltKind::Stone => {
                            if !target.knockback_immune {
                                standard_impulse(
                                    world,
                                    target.id,
                                    target.position - caster_position,
                                    1.05,
                                );
                            }
                        }
                        BoltKind::Fireball => unreachable!(),
                    }
                }
            } else if self.kind == BoltKind::Spark {
                world.ignite_surface(end, self.velocity.normalize_or_zero());
            }
            return Ok(BoltStep::Impact {
                position: end,
                target: victim.map(|t| t.id),
                xp,
            });
        }
        self.position = maximum;
        self.travelled += self.velocity.length();
        match self.kind {
            BoltKind::Stone => self.velocity = (self.velocity - DVec3::Y * 0.035) * 0.995,
            BoltKind::Water => self.velocity *= 0.997,
            BoltKind::Spark | BoltKind::Fireball => {}
        }
        Ok(BoltStep::Flying)
    }
}
fn standard_impulse(world: &mut impl StarterWorld, target: u64, direction: DVec3, force: f64) {
    let d = direction.normalize_or_zero();
    world.impulse(
        target,
        DVec3::new(d.x * force, 0.12 + d.y.max(0.0) * force * 0.35, d.z * force),
    );
}
pub fn air_sector(bounds: Bounds, origin: DVec3, direction: DVec3, reach: f64) -> bool {
    let delta = bounds.center() - origin;
    let size = bounds.max - bounds.min;
    let extent = (size.x + size.y + size.z) / 3.0 * 0.35;
    delta.length() <= reach + extent
        && delta.normalize_or_zero().dot(direction.normalize_or_zero()) >= 0.55
}
#[derive(Debug, Clone, PartialEq)]
pub struct GustOutcome {
    pub damaged: u32,
    pub discarded: u32,
    pub xp: u32,
}
pub fn gust(
    world: &mut impl StarterWorld,
    caster: u64,
    eye: DVec3,
    direction: DVec3,
    reach: f64,
) -> Result<GustOutcome, String> {
    if !eye.is_finite() || !direction.is_finite() || (direction.length_squared() - 1.0).abs() > 1e-5
    {
        return Err("invalid gust context".into());
    }
    let query = Bounds {
        min: eye - DVec3::splat(reach + 1.0),
        max: eye + DVec3::splat(reach + 1.0),
    };
    let mut damaged = 0;
    let mut discarded = 0;
    for target in world.hostile_targets(caster, query) {
        if !air_sector(target.bounds, eye, direction, reach)
            || !matches!(world.clip_blocks(eye, target.bounds.center()), Ok(None))
        {
            continue;
        }
        if !target.knockback_immune {
            let resistance = (1.0 - world.knockback_resistance(target.id)).clamp(0.08, 1.0);
            world.impulse(
                target.id,
                direction * DVec3::new(1.0, 0.1, 1.0) * 1.1 * resistance,
            );
        }
        if world.damage_starter(caster, target.id, 2.0, DamageStyle::Magic) {
            damaged += 1;
        }
    }
    for projectile in world.hostile_projectiles(caster, query) {
        if air_sector(projectile.bounds, eye, direction, reach)
            && matches!(world.clip_blocks(eye, projectile.position), Ok(None))
        {
            world.discard_projectile(projectile.id);
            discarded += 1;
        }
    }
    Ok(GustOutcome {
        damaged,
        discarded,
        xp: if damaged > 0 { 4 } else { 0 },
    })
}
