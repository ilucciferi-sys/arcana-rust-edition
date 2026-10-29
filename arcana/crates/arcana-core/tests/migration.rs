use arcana_core::{Catalog, DVec3, School, casting::*, network::*, solar::*, state::*};

fn ready() -> (Catalog, MagicState, Caster) {
    let catalog = Catalog::bundled().unwrap();
    let mut state = MagicState::default();
    assert!(state.awaken(School::Fire, &catalog));
    state.set_level(5, &catalog);
    state.fill_mana();
    (catalog, state, Caster::default())
}

#[test]
fn all_original_content_survives() {
    let c = Catalog::bundled().unwrap();
    for (school, count) in [
        (School::Fire, 15),
        (School::Water, 15),
        (School::Earth, 15),
        (School::Air, 15),
        (School::Necromancy, 16),
    ] {
        assert_eq!(
            c.spells().iter().filter(|s| s.school == school).count(),
            count
        );
    }
    let s = c.get("solar_spear").unwrap();
    assert_eq!(
        (s.mana, s.cooldown, s.cast_time, s.range, s.power, s.radius),
        (128.0, 1400, 70, 32.0, 48.0, 12.0)
    );
    assert_eq!(s.name, "Солнечное копьё");
    assert_eq!(c.spells().iter().filter(|s| s.executable()).count(), 8);
}

#[test]
fn progression_boundaries_and_level_reduction() {
    for (xp, level) in [
        (0, 1),
        (99, 1),
        (100, 2),
        (349, 2),
        (350, 3),
        (799, 3),
        (800, 4),
        (1599, 4),
        (1600, 5),
        (u32::MAX, 5),
    ] {
        assert_eq!(level_for_xp(xp), level);
    }
    let (c, mut s, _) = ready();
    assert!(s.select("solar_spear"));
    s.set_level(1, &c);
    assert!(!s.equipped("solar_spear"));
    assert_eq!(s.selected(), Some("spark"));
    assert_eq!(s.max_mana(), 100.0);
}

#[test]
fn cancellation_refunds_mana_and_retains_reserved_cooldown() {
    let (c, mut s, mut caster) = ready();
    caster
        .request(&mut s, &c, "solar_spear", 100, CastPermission::default())
        .unwrap();
    assert_eq!(s.mana(), 32.0);
    caster.cancel(&mut s, 110, false).unwrap();
    assert_eq!(s.mana(), 160.0);
    assert_eq!(s.cooldown("solar_spear"), 1400);
    assert_eq!(
        caster.request(&mut s, &c, "solar_spear", 111, CastPermission::default()),
        Err(Rejection::Cooldown)
    );
}

#[test]
fn interrupted_preparation_is_refunded_once_when_saved() {
    let (c, mut s, mut caster) = ready();
    caster
        .request(&mut s, &c, "solar_spear", 10, CastPermission::default())
        .unwrap();
    let snapshot = caster.saved_state(&s);
    assert_eq!(snapshot.mana(), 160.0);
    assert_eq!(snapshot.cooldown("solar_spear"), 1400);
    assert_eq!(s.mana(), 32.0);
    let loaded = MagicState::load_json(&snapshot.save_json().unwrap(), &c).unwrap();
    assert_eq!(Caster::default().saved_state(&loaded).mana(), 160.0);
    caster.advance(80).unwrap();
    assert_eq!(caster.saved_state(&s).mana(), 32.0);
}

#[test]
fn release_is_exactly_once_even_when_tick_is_skipped() {
    let (c, mut s, mut caster) = ready();
    caster
        .request(&mut s, &c, "solar_spear", 10, CastPermission::default())
        .unwrap();
    assert!(caster.advance(79).unwrap().is_empty());
    assert_eq!(caster.advance(82).unwrap()[0].phase, Phase::Release);
    assert!(caster.advance(82).unwrap().is_empty());
    assert!(caster.cancel(&mut s, 83, false).is_none());
    assert_eq!(s.mana(), 32.0);
    assert_eq!(caster.advance(96).unwrap()[0].phase, Phase::Finished);
    assert!(caster.idle());
}

