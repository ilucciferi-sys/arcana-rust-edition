use arcana_core::{Catalog, DVec3, School, ritual::*, state::MagicState};
use std::collections::{HashMap, HashSet};

#[test]
fn snapshots_accept_missing_rituals_and_reject_unsafe_remote_progress() {
    use arcana_core::network::MagicSnapshot;
    let c = Catalog::bundled().unwrap();
    let mut packet = serde_json::json!({"protocol":1,"server_tick":10,"state":MagicState::default(),"cast":null,"notice":null});
    let decode = |packet: &serde_json::Value| {
        MagicSnapshot::decode(&serde_json::to_vec(packet).unwrap(), &c)
    };
    assert!(decode(&packet).unwrap().ritual.is_none());
    packet["ritual"] = serde_json::json!({"school":"air","center":[0,120,0],"dimension":"minecraft:overworld","progress":95,"duration":120});
    assert_eq!(decode(&packet).unwrap().ritual.unwrap().progress, 95);
    packet["ritual"]["duration"] = 0.into();
    assert!(decode(&packet).is_err());
    packet["ritual"]["duration"] = 120.into();
    packet["ritual"]["progress"] = 121.into();
    assert!(decode(&packet).is_err());
}

struct Fixture {
    blocks: HashMap<[i32; 3], Block>,
    unloaded: HashSet<[i32; 3]>,
    sky: bool,
    busy: bool,
}
impl World for Fixture {
    fn block(&self, at: [i32; 3]) -> Option<Block> {
        if self.unloaded.contains(&at) {
            return None;
        }
        Some(self.blocks.get(&at).cloned().unwrap_or(Block {
            id: "minecraft:air".into(),
            lit: false,
            fluid: Fluid::None,
        }))
    }
    fn can_see_sky(&self, _: [i32; 3]) -> bool {
        self.sky
    }
    fn occupied(&self, _: &str, _: [i32; 3]) -> bool {
        self.busy
    }
}
// Independent construction fixture, matching the original Java GameTest builder.
fn fixture(school: School, y: i32) -> Fixture {
    let mut w = Fixture {
        blocks: HashMap::new(),
        unloaded: HashSet::new(),
        sky: true,
        busy: false,
    };
    let mut put = |x, dy, z, id: &str, lit, fluid| {
        w.blocks.insert(
            [x, y + dy, z],
            Block {
                id: format!("minecraft:{id}"),
                lit,
                fluid,
            },
        );
    };
    match school {
        School::Water => {
            put(0, 0, 0, "prismarine", false, Fluid::None);
            for x in -1..=1 {
                for z in -1..=1 {
                    if x != 0 || z != 0 {
                        put(x, 0, z, "prismarine_bricks", false, Fluid::None);
                        put(x, 1, z, "water", false, Fluid::WaterSource);
                    }
                }
            }
            for x in [-2, 2] {
                for z in [-2, 2] {
                    put(x, 0, z, "sea_lantern", false, Fluid::None);
                }
            }
        }
        School::Fire | School::Earth | School::Air => {
            let names = match school {
                School::Fire => ["obsidian", "obsidian", "magma_block", "obsidian"],
                School::Earth => [
                    "polished_deepslate",
                    "deepslate",
                    "copper_block",
                    "iron_ore",
                ],
                _ => ["quartz_block", "smooth_quartz", "quartz_block", "glass"],
            };
            put(0, 0, 0, names[0], false, Fluid::None);
            for d in [1, 2] {
                for [x, z] in [[d, 0], [-d, 0], [0, d], [0, -d]] {
                    put(x, 0, z, names[d as usize], false, Fluid::None);
                    if school == School::Air && d == 2 {
                        put(x, 1, z, "end_rod", false, Fluid::None);
                    }
                }
            }
            let d = if school == School::Air { 1 } else { 2 };
            for x in [-d, d] {
                for z in [-d, d] {
                    put(x, 0, z, names[3], false, Fluid::None);
                    if school == School::Fire {
                        put(x, 1, z, "campfire", true, Fluid::None);
                    }
                }
            }
            if school == School::Fire {
                put(3, 0, 0, "lava", false, Fluid::Lava);
            }
        }
        _ => panic!("necromancy is not an elemental ritual"),
    }
    w
}
fn context(y: i32) -> Context {
    Context {
        position: DVec3::new(0.5, f64::from(y) + 1.0, 0.5),
        dimension: "minecraft:overworld".into(),
        alive: true,
        spectator: false,
        sleeping: false,
        passenger: false,
        empty_hands: true,
        sneaking: true,
        casting: false,
    }
}
fn start(
    t: &mut Tracker,
    w: &Fixture,
    cx: &Context,
    s: &mut MagicState,
    c: &Catalog,
) -> Option<Event> {
    for _ in 0..19 {
        assert!(t.tick(w, cx, s, c).is_none());
    }
    t.tick(w, cx, s, c)
}
#[test]
fn all_four_original_patterns_awaken_after_twenty_plus_one_hundred_twenty_ticks() {
    let c = Catalog::bundled().unwrap();
    for (school, y, selected) in [
        (School::Fire, 74, "spark"),
        (School::Water, 74, "water_bolt"),
        (School::Earth, 64, "stone_bolt"),
        (School::Air, 120, "wind_gust"),
    ] {
        let w = fixture(school, y);
        let mut s = MagicState::default();
        let mut t = Tracker::default();
        let mut cx = context(y);
        assert!(matches!(
            start(&mut t, &w, &cx, &mut s, &c),
            Some(Event::Started(_))
        ));
        assert!(s.school().is_none());
        cx.sneaking = false;
        for progress in 1..120 {
            let event = t.tick(&w, &cx, &mut s, &c);
            assert!(s.school().is_none());
            if school == School::Air && progress == 95 {
                assert!(matches!(event, Some(Event::Levitation(_))));
            }
        }
        assert!(matches!(
            t.tick(&w, &cx, &mut s, &c),
            Some(Event::Completed(_))
        ));
        assert_eq!(s.school(), Some(school));
        assert_eq!(
            (s.level(), s.xp(), s.mana(), s.selected()),
            (1, 0, 100.0, Some(selected))
        );
        assert!(t.active().is_none());
    }
}
#[test]
fn every_required_cell_and_loaded_chunk_matters() {
    for (school, y, count) in [
        (School::Fire, 74, 18),
        (School::Water, 74, 21),
        (School::Earth, 64, 13),
        (School::Air, 120, 17),
    ] {
        let mut w = fixture(school, y);
        assert_eq!(w.blocks.len(), count);
        assert!(matches(&w, school, [0, y, 0]));
        for pos in w.blocks.keys().copied().collect::<Vec<_>>() {
            let b = w.blocks.remove(&pos).unwrap();
            assert!(!matches(&w, school, [0, y, 0]), "missing {pos:?}");
            w.blocks.insert(pos, b);
            w.unloaded.insert(pos);
            assert!(!matches(&w, school, [0, y, 0]));
            w.unloaded.clear();
        }
    }
    let mut fire = fixture(School::Fire, 74);
    fire.blocks.get_mut(&[2, 75, 2]).unwrap().lit = false;
    assert!(!matches(&fire, School::Fire, [0, 74, 0]));
    let mut water = fixture(School::Water, 74);
    water.blocks.get_mut(&[1, 75, 1]).unwrap().fluid = Fluid::OtherWater;
    assert!(!matches(&water, School::Water, [0, 74, 0]));
}
#[test]
fn height_and_open_sky_are_required_even_when_the_structure_is_correct() {
    assert!(!matches(
        &fixture(School::Earth, 65),
        School::Earth,
        [0, 65, 0]
    ));
    assert!(!matches(
        &fixture(School::Air, 119),
        School::Air,
        [0, 119, 0]
    ));
    let mut air = fixture(School::Air, 120);
    air.sky = false;
    assert!(!matches(&air, School::Air, [0, 120, 0]));
    assert!(pattern(School::Necromancy).is_none());
}
#[test]
fn occupied_circle_and_existing_bond_cannot_be_overwritten() {
    let c = Catalog::bundled().unwrap();
    let mut w = fixture(School::Fire, 74);
    let mut s = MagicState::default();
    w.busy = true;
    assert!(matches!(
        start(&mut Tracker::default(), &w, &context(74), &mut s, &c),
        Some(Event::Rejected(Rejection::Occupied))
    ));
    assert!(s.school().is_none());
    w.busy = false;
    s.awaken(School::Water, &c);
    assert!(matches!(
        start(&mut Tracker::default(), &w, &context(74), &mut s, &c),
        Some(Event::Rejected(Rejection::AlreadyBound))
    ));
    assert_eq!(s.school(), Some(School::Water));
}
#[test]
fn start_guards_and_release_to_retry_match_the_original() {
    let c = Catalog::bundled().unwrap();
    let w = fixture(School::Fire, 74);
    for guard in 0..6 {
        let mut cx = context(74);
        match guard {
            0 => cx.empty_hands = false,
            1 => cx.alive = false,
            2 => cx.spectator = true,
            3 => cx.sleeping = true,
            4 => cx.passenger = true,
            _ => cx.casting = true,
        }
        let mut t = Tracker::default();
        let mut s = MagicState::default();
        assert!(start(&mut t, &w, &cx, &mut s, &c).is_none());
        cx = context(74);
        for _ in 0..30 {
            assert!(t.tick(&w, &cx, &mut s, &c).is_none());
        }
        cx.sneaking = false;
        t.tick(&w, &cx, &mut s, &c);
        cx.sneaking = true;
        assert!(matches!(
            start(&mut t, &w, &cx, &mut s, &c),
            Some(Event::Started(_))
        ));
    }
}
#[test]
fn lifecycle_and_distance_interrupt_without_granting_a_school() {
    let c = Catalog::bundled().unwrap();
    let w = fixture(School::Fire, 74);
    for guard in 0..5 {
        let mut t = Tracker::default();
        let mut s = MagicState::default();
        let mut cx = context(74);
        start(&mut t, &w, &cx, &mut s, &c);
        match guard {
            0 => cx.alive = false,
            1 => cx.spectator = true,
            2 => cx.passenger = true,
            3 => cx.dimension = "minecraft:the_nether".into(),
            _ => cx.position.x += 3.0,
        }
        assert!(matches!(
            t.tick(&w, &cx, &mut s, &c),
            Some(Event::Cancelled(_))
        ));
        assert!(s.school().is_none());
        assert!(t.active().is_none());
    }
}
#[test]
fn final_recheck_prevents_a_late_broken_pattern_from_binding_permanently() {
    let c = Catalog::bundled().unwrap();
    let mut w = fixture(School::Fire, 74);
    let mut t = Tracker::with_duration(47).unwrap();
    let mut s = MagicState::default();
    let cx = context(74);
    start(&mut t, &w, &cx, &mut s, &c);
    for _ in 0..46 {
        t.tick(&w, &cx, &mut s, &c);
    }
    w.blocks.remove(&[1, 74, 0]);
    assert!(matches!(
        t.tick(&w, &cx, &mut s, &c),
        Some(Event::Cancelled(_))
    ));
    assert!(s.school().is_none());
    assert!(Tracker::with_duration(39).is_err());
    assert!(Tracker::with_duration(1201).is_err());
}
