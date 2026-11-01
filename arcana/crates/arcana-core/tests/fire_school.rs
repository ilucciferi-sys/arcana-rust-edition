use arcana_core::{
    Catalog, DVec3,
    fire_school::*,
    solar::{Bounds, CombatWorld, Target},
    starter::{DamageStyle, MovingProjectile, StarterWorld},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct World {
    targets: Vec<Target>,
    hits: Vec<(u64, f64)>,
    fire: HashMap<u64, u32>,
    impulses: Vec<(u64, DVec3)>,
    cells: Vec<DVec3>,
    patches: Vec<(DVec3, f64)>,
    breaks: Vec<f64>,
    teleported: Option<DVec3>,
    velocity: Option<DVec3>,
    reject: bool,
    blocked: HashSet<u64>,
    wall: Option<f64>,
    movement_limit: Option<f64>,
    unloaded: bool,
}
impl CombatWorld for World {
    fn clip_blocks(&self, a: DVec3, b: DVec3) -> Result<Option<DVec3>, String> {
        if self.unloaded {
            return Err("unloaded".into());
        }
        if self
            .targets
            .iter()
            .any(|t| self.blocked.contains(&t.id) && t.bounds.center() == b)
        {
            return Ok(Some((a + b) * 0.5));
        }
        Ok(self
            .wall
            .filter(|z| a.z <= *z && b.z >= *z)
            .map(|z| a + (b - a) * ((z - a.z) / (b.z - a.z))))
    }
    fn hostile_targets(&self, _: u64, b: Bounds) -> Vec<Target> {
        self.targets
            .iter()
            .filter(|t| !t.bounds.max.cmplt(b.min).any() && !t.bounds.min.cmpgt(b.max).any())
            .copied()
            .collect()
    }
    fn damage(&mut self, _: u64, t: u64, p: f64) -> bool {
        self.hits.push((t, p));
        !self.reject && p > 0.0
    }
    fn ignite(&mut self, t: u64, n: u32) {
        self.fire.insert(t, n);
    }
    fn impulse(&mut self, t: u64, v: DVec3) {
        self.impulses.push((t, v));
    }
    fn break_soft_sphere(&mut self, _: u64, _: DVec3, r: f64) {
        self.breaks.push(r);
    }
}
impl StarterWorld for World {
    fn damage_starter(&mut self, c: u64, t: u64, p: f64, _: DamageStyle) -> bool {
        self.damage(c, t, p)
    }
    fn ignite_at_least(&mut self, t: u64, n: u32) {
        let f = self.fire.entry(t).or_default();
        *f = (*f).max(n);
    }
    fn clear_fire(&mut self, _: u64) {}
    fn slow(&mut self, _: u64, _: u32) {}
    fn thaw(&mut self, _: u64, _: f32) {}
    fn ignite_surface(&mut self, p: DVec3, _: DVec3) {
        self.cells.push(p);
    }
    fn knockback_resistance(&self, _: u64) -> f64 {
        0.0
    }
    fn hostile_projectiles(&self, _: u64, _: Bounds) -> Vec<MovingProjectile> {
        vec![]
    }
    fn discard_projectile(&mut self, _: u64) {}
    fn ignite_patch(&mut self, p: DVec3, r: f64) -> u32 {
        self.patches.push((p, r));
        1
    }
}
impl FireSchoolWorld for World {
    fn surface(&self, p: DVec3) -> Result<DVec3, String> {
        Ok(DVec3::new(p.x.floor() + 0.5, 0.0, p.z.floor() + 0.5))
    }
    fn ground(&self, p: DVec3) -> Result<DVec3, String> {
        self.surface(p)
    }
    fn loaded(&self, _: DVec3) -> bool {
        !self.unloaded
    }
    fn build_ceiling(&self) -> f64 {
        320.0
    }
    fn ignite_cell(&mut self, p: DVec3) -> bool {
        self.cells.push(p);
        true
    }
    fn can_move_owner(&self, _: DVec3, p: DVec3) -> bool {
        self.movement_limit.is_none_or(|z| p.z <= z)
    }
    fn teleport_owner(&mut self, p: DVec3) {
        self.teleported = Some(p);
    }
    fn set_owner_velocity(&mut self, v: DVec3) {
        self.velocity = Some(v);
    }
    fn random(&mut self) -> f64 {
        0.5
    }
    fn target(&self, id: u64) -> Option<Target> {
        self.targets.iter().find(|t| t.id == id).copied()
    }
}
fn owner() -> Owner {
    Owner {
        id: 99,
        position: DVec3::ZERO,
        eye: DVec3::Y * 1.6,
        hand: DVec3::new(0.3, 1.3, 0.55),
        direction: DVec3::Z,
        velocity: DVec3::ZERO,
        channeling: true,
    }
}
fn target(id: u64, p: DVec3) -> Target {
    Target {
        id,
        position: p,
        bounds: Bounds {
            min: p - DVec3::new(0.25, 0.0, 0.25),
            max: p + DVec3::new(0.25, 2.0, 0.25),
        },
        knockback_immune: false,
    }
}
fn release(w: &mut World, id: &str) -> (Effect, Outcome) {
    Effect::release(
        w,
        owner(),
        Catalog::bundled().unwrap().get(id).unwrap(),
        100,
    )
    .unwrap()
}
fn tick(e: &mut Effect, w: &mut World, age: u64) -> Outcome {
    e.tick(
        w,
        owner(),
        Catalog::bundled().unwrap().get(&e.spell_id).unwrap(),
        100 + age,
    )
    .unwrap()
}

#[test]
fn every_remaining_fire_definition_has_an_executor_and_original_parameters() {
    let c = Catalog::bundled().unwrap();
    let mut w = World::default();
    for id in [
        "flame_wall",
        "flame_shield",
        "fire_dash",
        "fire_ring",
        "fire_blast",
        "inferno_beam",
        "fire_storm",
        "phoenix_form",
        "meteor_rain",
        "fire_avatar",
    ] {
        assert!(supported(id));
        assert!(Effect::release(&mut w, owner(), c.get(id).unwrap(), 100).is_ok());
    }
    let s = c.get("flame_wall").unwrap();
    assert_eq!(
        (s.mana, s.cast_time, s.cooldown, s.radius, s.duration),
        (29.0, 15, 200, 12.0, 180)
    );
    assert!(!supported("water_bolt"));
}
#[test]
fn wall_hits_its_tall_slab_each_ten_ticks_and_awards_each_success() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "flame_wall");
    let at = e.position;
    w.targets = vec![
        target(1, at + DVec3::new(11.9, 0.0, 0.0)),
        target(2, at + DVec3::new(0.0, 10.9, 0.0)),
        target(3, at + DVec3::new(0.0, 0.0, 1.5)),
        target(4, at + DVec3::new(12.1, 0.0, 0.0)),
    ];
    w.fire.insert(1, 400);
    w.blocked.insert(1);
    assert_eq!(w.cells.len(), 25);
    assert_eq!(tick(&mut e, &mut w, 9).xp, 0);
    assert_eq!(tick(&mut e, &mut w, 10).xp, 8);
    assert_eq!(w.hits, vec![(1, 4.0), (2, 4.0)]);
    assert_eq!(w.fire[&1], 200);
    assert_eq!(tick(&mut e, &mut w, 10), Outcome::default());
    assert_eq!(w.hits.len(), 2);
    assert_eq!(tick(&mut e, &mut w, 20).xp, 8);
    assert!(tick(&mut e, &mut w, 181).finished);
    assert!(tick(&mut e, &mut w, 182).finished);
}
#[test]
fn shield_reduces_damage_but_does_not_reduce_bypass_or_grant_pulse_xp() {
    assert!((shield_damage(10.0, true, false) - 6.0).abs() < 0.001);
    assert_eq!(shield_damage(10.0, true, true), 10.0);
    assert_eq!(shield_damage(10.0, false, false), 10.0);
    let mut w = World {
        targets: vec![target(1, DVec3::Z)],
        ..Default::default()
    };
    let (mut e, r) = release(&mut w, "flame_shield");
    assert_eq!(r.xp, 4);
    assert_eq!(tick(&mut e, &mut w, 10).xp, 0);
    assert_eq!(w.fire[&1], 80);
}
#[test]
fn dash_traces_body_in_quarter_block_steps_and_respects_collision() {
    let mut w = World {
        movement_limit: Some(3.0),
        ..Default::default()
    };
    w.targets = vec![
        target(1, DVec3::Z * 2.0),
        target(2, DVec3::new(3.0, 0.0, 2.0)),
    ];
    let (e, r) = release(&mut w, "fire_dash");
    assert_eq!(w.teleported, Some(DVec3::Z * 3.0));
    assert_eq!(e.direction, DVec3::Z * 3.0);
    assert_eq!(r.xp, 4);
    assert_eq!(w.hits, vec![(1, 8.0)]);
    assert_eq!(w.cells.len(), 3);
    let mut w = World {
        movement_limit: Some(0.0),
        ..Default::default()
    };
    assert_eq!(release(&mut w, "fire_dash").1.xp, 0);
}
#[test]
fn ring_expands_and_checks_visibility_without_changing_anchor() {
    let mut w = World {
        targets: vec![target(1, DVec3::Z * 2.0), target(2, DVec3::X * 4.0)],
        ..Default::default()
    };
    let (mut e, _) = release(&mut w, "fire_ring");
    assert_eq!(tick(&mut e, &mut w, 10).xp, 0);
    assert_eq!(tick(&mut e, &mut w, 20).xp, 4);
    assert_eq!(w.hits, vec![(1, 4.800000000000001)]);
    w.blocked.insert(2);
    tick(&mut e, &mut w, 40);
    assert!(w.hits.iter().all(|h| h.0 == 1));
    assert_eq!(e.position, DVec3::ZERO);
    assert_eq!(w.patches.last().unwrap().1, 5.0);
}
#[test]
fn blast_has_falloff_per_target_rewards_and_one_impact() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "fire_blast");
    w.targets = vec![
        target(1, e.position),
        target(2, e.position + DVec3::X * 6.0),
        target(3, e.position + DVec3::X * 9.0),
    ];
    let r = tick(&mut e, &mut w, 1);
    assert!(r.impact);
    assert_eq!(r.xp, 8);
    assert_eq!(w.hits.len(), 2);
    assert!(w.hits[0].1 > w.hits[1].1);
    assert_eq!(w.breaks, vec![6.0]);
    assert_eq!(w.fire[&1], 200);
    assert_eq!(tick(&mut e, &mut w, 2).xp, 0);
    assert_eq!(w.hits.len(), 2);
}
#[test]
fn beam_pierces_bodies_but_not_walls_and_stops_with_channel() {
    let mut w = World {
        wall: Some(8.0),
        ..Default::default()
    };
    w.targets = vec![
        target(1, DVec3::Z * 3.0),
        target(2, DVec3::Z * 5.0),
        target(3, DVec3::Z * 10.0),
    ];
    let (mut e, r) = release(&mut w, "inferno_beam");
    assert_eq!(r.xp, 4);
    assert_eq!(w.hits.len(), 2);
    assert_eq!(e.end.z, 8.0);
    assert_eq!(tick(&mut e, &mut w, 5).xp, 0);
    assert_eq!(tick(&mut e, &mut w, 6).xp, 4);
    assert_eq!(w.hits.len(), 4);
    let mut o = owner();
    o.channeling = false;
    assert!(
        e.tick(
            &mut w,
            o,
            Catalog::bundled().unwrap().get("inferno_beam").unwrap(),
            107
        )
        .unwrap()
        .finished
    );
}
#[test]
fn meteor_rain_keeps_three_per_wave_and_four_block_margin() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "meteor_rain");
    assert!(tick(&mut e, &mut w, 11).meteors.is_empty());
    let r = tick(&mut e, &mut w, 12);
    assert_eq!(r.meteors.len(), 3);
    for m in r.meteors {
        assert!((m.position - e.position).with_y(0.0).length() <= 56.0);
        assert_eq!(m.velocity, -DVec3::Y * 0.8);
        assert_eq!(m.position.y, 17.0);
    }
    assert!(tick(&mut e, &mut w, 12).meteors.is_empty());
    assert!(tick(&mut e, &mut w, 361).finished);
}
#[test]
fn phoenix_rises_then_dives_and_explodes_exactly_once() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "phoenix_form");
    tick(&mut e, &mut w, 1);
    assert_eq!(w.velocity, Some(DVec3::Y * 0.32));
    assert!(e.dive(DVec3::Z));
    assert!(!e.dive(DVec3::X));
    tick(&mut e, &mut w, 2);
    assert_eq!(w.velocity, Some(DVec3::Z * 4.0));
    w.wall = Some(2.0);
    w.targets = vec![target(1, DVec3::Z * 2.0)];
    let r = tick(&mut e, &mut w, 3);
    assert!(r.impact);
    assert_eq!(e.stage, 2);
    assert_eq!(e.life, 43);
    assert_eq!(r.xp, 4);
    assert_eq!(w.velocity, Some(DVec3::ZERO));
    assert!(!tick(&mut e, &mut w, 4).impact);
    assert_eq!(w.hits.len(), 1);
    assert!(w.breaks.is_empty());
}
#[test]
fn phoenix_automatically_launches_at_one_hundred_and_bursts_by_one_fifty() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "phoenix_form");
    tick(&mut e, &mut w, 100);
    assert_eq!(e.stage, 1);
    assert_eq!(w.velocity, Some(DVec3::Z * 4.0));
    assert!(tick(&mut e, &mut w, 150).impact);
}
#[test]
fn avatar_aura_rewards_once_and_trail_depends_on_motion() {
    let mut w = World {
        targets: vec![target(1, DVec3::X), target(2, -DVec3::X)],
        ..Default::default()
    };
    let (mut e, r) = release(&mut w, "fire_avatar");
    assert_eq!(r.xp, 4);
    assert_eq!(tick(&mut e, &mut w, 10).xp, 4);
    assert_eq!(w.hits.len(), 2);
    assert!(w.patches.is_empty());
    let mut o = owner();
    o.velocity = DVec3::Z;
    e.tick(
        &mut w,
        o,
        Catalog::bundled().unwrap().get("fire_avatar").unwrap(),
        115,
    )
    .unwrap();
    assert_eq!(w.patches, vec![(DVec3::ZERO, 0.75)]);
}
#[test]
fn lifecycle_and_bad_context_never_repeat_damage() {
    let mut w = World::default();
    let (mut e, _) = release(&mut w, "fire_ring");
    e.stop();
    assert!(tick(&mut e, &mut w, 10).finished);
    assert!(w.hits.is_empty());
    let mut o = owner();
    o.direction = DVec3::NAN;
    assert!(
        Effect::release(
            &mut w,
            o,
            Catalog::bundled().unwrap().get("fire_ring").unwrap(),
            0
        )
        .is_err()
    );
    let (mut e, _) = release(&mut w, "fire_ring");
    w.unloaded = true;
    assert!(tick(&mut e, &mut w, 1).finished);
}