#[test]
fn a_large_clock_jump_does_not_drop_the_release() {
    let (c, mut s, mut caster) = ready();
    caster
        .request(&mut s, &c, "solar_spear", 0, CastPermission::default())
        .unwrap();
    assert_eq!(
        caster
            .advance(500)
            .unwrap()
            .iter()
            .map(|e| e.phase)
            .collect::<Vec<_>>(),
        vec![Phase::Release, Phase::Finished]
    );
    assert!(caster.advance(501).unwrap().is_empty());
}

#[test]
fn unsupported_mechanics_do_not_consume_resources() {
    let (c, mut s, mut caster) = ready();
    assert_eq!(
        caster.request(&mut s, &c, "flame_wall", 0, CastPermission::default()),
        Err(Rejection::UnsupportedMechanic)
    );
    assert_eq!(s.mana(), 160.0);
    assert_eq!(s.cooldown("flame_wall"), 0);
}

#[test]
fn authoritative_permissions_and_monotonic_clock() {
    let (c, mut s, mut caster) = ready();
    let p = CastPermission {
        direction_valid: false,
        ..Default::default()
    };
    assert_eq!(
        caster.request(&mut s, &c, "solar_spear", 10, p),
        Err(Rejection::InvalidDirection)
    );
    assert_eq!(
        caster.request(&mut s, &c, "solar_spear", 9, CastPermission::default()),
        Err(Rejection::ClockWentBackwards)
    );
    assert_eq!(
        caster.request(&mut s, &c, "solar_spear", 11, CastPermission::default()),
        Err(Rejection::RateLimited)
    );
    assert_eq!(s.mana(), 160.0);
}

#[test]
fn no_regeneration_during_cast_and_cooldowns_use_ticks() {
    let (c, mut s, mut caster) = ready();
    caster
        .request(&mut s, &c, "solar_spear", 0, CastPermission::default())
        .unwrap();
    s.tick(true);
    assert_eq!(s.mana(), 32.0);
    assert_eq!(s.cooldown("solar_spear"), 1399);
    s.tick(false);
    assert!((s.mana() - 32.1).abs() < 1e-8);
}

#[test]
fn saves_round_trip_and_do_not_replenish_spent_resurrections() {
    let c = Catalog::bundled().unwrap();
    let mut s = MagicState::default();
    s.awaken(School::Necromancy, &c);
    assert!(s.spend_resurrection());
    s.set_level(4, &c);
    let loaded = MagicState::load_json(&s.save_json().unwrap(), &c).unwrap();
    assert_eq!(s, loaded);
    assert_eq!(loaded.resurrection_charges(), 0);
    s.set_level(5, &c);
    assert_eq!(s.resurrection_charges(), 1);
    s.set_level(1, &c);
    s.set_level(5, &c);
    assert_eq!(s.resurrection_charges(), 1);
    assert_eq!(s.army_limit(), 12);
    assert!(
        MagicState::load_json(
            &s.save_json()
                .unwrap()
                .replace("\"schema\": 1", "\"schema\": 999"),
            &c
        )
        .is_err()
    );
}

#[derive(Default)]
struct Arena {
    wall: Option<f64>,
    targets: Vec<Target>,
    damage: Vec<(u64, f64)>,
    burns: Vec<u64>,
    pushes: Vec<u64>,
    breaks: usize,
}
impl CombatWorld for Arena {
    fn clip_blocks(&self, start: DVec3, end: DVec3) -> Result<Option<DVec3>, String> {
        Ok(self
            .wall
            .filter(|x| *x >= start.x && *x <= end.x)
            .map(|x| DVec3::new(x, start.y, start.z)))
    }
    fn hostile_targets(&self, _: u64, _: Bounds) -> Vec<Target> {
        self.targets.clone()
    }
    fn damage(&mut self, _: u64, id: u64, p: f64) -> bool {
        self.damage.push((id, p));
        true
    }
    fn ignite(&mut self, id: u64, _: u32) {
        self.burns.push(id);
    }
    fn impulse(&mut self, id: u64, _: DVec3) {
        self.pushes.push(id);
    }
    fn break_soft_sphere(&mut self, _: u64, _: DVec3, _: f64) {
        self.breaks += 1;
    }
}
fn target(id: u64, x: f64) -> Target {
    let p = DVec3::new(x, 1.0, 0.0);
    Target {
        id,
        bounds: Bounds {
            min: p - DVec3::splat(0.5),
            max: p + DVec3::splat(0.5),
        },
        position: p,
        knockback_immune: false,
    }
}
fn fire(arena: &mut Arena, griefing: bool) -> SolarOutcome {
    execute(
        arena,
        1,
        Catalog::bundled().unwrap().get("solar_spear").unwrap(),
        DVec3::Y,
        DVec3::X,
        griefing,
    )
    .unwrap()
}

