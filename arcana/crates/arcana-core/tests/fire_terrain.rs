use arcana_core::{DVec3, fire_terrain::*, solar::Bounds};
use std::collections::HashSet;
#[derive(Default)]
struct World {
    denied: HashSet<Position>,
    wet: HashSet<Position>,
    rain: bool,
    unloaded: bool,
    spread: bool,
    no_spread: HashSet<Position>,
    burned: Vec<Position>,
}
impl Terrain for World {
    fn block(&self, p: Position) -> Option<Block> {
        if self.unloaded {
            return None;
        }
        Some(Block {
            air: p[1] >= 1 && !self.wet.contains(&p),
            fluid: self.wet.contains(&p),
            fire: false,
            flammable: p == [1, 1, 0] && self.spread && !self.burned.contains(&p),
        })
    }
    fn allowed(&self, _: u128, p: Position) -> bool {
        !self.denied.contains(&p)
    }
    fn raining(&self, _: Position) -> bool {
        self.rain
    }
    fn fire_tick(&self, at: Position) -> bool {
        self.spread && !self.no_spread.contains(&at)
    }
    fn owner_present(&self, _: u128) -> bool {
        true
    }
    fn burn_flammable(&mut self, _: u128, p: Position) -> bool {
        if self.denied.contains(&p) {
            return false;
        }
        self.burned.push(p);
        true
    }
    fn one_in_three(&mut self) -> bool {
        true
    }
}
#[test]
fn spread_uses_the_fire_origins_rule_without_stopping_its_age() {
    let mut world = World {
        spread: true,
        no_spread: HashSet::from([[0, 1, 0]]),
        ..Default::default()
    };
    let mut fire = OwnedFire::default();
    assert!(fire.ignite(&world, 42, [0, 1, 0], 0));
    fire.tick(&mut world, 30);
    assert!(world.burned.is_empty());
    assert_eq!(fire.cells().next().unwrap().age, 1);
    world.no_spread.clear();
    fire.tick(&mut world, 55);
    assert_eq!(world.burned, vec![[1, 1, 0]]);
}
#[test]
fn requires_air_dry_support_and_permission_and_keeps_owner() {
    let mut w = World::default();
    let mut f = OwnedFire::default();
    assert!(f.ignite(&w, 42, [0, 1, 0], 0));
    assert!(!f.ignite(&w, 43, [0, 1, 0], 0));
    assert_eq!(f.cells().next().unwrap().owner, 42);
    assert!(!f.ignite(&w, 42, [0, 5, 0], 0));
    w.denied.insert([1, 1, 1]);
    assert!(!f.ignite(&w, 42, [1, 1, 1], 0));
    w.wet.insert([2, 1, 0]);
    assert!(!f.ignite(&w, 42, [2, 1, 0], 0));
}
#[test]
fn owns_fire_only_near_its_owner_and_reports_contact() {
    let w = World::default();
    let mut f = OwnedFire::default();
    f.ignite(&w, 42, [0, 1, 0], 0);
    assert!(f.protects(42, [1, 1, 1]));
    assert!(!f.protects(43, [0, 1, 0]));
    assert!(!f.protects(42, [3, 1, 0]));
    assert_eq!(
        f.touching(Bounds {
            min: DVec3::new(0.2, 1.0, 0.2),
            max: DVec3::new(0.8, 3.0, 0.8)
        }),
        vec![[0, 1, 0]]
    );
}
#[test]
fn ages_out_even_without_spread_but_pauses_in_unloaded_chunks() {
    let mut w = World::default();
    let mut f = OwnedFire::default();
    f.ignite(&w, 42, [0, 1, 0], 0);
    w.unloaded = true;
    assert!(!f.tick(&mut w, 1000));
    assert_eq!(f.cells().next().unwrap().age, 0);
    w.unloaded = false;
    assert!(f.tick(&mut w, 1001));
    assert_eq!(f.cells().next().unwrap().age, 1);
    for n in 1..=12 {
        f.tick(&mut w, 1001 + n * 25);
    }
    assert!(f.is_empty());
}
#[test]
fn water_rain_and_replaced_cells_remove_only_owned_fire() {
    let mut w = World::default();
    let mut f = OwnedFire::default();
    f.ignite(&w, 42, [0, 1, 0], 0);
    w.wet.insert([0, 1, 0]);
    assert!(f.tick(&mut w, 1));
    assert!(f.is_empty());
    w.wet.clear();
    f.ignite(&w, 42, [0, 1, 0], 0);
    w.rain = true;
    assert!(f.tick(&mut w, 1));
    assert!(f.is_empty());
    assert!(w.burned.is_empty());
}
#[test]
fn spread_respects_build_protection_and_fire_tick() {
    let mut w = World {
        spread: true,
        ..Default::default()
    };
    w.denied.insert([1, 1, 0]);
    let mut f = OwnedFire::default();
    f.ignite(&w, 42, [0, 1, 0], 0);
    f.tick(&mut w, 30);
    assert!(w.burned.is_empty());
    w.denied.clear();
    f.tick(&mut w, 55);
    assert_eq!(w.burned, vec![[1, 1, 0]]);
    assert_eq!(f.len(), 2);
}
#[test]
fn journal_round_trip_preserves_owner_and_rejects_duplicate_or_bad_age() {
    let mut f = OwnedFire::default();
    f.ignite(&World::default(), u128::MAX, [0, 1, 0], 0);
    let text = f.save_json().unwrap();
    let restored = OwnedFire::load_json(&text).unwrap();
    assert_eq!(restored.cells().next(), f.cells().next());
    assert!(OwnedFire::load_json(&text.replace("\"age\":0", "\"age\":13")).is_err());
    let cell = serde_json::to_string(f.cells().next().unwrap()).unwrap();
    assert!(OwnedFire::load_json(&format!("{{\"schema\":1,\"cells\":[{cell},{cell}]}}")).is_err());
}
