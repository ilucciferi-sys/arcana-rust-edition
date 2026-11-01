//! Persistent Fire mechanics from SpellEngine/ReworkSpells/ElementForms.
//! Effects are server simulations. Their snapshots also describe their geometry.
use crate::{
    DVec3, School, Spell,
    solar::{Bounds, Target},
    starter::{DamageStyle, StarterWorld},
};
use serde::{Deserialize, Serialize};

pub const EFFECTS_PER_OWNER: usize = 12;
pub const EFFECTS_PER_LEVEL: usize = 256;

#[derive(Debug, Clone, Copy)]
pub struct Owner {
    pub id: u64,
    pub position: DVec3,
    pub eye: DVec3,
    pub hand: DVec3,
    pub direction: DVec3,
    pub velocity: DVec3,
    pub channeling: bool,
}
impl Owner {
    pub fn valid(self) -> bool {
        [
            self.position,
            self.eye,
            self.hand,
            self.direction,
            self.velocity,
        ]
        .iter()
        .all(|p| p.is_finite())
            && (self.direction.length_squared() - 1.0).abs() < 1e-5
    }
}
pub trait FireSchoolWorld: StarterWorld {
    /// WorldMagic.surface: scan +4..=-5, return air above a dry non-fire support.
    fn surface(&self, near: DVec3) -> Result<DVec3, String>;
    /// First collision surface below the supplied position, without chunk loading.
    fn ground(&self, near: DVec3) -> Result<DVec3, String>;
    fn loaded(&self, at: DVec3) -> bool;
    fn build_ceiling(&self) -> f64;
    fn ignite_cell(&mut self, at: DVec3) -> bool;
    fn can_move_owner(&self, from: DVec3, to: DVec3) -> bool;
    fn teleport_owner(&mut self, to: DVec3);
    fn set_owner_velocity(&mut self, velocity: DVec3);
    fn random(&mut self) -> f64;
    fn target(&self, id: u64) -> Option<Target>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Effect {
    pub spell_id: String,
    pub position: DVec3,
    pub direction: DVec3,
    pub end: DVec3,
    pub age: u32,
    pub life: u32,
    /// Phoenix: 0 rising, 1 diving, 2 impact. Others: 0 active.
    pub stage: u8,
    pub target: Option<u64>,
    pub born: u64,
    last_tick: Option<u64>,
    finished: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Meteor {
    pub position: DVec3,
    pub velocity: DVec3,
}
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub xp: u32,
    pub meteors: Vec<Meteor>,
    pub finished: bool,
    pub impact: bool,
}

pub fn supported(id: &str) -> bool {
    matches!(
        id,
        "flame_wall"
            | "flame_shield"
            | "fire_dash"
            | "fire_ring"
            | "fire_blast"
            | "inferno_beam"
            | "fire_storm"
            | "phoenix_form"
            | "meteor_rain"
            | "fire_avatar"
    )
}
fn area(at: DVec3, radius: f64) -> Bounds {
    Bounds {
        min: at - DVec3::splat(radius),
        max: at + DVec3::splat(radius),
    }
}
fn visible(world: &impl StarterWorld, from: DVec3, t: Target) -> bool {
    matches!(world.clip_blocks(from, t.bounds.center()), Ok(None))
}
fn hit(
    world: &mut impl StarterWorld,
    owner: u64,
    t: Target,
    power: f64,
    impulse: Option<DVec3>,
) -> bool {
    hit_with_thaw(world, owner, t, power, impulse, 0.08)
}
fn hit_with_thaw(
    world: &mut impl StarterWorld,
    owner: u64,
    t: Target,
    power: f64,
    impulse: Option<DVec3>,
    thaw: f32,
) -> bool {
    let damaged = world.damage_starter(owner, t.id, power, DamageStyle::Magic);
    world.ignite_at_least(t.id, 80);
    let pushed = impulse.is_some() && !t.knockback_immune;
    if let Some(v) = impulse.filter(|_| pushed) {
        world.impulse(t.id, v);
    }
    if damaged || pushed {
        world.thaw(t.id, thaw);
    }
    damaged || pushed
}
/// Original ReworkSpells.blast: center distance, minimum 10%, per-victim XP.
/// Its impulse is raw push (not scaled by vanilla knockback resistance).
pub fn blast(
    world: &mut impl StarterWorld,
    owner: u64,
    at: DVec3,
    radius: f64,
    power: f64,
    break_blocks: bool,
) -> u32 {
    let mut xp = 0;
    for t in world.hostile_targets(owner, area(at, radius)) {
        let distance = t.bounds.center().distance(at);
        if distance > radius {
            continue;
        }
        let scale = (1.0 - distance / radius).max(0.1);
        if world.damage_starter(owner, t.id, power * scale, DamageStyle::Magic) {
            xp += 4;
        }
        if !t.knockback_immune {
            let dir = (t.position - at).normalize_or_zero() + DVec3::Y * 0.25;
            let force = 1.2 * scale;
            world.impulse(
                t.id,
                DVec3::new(
                    dir.x * force,
                    0.12 + dir.y.max(0.0) * force * 0.35,
                    dir.z * force,
                ),
            );
        }
        world.ignite(t.id, 200);
    }
    if break_blocks {
        world.break_soft_sphere(owner, at, radius.min(6.0));
    }
    xp
}

pub fn aimed(world: &impl StarterWorld, owner: Owner, range: f64) -> Result<DVec3, String> {
    let end = world
        .clip_blocks(owner.eye, owner.eye + owner.direction * range)?
        .unwrap_or(owner.eye + owner.direction * range);
    let candidates = Bounds {
        min: owner.eye.min(end) - DVec3::ONE,
        max: owner.eye.max(end) + DVec3::ONE,
    };
    Ok(world
        .hostile_targets(owner.id, candidates)
        .into_iter()
        .filter(|t| {
            Bounds {
                min: t.bounds.min - DVec3::splat(0.35),
                max: t.bounds.max + DVec3::splat(0.35),
            }
            .intersect(owner.eye, end)
            .is_some()
        })
        .min_by(|a, b| {
            a.position
                .distance_squared(owner.position)
                .total_cmp(&b.position.distance_squared(owner.position))
        })
        .map_or(end, |t| t.position))
}

impl Effect {
    pub fn release(
        world: &mut impl FireSchoolWorld,
        owner: Owner,
        spell: &Spell,
        tick: u64,
    ) -> Result<(Self, Outcome), String> {
        if !owner.valid() || spell.school != School::Fire || !supported(&spell.id) {
            return Err("invalid fire school release".into());
        }
        let mut position = owner.position;
        let mut direction = owner.direction;
        let mut target = None;
        let mut life = spell.duration;
        let mut outcome = Outcome::default();
        match spell.mechanic.as_str() {
            "WALL" => {
                position = world.surface(aimed(world, owner, spell.range)?)?;
                let side = direction.cross(DVec3::Y).normalize_or_zero();
                for i in -(spell.radius as i32)..=spell.radius as i32 {
                    world.ignite_cell(position + side * f64::from(i));
                }
            }
            "BLAST" | "METEORS" => position = aimed(world, owner, spell.range)?,
            "TORNADO" => {
                let at = aimed(world, owner, spell.range)?;
                target = world
                    .hostile_targets(owner.id, area(at, 1.0))
                    .into_iter()
                    .find(|t| t.position.distance_squared(at) < 1e-6)
                    .map(|t| t.id);
                direction = (direction * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
                position = owner.position + direction * 2.0;
            }
            "SHIELD" | "AVATAR" => outcome.xp = 4,
            "BEAM" => position = owner.hand,
            "DASH" => {
                direction = (direction * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
                let mut end = position;
                for i in 1..=32 {
                    let next = position + direction * (f64::from(i) * 0.25);
                    if !world.loaded(next) || !world.can_move_owner(position, next) {
                        break;
                    }
                    end = next;
                    if i % 4 == 0
                        && let Ok(surface) = world.surface(next)
                    {
                        world.ignite_cell(surface);
                    }
                }
                world.teleport_owner(end);
                let query = Bounds {
                    min: position.min(end) - DVec3::splat(1.2),
                    max: position.max(end) + DVec3::splat(1.2),
                };
                for t in world.hostile_targets(owner.id, query) {
                    if (Bounds {
                        min: t.bounds.min - DVec3::splat(0.8),
                        max: t.bounds.max + DVec3::splat(0.8),
                    })
                    .intersect(position + DVec3::Y * 0.8, end + DVec3::Y * 0.8)
                    .is_some()
                    {
                        hit(
                            world,
                            owner.id,
                            t,
                            spell.power,
                            Some(direction * 0.4 + DVec3::Y * 0.15),
                        );
                    }
                }
                outcome.xp = if end.distance_squared(position) > 0.1 {
                    4
                } else {
                    0
                };
                direction *= end.distance(position);
                life = 35;
            }
            _ => {}
        }
        let mut effect = Self {
            spell_id: spell.id.clone(),
            position,
            direction,
            end: position,
            age: 0,
            life,
            stage: 0,
            target,
            born: tick,
            last_tick: None,
            finished: false,
        };
        if spell.mechanic == "BEAM" {
            outcome.xp = effect.beam(world, owner, spell)?;
        }
        Ok((effect, outcome))
    }

    pub fn dive(&mut self, direction: DVec3) -> bool {
        if self.spell_id != "phoenix_form"
            || self.stage != 0
            || !direction.is_finite()
            || (direction.length_squared() - 1.0).abs() > 1e-5
        {
            return false;
        }
        self.stage = 1;
        self.direction = direction;
        true
    }
    pub fn stop(&mut self) {
        self.finished = true;
    }
    pub fn active(&self) -> bool {
        !self.finished && self.age < self.life
    }
    fn beam(
        &mut self,
        world: &mut impl FireSchoolWorld,
        owner: Owner,
        spell: &Spell,
    ) -> Result<u32, String> {
        self.position = owner.hand;
        self.direction = owner.direction;
        self.end = world
            .clip_blocks(owner.hand, owner.hand + owner.direction * spell.range)?
            .unwrap_or(owner.hand + owner.direction * spell.range);
        let mut success = false;
        let query = Bounds {
            min: owner.hand.min(self.end) - DVec3::ONE,
            max: owner.hand.max(self.end) + DVec3::ONE,
        };
        for t in world.hostile_targets(owner.id, query) {
            if (Bounds {
                min: t.bounds.min - DVec3::splat(0.33),
                max: t.bounds.max + DVec3::splat(0.33),
            })
            .intersect(owner.hand, self.end)
            .is_some()
            {
                success |= hit_with_thaw(world, owner.id, t, spell.power, None, 0.18);
            }
        }
        world.ignite_surface(self.end, owner.direction);
        Ok(if success { 4 } else { 0 })
    }
    pub fn tick(
        &mut self,
        world: &mut impl FireSchoolWorld,
        owner: Owner,
        spell: &Spell,
        tick: u64,
    ) -> Result<Outcome, String> {
        if !owner.valid() || spell.id != self.spell_id || tick < self.born {
            return Err("invalid fire effect context".into());
        }
        if self.last_tick.is_some_and(|last| tick <= last) {
            return Ok(Outcome::default());
        }
        self.last_tick = Some(tick);
        self.age = (tick - self.born).min(u64::from(u32::MAX)) as u32;
        let mut out = Outcome::default();
        if self.finished || self.age > self.life || !world.loaded(self.position) {
            self.finished = true;
            out.finished = true;
            return Ok(out);
        }
        if self.age == 0 {
            return Ok(out);
        }
        let pulse = self.age.is_multiple_of(10);
        match spell.mechanic.as_str() {
            "WALL" if pulse => {
                let side = self.direction.cross(DVec3::Y).normalize_or_zero();
                let forward = side.cross(DVec3::Y).normalize_or_zero();
                for t in world.hostile_targets(owner.id, area(self.position, spell.radius)) {
                    let delta = t.bounds.center() - self.position;
                    if delta.dot(side).abs() <= spell.radius
                        && delta.dot(forward).abs() < 1.5
                        && (0.0..=12.0).contains(&delta.y)
                    {
                        world.ignite(t.id, 200);
                        if world.damage_starter(
                            owner.id,
                            t.id,
                            spell.power * 0.4,
                            DamageStyle::Magic,
                        ) {
                            out.xp += 4;
                        }
                    }
                }
            }
            "BLAST" if self.age == 1 => {
                out.xp = blast(
                    world,
                    owner.id,
                    self.position,
                    spell.radius,
                    spell.power,
                    true,
                );
                world.ignite_patch(self.position, spell.radius);
                out.impact = true;
            }
            "BEAM" => {
                if !owner.channeling {
                    self.finished = true;
                    out.finished = true;
                } else if self.age.is_multiple_of(6) {
                    out.xp = self.beam(world, owner, spell)?;
                } else {
                    self.position = owner.hand;
                    self.direction = owner.direction;
                    self.end = world
                        .clip_blocks(owner.hand, owner.hand + owner.direction * spell.range)?
                        .unwrap_or(owner.hand + owner.direction * spell.range);
                }
            }
            "SHIELD" | "AVATAR" => {
                self.position = owner.position;
                if pulse {
                    let radius = if spell.mechanic == "SHIELD" { 2.0 } else { 5.0 };
                    let mut ok = false;
                    for t in world.hostile_targets(owner.id, area(self.position, radius)) {
                        if t.position.distance_squared(self.position) > radius * radius
                            || (spell.mechanic == "AVATAR" && !visible(world, owner.eye, t))
                        {
                            continue;
                        }
                        ok |= hit(
                            world,
                            owner.id,
                            t,
                            if spell.mechanic == "SHIELD" { 0.0 } else { 5.0 },
                            None,
                        );
                    }
                    if ok && spell.mechanic == "AVATAR" {
                        out.xp = 4;
                    }
                }
                if spell.mechanic == "AVATAR"
                    && self.age.is_multiple_of(5)
                    && owner.velocity.length_squared() > 0.002
                {
                    world.ignite_patch(owner.position, 0.75);
                }
            }
            "RING" if pulse => {
                let r = (spell.radius + 0.75).min(f64::from(self.age) * 0.16);
                let mut ok = false;
                for t in world.hostile_targets(owner.id, area(self.position, r)) {
                    if t.position.distance_squared(self.position) <= r * r
                        && visible(world, self.position + DVec3::Y * 0.8, t)
                    {
                        ok |= hit(world, owner.id, t, spell.power * 0.4, None);
                    }
                }
                if ok {
                    out.xp = 4;
                }
                world.ignite_patch(self.position, r.min(5.0));
            }
            "TORNADO" => {
                if let Some(t) = self.target.and_then(|id| world.target(id)) {
                    self.direction = (self.direction * 0.8
                        + (t.position - self.position).normalize_or_zero() * 0.2)
                        .normalize_or_zero();
                }
                let next = self.position + self.direction * spell.speed;
                if world.loaded(next) {
                    self.position = next;
                }
                let r = spell.radius;
                let query = Bounds {
                    min: self.position - DVec3::new(r, 1.0, r),
                    max: self.position + DVec3::new(r, 14.0, r),
                };
                let mut affected = false;
                for t in world.hostile_targets(owner.id, query) {
                    if !fire_tornado(t.bounds, self.position, r, 14.0, f64::from(self.age))
                        || !visible(world, self.position + DVec3::Y * 0.8, t)
                    {
                        continue;
                    }
                    let delta = t.position - self.position;
                    let distance = delta.x.hypot(delta.z).max(0.5);
                    let age = f64::from(self.age);
                    let id = t.id as f64;
                    if !t.knockback_immune {
                        world.impulse(
                            t.id,
                            DVec3::new(
                                -delta.x / distance * 0.09
                                    - delta.z / distance * (0.12 + 0.025 * (age * 0.19 + id).sin()),
                                0.065 + 0.025 * (age * 0.11 + id).sin(),
                                -delta.z / distance * 0.09
                                    + delta.x / distance * (0.12 + 0.03 * (age * 0.17 + id).cos()),
                            ),
                        );
                    }
                    if pulse {
                        affected = true;
                        hit(world, owner.id, t, spell.power * 0.3, None);
                    }
                }
                if affected {
                    out.xp = 4;
                }
                if pulse {
                    world.ignite_patch(self.position, r.min(5.0));
                }
            }
            "METEORS" if self.age.is_multiple_of(12) => {
                for _ in 0..3 {
                    let angle = world.random() * std::f64::consts::TAU;
                    let radius = world.random().sqrt() * (spell.radius - 4.0).max(0.0);
                    let landing =
                        self.position + DVec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    if !world.loaded(landing) {
                        continue;
                    }
                    if let Ok(ground) = world.ground(landing) {
                        let height = (ground.y + 14.0 + world.random() * 6.0)
                            .min(world.build_ceiling() - 1.0);
                        if height > ground.y + 1.0 {
                            out.meteors.push(Meteor {
                                position: DVec3::new(landing.x, height, landing.z),
                                velocity: -DVec3::Y * 0.8,
                            });
                        }
                    }
                }
            }
            "PHOENIX" => {
                if self.stage == 2 {
                    return Ok(out);
                }
                self.position = owner.position;
                if self.stage == 0 && self.age >= 100 {
                    self.dive(owner.direction);
                }
                if self.stage == 0 {
                    world.set_owner_velocity(
                        owner.velocity * DVec3::new(0.75, 0.0, 0.75) + DVec3::Y * 0.32,
                    );
                } else {
                    let end = owner.eye + self.direction * 4.0;
                    let block = world.clip_blocks(owner.eye, end)?;
                    let query = Bounds {
                        min: owner.eye.min(end) - DVec3::ONE,
                        max: owner.eye.max(end) + DVec3::ONE,
                    };
                    let body = world.hostile_targets(owner.id, query).into_iter().any(|t| {
                        (Bounds {
                            min: t.bounds.min - DVec3::splat(0.7),
                            max: t.bounds.max + DVec3::splat(0.7),
                        })
                        .intersect(owner.eye, end)
                        .is_some()
                    });
                    if block.is_some() || body || self.age >= 150 {
                        self.position = block.unwrap_or(owner.position);
                        self.stage = 2;
                        self.life = self.age + 40;
                        world.set_owner_velocity(DVec3::ZERO);
                        out.xp = blast(world, owner.id, self.position, 20.0, spell.power, false);
                        world.ignite_patch(self.position, 20.0);
                        out.impact = true;
                    } else {
                        world.set_owner_velocity(self.direction * 4.0);
                    }
                }
            }
            _ => {}
        }
        Ok(out)
    }
}

/// Fire funnel's combat profile; the renderer uses these same dimensions.
pub fn fire_tornado(bounds: Bounds, base: DVec3, radius: f64, height: f64, age: f64) -> bool {
    if bounds.max.y < base.y || bounds.min.y > base.y + height {
        return false;
    }
    let center = bounds.center();
    let q = ((center.y - base.y) / height).clamp(0.0, 1.0);
    let bend = DVec3::new(
        (age * 0.025 + q * 3.8).sin() * radius * 0.14 * q,
        0.0,
        (age * 0.021 + q * 4.5).cos() * radius * 0.11 * q,
    );
    let offset = center - base - bend;
    let extent = (bounds.max.x - bounds.min.x) * 0.5;
    offset.x * offset.x + offset.z * offset.z <= (0.65 + radius * q.powf(1.2) + extent).powi(2)
}

pub fn shield_damage(amount: f32, active: bool, bypasses_invulnerability: bool) -> f32 {
    if active && !bypasses_invulnerability {
        amount * 0.6
    } else {
        amount
    }
}