#[test]
fn nearest_body_clips_the_beam_and_is_not_hit_twice() {
    let mut a = Arena {
        wall: Some(25.0),
        targets: vec![target(3, 21.0), target(2, 8.0)],
        ..Default::default()
    };
    let out = fire(&mut a, false);
    assert_eq!(out.end, DVec3::new(7.5, 1.0, 0.0));
    assert!(out.impacted);
    assert_eq!(a.damage.len(), 1);
    assert_eq!(a.damage[0].0, 2);
    assert_eq!(out.xp_awarded, 4);
    assert_eq!(a.breaks, 0);
}

#[test]
fn miss_does_not_explode_or_damage() {
    let mut a = Arena::default();
    let out = fire(&mut a, true);
    assert!(!out.impacted);
    assert_eq!(out.end, DVec3::new(32.0, 1.0, 0.0));
    assert!(a.damage.is_empty());
    assert_eq!(a.breaks, 0);
}

#[test]
fn original_blast_falloff_and_freeze_knockback_immunity() {
    let mut victim = target(2, 10.0);
    victim.position.z = 12.0;
    victim.bounds = Bounds {
        min: victim.position - DVec3::splat(0.5),
        max: victim.position + DVec3::splat(0.5),
    };
    victim.knockback_immune = true;
    let mut a = Arena {
        wall: Some(10.0),
        targets: vec![victim],
        ..Default::default()
    };
    fire(&mut a, true);
    assert!((a.damage[0].1 - 4.8).abs() < 1e-8);
    assert!(a.pushes.is_empty());
    assert_eq!(a.burns, vec![2]);
    assert_eq!(a.breaks, 1);
}

#[test]
fn invalid_direction_cannot_modify_the_world() {
    let mut a = Arena {
        wall: Some(10.0),
        ..Default::default()
    };
    let c = Catalog::bundled().unwrap();
    assert!(
        execute(
            &mut a,
            1,
            c.get("solar_spear").unwrap(),
            DVec3::Y,
            DVec3::X * 20.0,
            true
        )
        .is_err()
    );
    assert_eq!(a.breaks, 0);
}

#[test]
fn two_replicas_share_ticks_and_reject_duplicate_impacts() {
    let (c, mut s, mut caster) = ready();
    let cast = caster
        .request(&mut s, &c, "solar_spear", 200, CastPermission::default())
        .unwrap();
    let event = ServerEvent {
        protocol: PROTOCOL,
        session: 45,
        event_id: 1,
        cast,
        solar: None,
    };
    let wire = serde_json::to_vec(&event).unwrap();
    let decoded = ServerEvent::decode(&wire).unwrap();
    let mut a = Replica::new(45);
    let mut b = Replica::new(45);
    assert!(a.accept(&decoded));
    assert!(b.accept(&decoded));
    assert!(!b.accept(&decoded));
    assert!(!Replica::new(46).accept(&decoded));
    assert_eq!(Replica::age_ticks(&decoded, 230.5), 30.5);
    assert!(ServerEvent::decode(&vec![0; MAX_PACKET + 1]).is_err());
}

#[test]
fn client_actions_cannot_supply_combat_facts() {
    assert!(
        serde_json::from_str::<Action>(r#"{"kind":"Cast","spell_id":"solar_spear","damage":9000}"#)
            .is_err()
    );
}
