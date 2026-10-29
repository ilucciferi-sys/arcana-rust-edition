use arcana_core::{
    Catalog, DVec3,
    fire::*,
    solar::{Bounds, CombatWorld, Target},
    starter::{DamageStyle, MovingProjectile, StarterWorld},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct Arena {
    targets: Vec<Target>,
    wall: Option<f64>,
    hidden: HashSet<u64>,
    reject_damage: bool,
    allow_terrain: bool,
    hits: Vec<(u64, f64)>,
    burns: HashMap<u64, u32>,
    impulses: Vec<(u64, DVec3)>,
    thaws: Vec<(u64, f32)>,
    surfaces: u32,
    patches: Vec<(DVec3, f64)>,
    placed_fires: u32,
}
impl CombatWorld for Arena {
    fn clip_blocks(&self, a: DVec3, b: DVec3) -> Result<Option<DVec3>, String> {
        if self.hidden.iter().any(|id| {
            self.targets
                .iter()
                .any(|t| t.id == *id && t.bounds.center() == b)
        }) {
            return Ok(Some((a + b) * 0.5));
        }
        Ok(self
            .wall
            .filter(|x| *x >= a.x && *x <= b.x)
            .map(|x| a + (b - a) * ((x - a.x) / (b.x - a.x))))
    }
    fn hostile_targets(&self, _: u64, _: Bounds) -> Vec<Target> {
        self.targets.clone()
    }
    fn damage(&mut self, _: u64, target: u64, power: f64) -> bool {
        if self.reject_damage {
            return false;
        }
        self.hits.push((target, power));
        true
    }
    fn ignite(&mut self, _: u64, _: u32) {
        panic!("must preserve a longer burn")
    }
    fn impulse(&mut self, id: u64, v: DVec3) {
        self.impulses.push((id, v));
    }
    fn break_soft_sphere(&mut self, _: u64, _: DVec3, _: f64) {
        panic!("these spells do not destroy a sphere")
    }
}
impl StarterWorld for Arena {
    fn damage_starter(&mut self, caster: u64, target: u64, power: f64, _: DamageStyle) -> bool {
        self.damage(caster, target, power)
    }
    fn ignite_at_least(&mut self, id: u64, ticks: u32) {
        let v = self.burns.entry(id).or_default();
        *v = (*v).max(ticks);
    }
    fn clear_fire(&mut self, _: u64) {
        panic!()
    }
    fn slow(&mut self, _: u64, _: u32) {
        panic!()
    }
    fn thaw(&mut self, id: u64, amount: f32) {
        self.thaws.push((id, amount));
    }
    fn ignite_surface(&mut self, _: DVec3, _: DVec3) {
        panic!("ignition uses its own eye ray")
    }
    fn knockback_resistance(&self, _: u64) -> f64 {
        1.0
    }
    fn hostile_projectiles(&self, _: u64, _: Bounds) -> Vec<MovingProjectile> {
        panic!()
    }
    fn discard_projectile(&mut self, _: u64) {
        panic!()
    }
    fn ignite_patch(&mut self, at: DVec3, radius: f64) -> u32 {
        self.patches.push((at, radius));
        self.placed_fires
    }
}
impl FireWorld for Arena {
    fn ignite_ray(&mut self, _: DVec3, _: DVec3, _: f64) -> bool {
        self.surfaces += 1;
        self.allow_terrain
    }
}
fn victim(id: u64, x: f64, z: f64) -> Target {
    let p = DVec3::new(x, 0.0, z);
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
fn ignition(world: &mut Arena) -> Ignition {
    ignite(
        world,
        1,
        Catalog::bundled().unwrap().get("ignition").unwrap(),
        DVec3::ZERO,
        DVec3::Y,
        DVec3::X,
    )
    .unwrap()
}
fn shove(world: &mut Arena) -> AreaOutcome {
    push(
        world,
        1,
        Catalog::bundled().unwrap().get("fire_push").unwrap(),
        DVec3::ZERO,
        DVec3::Y,
        DVec3::X,
    )
    .unwrap()
}

fn fireball() -> arcana_core::starter::Bolt {
    arcana_core::starter::Bolt::new(
        42,
        Catalog::bundled().unwrap().get("fireball").unwrap(),
        DVec3::Y,
        DVec3::X,
    )
    .unwrap()
}
#[test]
fn fireball_blast_uses_three_blocks_flat_six_damage_and_visibility() {
    let mut w = Arena {
        targets: vec![
            victim(1, 1.0, 0.0),
            victim(2, 2.99, 0.0),
            victim(3, 3.01, 0.0),
            victim(4, 2.0, 0.0),
        ],
        hidden: HashSet::from([4]),
        ..Default::default()
    };
    w.burns.insert(1, 240);
    let r = fireball_impact(&mut w, 99, DVec3::ZERO, 6.0).unwrap();
    assert_eq!((r.affected, r.damaged, r.pushed, r.xp), (2, 2, 2, 4));
    assert_eq!(w.hits, vec![(1, 6.0), (2, 6.0)]);
    assert_eq!(w.burns, HashMap::from([(1, 240), (2, 80)]));
    assert_eq!(
        w.impulses,
        vec![
            (1, DVec3::new(0.7, 0.25, 0.0)),
            (2, DVec3::new(0.7, 0.25, 0.0))
        ]
    );
    assert_eq!(w.thaws, vec![(1, 0.08), (2, 0.08)]);
}
#[test]
fn fireball_rejected_damage_preserves_impulse_reward_and_freeze_immunity() {
    let mut frozen = victim(2, 2.0, 0.0);
    frozen.knockback_immune = true;
    let mut w = Arena {
        targets: vec![victim(1, 1.0, 0.0), frozen],
        reject_damage: true,
        ..Default::default()
    };
    let r = fireball_impact(&mut w, 99, DVec3::ZERO, 6.0).unwrap();
    assert_eq!((r.damaged, r.pushed, r.xp), (0, 1, 4));
    assert_eq!(w.thaws, vec![(1, 0.08)]);
    assert_eq!(w.burns.len(), 2);
    w.targets.remove(0);
    w.impulses.clear();
    let r = fireball_impact(&mut w, 99, DVec3::ZERO, 6.0).unwrap();
    assert_eq!(r.xp, 0);
    assert!(w.impulses.is_empty());
}
#[test]
fn fireball_requests_two_block_fire_patch_without_rewarding_terrain_only() {
    let mut w = Arena {
        placed_fires: 5,
        ..Default::default()
    };
    let r = fireball_impact(&mut w, 99, DVec3::Y, 6.0).unwrap();
    assert_eq!(r.xp, 0);
    assert_eq!(w.patches, vec![(DVec3::Y, 2.0)]);
    assert!(fireball_impact(&mut w, 99, DVec3::NAN, 6.0).is_err());
    assert!(fireball_impact(&mut w, 99, DVec3::Y, f64::NAN).is_err());
    assert_eq!(w.patches.len(), 1);
}
#[test]
fn fireball_swept_collision_has_one_blast_and_no_extra_direct_hit() {
    use arcana_core::starter::BoltStep;
    let mut w = Arena {
        targets: vec![victim(1, 1.1, 0.0), victim(2, 0.9, 0.0)],
        ..Default::default()
    };
    let mut bolt = fireball();
    assert!(matches!(
        bolt.tick(&mut w, 99, DVec3::ZERO).unwrap(),
        BoltStep::Impact {
            target: Some(2),
            xp: 4,
            ..
        }
    ));
    assert_eq!(w.hits, vec![(1, 6.0), (2, 6.0)]);
    assert_eq!(w.patches.len(), 1);
    assert_eq!(
        bolt.tick(&mut w, 99, DVec3::ZERO).unwrap(),
        BoltStep::Expired
    );
    assert_eq!(w.hits.len(), 2);
    assert_eq!(w.patches.len(), 1);
}
#[test]
fn fireball_stops_at_wall_and_does_not_blast_a_hidden_body() {
    use arcana_core::starter::BoltStep;
    let mut w = Arena {
        wall: Some(0.3),
        targets: vec![victim(1, 1.5, 0.0)],
        ..Default::default()
    };
    let mut bolt = fireball();
    assert!(matches!(
        bolt.tick(&mut w, 99, DVec3::ZERO).unwrap(),
        BoltStep::Impact {
            target: None,
            xp: 0,
            ..
        }
    ));
    assert_eq!(bolt.position, DVec3::new(0.3, 1.0, 0.0));
    assert!(w.hits.is_empty());
    assert!(w.burns.is_empty());
    assert!(w.impulses.is_empty());
}
#[test]
fn fireball_flies_straight_and_expiry_never_explodes() {
    use arcana_core::starter::BoltStep;
    let mut w = Arena::default();
    let mut bolt = fireball();
    for _ in 0..5 {
        assert_eq!(
            bolt.tick(&mut w, 99, DVec3::ZERO).unwrap(),
            BoltStep::Flying
        );
    }
    assert_eq!(bolt.velocity, DVec3::new(1.2, 0.0, 0.0));
    assert!((bolt.position.x - 6.0).abs() < 1e-9);
    assert_eq!(bolt.position.y, 1.0);
    bolt.range = bolt.travelled - 0.1;
    assert_eq!(
        bolt.tick(&mut w, 99, DVec3::ZERO).unwrap(),
        BoltStep::Expired
    );
    let mut old = fireball();
    old.age = 160;
    assert_eq!(
        old.tick(&mut w, 99, DVec3::ZERO).unwrap(),
        BoltStep::Expired
    );
    assert!(w.patches.is_empty());
    assert!(w.hits.is_empty());
}
#[test]
fn fireball_unlocks_at_level_two_and_keeps_original_cost_and_cancellation() {
    use arcana_core::{
        School,
        casting::{CastPermission, Caster, Phase, Rejection},
        state::MagicState,
    };
    let c = Catalog::bundled().unwrap();
    let mut state = MagicState::default();
    state.awaken(School::Fire, &c);
    let mut caster = Caster::default();
    assert_eq!(
        caster.request(&mut state, &c, "fireball", 0, CastPermission::default()),
        Err(Rejection::Locked)
    );
    assert_eq!(state.mana(), 100.0);
    assert_eq!(state.cooldown("fireball"), 0);
    state.add_xp(100, &c);
    state.fill_mana();
    assert_eq!(state.level(), 2);
    caster
        .request(&mut state, &c, "fireball", 10, CastPermission::default())
        .unwrap();
    assert_eq!(state.mana(), 89.0);
    assert_eq!(state.cooldown("fireball"), 200);
    assert!(caster.advance(24).unwrap().is_empty());
    assert_eq!(
        caster.cancel(&mut state, 24, false).unwrap().phase,
        Phase::Cancelled
    );
    assert_eq!(state.mana(), 115.0);
    assert_eq!(state.cooldown("fireball"), 200);
    assert!(caster.cancel(&mut state, 24, false).is_none());
    assert_eq!(state.mana(), 115.0);
}
#[test]
fn ignition_selects_distance_to_caster_instead_of_first_box_intersection() {
    let mut large = victim(1, 8.0, 0.0);
    large.bounds.min.x = 1.0;
    let mut w = Arena {
        targets: vec![large, victim(2, 4.0, 0.0)],
        ..Default::default()
    };
    let result = ignition(&mut w);
    assert_eq!(result.target, Some(2));
    assert_eq!(result.position, DVec3::new(4.0, 0.0, 0.0));
    assert_eq!(w.hits, vec![(2, 3.0)]);
    assert_eq!(w.burns.get(&2), Some(&80));
    assert_eq!(w.thaws, vec![(2, 0.08)]);
    assert_eq!(result.xp, 4);
}
#[test]
fn ignition_respects_walls_and_original_point_three_five_hitbox_margin() {
    let mut w = Arena {
        targets: vec![victim(1, 5.0, 0.0)],
        wall: Some(3.0),
        ..Default::default()
    };
    let result = ignition(&mut w);
    assert_eq!(result.target, None);
    assert!(!result.damaged);
    assert_eq!(result.xp, 0);
    assert_eq!(result.position, DVec3::new(3.0, 1.0, 0.0));
    assert_eq!(w.surfaces, 1);
    w.wall = None;
    w.targets = vec![victim(2, 5.0, 0.55)];
    assert_eq!(ignition(&mut w).target, Some(2));
    w.targets = vec![victim(3, 5.0, 0.65)];
    assert_eq!(ignition(&mut w).target, None);
}
#[test]
fn ignition_terrain_success_rewards_once_with_or_without_an_entity() {
    let mut w = Arena {
        wall: Some(10.0),
        allow_terrain: true,
        ..Default::default()
    };
    let result = ignition(&mut w);
    assert!(result.surface_ignited);
    assert_eq!(result.xp, 4);
    w.targets = vec![victim(1, 4.0, 0.0)];
    assert_eq!(ignition(&mut w).xp, 4);
    w.wall = None;
    w.targets.clear();
    let calls = w.surfaces;
    assert_eq!(ignition(&mut w).xp, 0);
    assert_eq!(w.surfaces, calls);
}
#[test]
fn rejected_ignition_damage_still_burns_but_does_not_reward_or_thaw() {
    let mut w = Arena {
        targets: vec![victim(1, 5.0, 0.0)],
        reject_damage: true,
        ..Default::default()
    };
    w.burns.insert(1, 240);
    let result = ignition(&mut w);
    assert_eq!(w.burns.get(&1), Some(&240));
    assert_eq!(result.xp, 0);
    assert!(w.thaws.is_empty());
}
#[test]
fn push_checks_sphere_cone_and_visibility_and_awards_only_four_xp() {
    let mut w = Arena {
        targets: vec![
            victim(1, 8.0, 0.0),
            victim(2, 11.9, 0.0),
            victim(3, 12.1, 0.0),
            victim(4, -2.0, 0.0),
            victim(5, 1.0, 3.0),
            victim(6, 4.0, 0.0),
        ],
        hidden: HashSet::from([6]),
        ..Default::default()
    };
    let result = shove(&mut w);
    assert_eq!(
        (result.affected, result.damaged, result.pushed, result.xp),
        (2, 2, 2, 4)
    );
    assert_eq!(w.hits, vec![(1, 3.0), (2, 3.0)]);
    assert_eq!(
        w.impulses,
        vec![
            (1, DVec3::new(0.9, 0.2, 0.0)),
            (2, DVec3::new(0.9, 0.2, 0.0))
        ]
    );
}
#[test]
fn push_without_damage_still_pushes_thaws_and_rewards_but_freeze_blocks_the_impulse() {
    let mut w = Arena {
        targets: vec![victim(1, 5.0, 0.0)],
        reject_damage: true,
        ..Default::default()
    };
    let result = shove(&mut w);
    assert_eq!((result.damaged, result.pushed, result.xp), (0, 1, 4));
    assert_eq!(w.thaws, vec![(1, 0.08)]);
    w.targets[0].knockback_immune = true;
    w.thaws.clear();
    w.impulses.clear();
    let result = shove(&mut w);
    assert_eq!((result.damaged, result.pushed, result.xp), (0, 0, 0));
    assert!(w.impulses.is_empty());
    assert!(w.thaws.is_empty());
    assert_eq!(w.burns.get(&1), Some(&80));
}
#[test]
fn invalid_context_cannot_touch_the_world() {
    let c = Catalog::bundled().unwrap();
    let mut w = Arena::default();
    assert!(
        ignite(
            &mut w,
            1,
            c.get("ignition").unwrap(),
            DVec3::ZERO,
            DVec3::Y,
            DVec3::ZERO
        )
        .is_err()
    );
    assert!(
        push(
            &mut w,
            1,
            c.get("fire_push").unwrap(),
            DVec3::NAN,
            DVec3::Y,
            DVec3::X
        )
        .is_err()
    );
    assert!(
        push(
            &mut w,
            1,
            c.get("ignition").unwrap(),
            DVec3::ZERO,
            DVec3::Y,
            DVec3::X
        )
        .is_err()
    );
    assert!(w.hits.is_empty());
    assert!(w.burns.is_empty());
    assert_eq!(w.surfaces, 0);
}
