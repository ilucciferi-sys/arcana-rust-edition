use arcana_core::{
    Catalog, DVec3,
    solar::{Bounds, CombatWorld, Target},
    starter::*,
};
#[derive(Default)]
struct Arena {
    targets: Vec<Target>,
    projectiles: Vec<MovingProjectile>,
    wall: Option<f64>,
    hits: Vec<(u64, f64)>,
    fire: u32,
    slow: u32,
    thaw: f32,
    impulses: Vec<(u64, DVec3)>,
    discarded: Vec<u64>,
    reject_damage: bool,
    resistance: f64,
}
impl CombatWorld for Arena {
    fn clip_blocks(&self, a: DVec3, b: DVec3) -> Result<Option<DVec3>, String> {
        Ok(self
            .wall
            .filter(|x| *x >= a.x && *x <= b.x)
            .map(|x| a + (b - a) * ((x - a.x) / (b.x - a.x))))
    }
    fn hostile_targets(&self, _: u64, _: Bounds) -> Vec<Target> {
        self.targets.clone()
    }
    fn damage(&mut self, _: u64, id: u64, power: f64) -> bool {
        if self.reject_damage {
            return false;
        }
        self.hits.push((id, power));
        true
    }
    fn ignite(&mut self, _: u64, ticks: u32) {
        self.fire = ticks;
    }
    fn impulse(&mut self, id: u64, v: DVec3) {
        self.impulses.push((id, v));
    }
    fn break_soft_sphere(&mut self, _: u64, _: DVec3, _: f64) {
        panic!("starters must not destroy terrain");
    }
}
impl StarterWorld for Arena {
    fn damage_starter(&mut self, caster: u64, id: u64, amount: f64, _: DamageStyle) -> bool {
        self.damage(caster, id, amount)
    }
    fn ignite_at_least(&mut self, _: u64, ticks: u32) {
        self.fire = self.fire.max(ticks);
    }
    fn clear_fire(&mut self, _: u64) {
        self.fire = 0;
    }
    fn slow(&mut self, _: u64, ticks: u32) {
        self.slow = ticks;
    }
    fn thaw(&mut self, _: u64, amount: f32) {
        self.thaw = amount;
    }
    fn ignite_surface(&mut self, _: DVec3, _: DVec3) {}
    fn knockback_resistance(&self, _: u64) -> f64 {
        self.resistance
    }
    fn hostile_projectiles(&self, _: u64, _: Bounds) -> Vec<MovingProjectile> {
        self.projectiles.clone()
    }
    fn discard_projectile(&mut self, id: u64) {
        self.discarded.push(id);
    }
}
fn victim(id: u64, x: f64, z: f64) -> Target {
    let p = DVec3::new(x, 1.0, z);
    Target {
        id,
        bounds: Bounds {
            min: p - DVec3::splat(0.25),
            max: p + DVec3::splat(0.25),
        },
        position: p,
        knockback_immune: false,
    }
}
fn bolt(id: &str) -> Bolt {
    Bolt::new(
        1,
        Catalog::bundled().unwrap().get(id).unwrap(),
        DVec3::Y,
        DVec3::X,
    )
    .unwrap()
}
#[test]
fn each_school_preserves_its_original_trajectory() {
    let mut world = Arena::default();
    let mut fire = bolt("spark");
    let mut water = bolt("water_bolt");
    let mut stone = bolt("stone_bolt");
    fire.tick(&mut world, 1, DVec3::ZERO).unwrap();
    water.tick(&mut world, 1, DVec3::ZERO).unwrap();
    stone.tick(&mut world, 1, DVec3::ZERO).unwrap();
    assert_eq!(fire.position, DVec3::new(1.6, 1.0, 0.0));
    assert_eq!(fire.velocity, DVec3::X * 1.6);
    assert!((water.velocity.x - 0.95 * 0.997).abs() < 1e-8);
    assert!((stone.velocity.y + 0.035 * 0.995).abs() < 1e-8);
    assert!((stone.velocity.x - 1.1 * 0.995).abs() < 1e-8);
}
#[test]
fn a_fast_spark_hits_the_nearest_body_without_tunnelling() {
    let mut world = Arena {
        targets: vec![victim(3, 1.5, 0.0), victim(2, 0.9, 0.0)],
        fire: 200,
        ..Default::default()
    };
    let result = bolt("spark").tick(&mut world, 1, DVec3::ZERO).unwrap();
    assert!(matches!(
        result,
        BoltStep::Impact {
            target: Some(2),
            xp: 4,
            ..
        }
    ));
    assert_eq!(world.hits, vec![(2, 4.0)]);
    assert_eq!(world.fire, 200);
    assert_eq!(world.thaw, 0.035);
}
#[test]
fn the_clipped_block_prevents_damage_to_a_body_behind_it() {
    let mut world = Arena {
        targets: vec![victim(2, 1.5, 0.0)],
        wall: Some(0.3),
        ..Default::default()
    };
    assert!(matches!(
        bolt("spark").tick(&mut world, 1, DVec3::ZERO).unwrap(),
        BoltStep::Impact {
            target: None,
            xp: 0,
            ..
        }
    ));
    assert!(world.hits.is_empty());
}
#[test]
fn water_extinguishes_and_slows_but_only_after_successful_damage() {
    let mut world = Arena {
        targets: vec![victim(2, 0.8, 0.0)],
        fire: 200,
        ..Default::default()
    };
    bolt("water_bolt").tick(&mut world, 1, DVec3::ZERO).unwrap();
    assert_eq!(world.fire, 0);
    assert_eq!(world.slow, 35);
    assert_eq!(world.hits, vec![(2, 3.0)]);
    assert!((world.impulses[0].1.x - DVec3::new(0.8, 1.0, 0.0).normalize().x * 0.65).abs() < 1e-8);
    let mut immune = Arena {
        targets: world.targets.clone(),
        fire: 200,
        reject_damage: true,
        ..Default::default()
    };
    bolt("water_bolt")
        .tick(&mut immune, 1, DVec3::ZERO)
        .unwrap();
    assert_eq!((immune.fire, immune.slow), (200, 0));
    assert!(immune.impulses.is_empty());
}
#[test]
fn stone_keeps_its_six_damage_and_one_point_zero_five_impulse() {
    let mut target = victim(2, 0.8, 0.0);
    target.position = DVec3::X;
    let mut world = Arena {
        targets: vec![target],
        ..Default::default()
    };
    bolt("stone_bolt").tick(&mut world, 1, DVec3::ZERO).unwrap();
    assert_eq!(world.hits, vec![(2, 6.0)]);
    assert_eq!(world.impulses[0].1, DVec3::new(1.05, 0.12, 0.0));
}
#[test]
fn gust_checks_sector_visibility_resistance_and_awards_xp_once() {
    let mut world = Arena {
        targets: vec![
            victim(2, 3.0, 0.0),
            victim(3, 5.0, 0.0),
            victim(4, -2.0, 0.0),
            victim(5, 0.0, 5.0),
        ],
        resistance: 0.5,
        ..Default::default()
    };
    let projectile = victim(9, 4.0, 0.0);
    world.projectiles.push(MovingProjectile {
        id: 9,
        bounds: projectile.bounds,
        position: projectile.position,
    });
    let outcome = gust(&mut world, 1, DVec3::Y, DVec3::X, 8.0).unwrap();
    assert_eq!(
        outcome,
        GustOutcome {
            damaged: 2,
            discarded: 1,
            xp: 4
        }
    );
    assert_eq!(world.hits, vec![(2, 2.0), (3, 2.0)]);
    assert_eq!(world.impulses[0].1, DVec3::X * 0.55);
    assert_eq!(world.discarded, vec![9]);
    world.hits.clear();
    world.wall = Some(1.0);
    assert_eq!(
        gust(&mut world, 1, DVec3::Y, DVec3::X, 8.0)
            .unwrap()
            .damaged,
        0
    );
    assert!(world.hits.is_empty());
}
#[test]
fn a_projectile_has_a_finite_range_and_lifetime() {
    let mut p = bolt("water_bolt");
    let mut world = Arena::default();
    let mut expired = false;
    for _ in 0..162 {
        if p.tick(&mut world, 1, DVec3::ZERO).unwrap() == BoltStep::Expired {
            expired = true;
            break;
        }
    }
    assert!(expired);
    assert!(world.hits.is_empty());
}
