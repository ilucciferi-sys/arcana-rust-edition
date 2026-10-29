use super::*;
use arcana_core::starter::{Bolt, BoltKind, BoltStep, DamageStyle, MovingProjectile, StarterWorld};
use horizon_client::projectiles::ProjectileKind;

pub(super) struct ActiveBolt {
    pub born: u64,
    pub bolt: Bolt,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub(super) enum VisualStage {
    Preparation,
    Flight,
    Impact,
    Expired,
    Gust,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StarterVisual {
    pub owner: u32,
    pub cast_id: u64,
    pub dimension: String,
    pub spell_id: String,
    pub position: DVec3,
    pub direction: DVec3,
    pub velocity: DVec3,
    pub age: u32,
    pub stage: VisualStage,
}
pub(super) struct Viewed {
    pub message: StarterVisual,
    pub received: Instant,
}
impl Game<'_> {
    pub(super) fn arcana_starter_visual(
        &mut self,
        cast_id: u64,
        spell_id: &str,
        position: DVec3,
        direction: DVec3,
        velocity: DVec3,
        age: u32,
        stage: VisualStage,
    ) {
        self.arcana.starter_outbox.push(StarterVisual {
            owner: self.lan_acting.unwrap_or(0),
            cast_id,
            dimension: self.dim.id().into(),
            spell_id: spell_id.into(),
            position,
            direction,
            velocity,
            age,
            stage,
        });
    }
    pub(super) fn arcana_release_starter(
        &mut self,
        actor: &mut Actor,
        event: &CastEvent,
        start: DVec3,
        direction: DVec3,
    ) -> bool {
        let spell = catalog().get(&event.spell_id).unwrap();
        if BoltKind::for_spell(&spell.id).is_some() {
            let total = actor.bolts.len()
                + self
                    .arcana
                    .actors
                    .values()
                    .filter(|a| a.dimension == self.dim.id())
                    .map(|a| a.bolts.len())
                    .sum::<usize>();
            if total < 192
                && actor.bolts.len() < 48
                && let Ok(bolt) = Bolt::new(event.cast_id, spell, start, direction)
            {
                self.arcana_starter_visual(
                    bolt.id,
                    &spell.id,
                    bolt.position,
                    direction,
                    bolt.velocity,
                    0,
                    VisualStage::Flight,
                );
                actor.bolts.push(ActiveBolt {
                    born: event.server_tick,
                    bolt,
                });
            }
            true
        } else if spell.id == "wind_gust" {
            let Some(eye) = self.eye_position().map(DVec3::from_array) else {
                return true;
            };
            let owner = self.acting_player_uuid();
            let caster = u64::from(self.lan_acting.unwrap_or(0));
            let mut world = PilotWorld { game: self, owner };
            if let Ok(result) =
                arcana_core::starter::gust(&mut world, caster, eye, direction, spell.range)
            {
                actor.state.add_xp(result.xp, catalog());
                log::info!(
                    "Arcana gust: damaged={}, discarded={}",
                    result.damaged,
                    result.discarded
                );
            }
            self.arcana_starter_visual(
                event.cast_id,
                &spell.id,
                start,
                direction,
                DVec3::ZERO,
                0,
                VisualStage::Gust,
            );
            true
        } else {
            false
        }
    }
    pub(super) fn arcana_tick_starters(&mut self, actor: &mut Actor, tick: u64) {
        if let Some(event) = actor.caster.status(tick)
            && event.phase == Phase::Preparation
            && event.spell_id != "solar_spear"
            && tick.is_multiple_of(2)
            && let Some((at, direction)) = self.arcana_context()
        {
            self.arcana_starter_visual(
                event.cast_id,
                &event.spell_id,
                at,
                direction,
                DVec3::ZERO,
                (tick - event.start_tick) as u32,
                VisualStage::Preparation,
            );
        }
        let owner = self.acting_player_uuid();
        let caster = u64::from(self.lan_acting.unwrap_or(0));
        let caster_position = self
            .player
            .as_ref()
            .map_or(DVec3::ZERO, |p| core(p.position));
        for mut active in std::mem::take(&mut actor.bolts) {
            if active.born == tick {
                actor.bolts.push(active);
                continue;
            }
            let mut world = PilotWorld { game: self, owner };
            let step = active.bolt.tick(&mut world, caster, caster_position);
            let bolt = &active.bolt;
            let direction = bolt.velocity.normalize_or_zero();
            match step {
                Ok(BoltStep::Flying) => {
                    if bolt.age % 2 == 0 {
                        self.arcana_starter_visual(
                            bolt.id,
                            bolt.kind.spell_id(),
                            bolt.position,
                            direction,
                            bolt.velocity,
                            bolt.age,
                            VisualStage::Flight,
                        );
                    }
                    actor.bolts.push(active);
                }
                Ok(BoltStep::Impact { xp, target, .. }) => {
                    actor.state.add_xp(xp, catalog());
                    actor.dirty = true;
                    self.arcana_starter_visual(
                        bolt.id,
                        bolt.kind.spell_id(),
                        bolt.position,
                        direction,
                        DVec3::ZERO,
                        bolt.age,
                        VisualStage::Impact,
                    );
                    log::info!(
                        "Arcana {} impact: target={target:?}, xp={xp}",
                        bolt.kind.spell_id()
                    );
                }
                _ => self.arcana_starter_visual(
                    bolt.id,
                    bolt.kind.spell_id(),
                    bolt.position,
                    direction,
                    DVec3::ZERO,
                    bolt.age,
                    VisualStage::Expired,
                ),
            }
        }
    }
    pub(in crate::game) fn arcana_receive_starter(&mut self, bytes: &[u8]) {
        if bytes.len() > MAX_PACKET {
            return;
        }
        let Ok(message) = serde_json::from_slice::<StarterVisual>(bytes) else {
            return;
        };
        if !message.position.is_finite()
            || !message.direction.is_finite()
            || !message.velocity.is_finite()
            || message.age > 200
        {
            return;
        }
        if !["spark", "water_bolt", "stone_bolt", "wind_gust"].contains(&message.spell_id.as_str())
        {
            return;
        }
        let key = (message.owner, message.cast_id);
        if message.stage == VisualStage::Expired {
            self.arcana.starter_views.remove(&key);
        } else if self.arcana.starter_views.len() < 512
            || self.arcana.starter_views.contains_key(&key)
        {
            self.arcana.starter_views.insert(
                key,
                Viewed {
                    message,
                    received: Instant::now(),
                },
            );
        }
    }
}
impl StarterWorld for PilotWorld<'_, '_> {
    fn damage_starter(&mut self, _: u64, id: u64, amount: f64, style: DamageStyle) -> bool {
        let from = self
            .game
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        let Some(m) = self.game.mobs.iter_mut().find(|m| u64::from(m.id) == id) else {
            return false;
        };
        m.damage(
            amount as f32,
            Damage {
                kind: match style {
                    DamageStyle::Magic => DamageKind::Magic,
                    DamageStyle::Thrown => DamageKind::Thrown,
                },
                attacker: Some(Who::Player),
                from,
            },
        )
    }
    fn ignite_at_least(&mut self, id: u64, ticks: u32) {
        if let Some(m) = self.game.mobs.iter_mut().find(|m| u64::from(m.id) == id) {
            m.fire_ticks = m.fire_ticks.max(ticks as i32);
        }
    }
    fn clear_fire(&mut self, id: u64) {
        if let Some(m) = self.game.mobs.iter_mut().find(|m| u64::from(m.id) == id) {
            m.fire_ticks = 0;
        }
    }
    fn slow(&mut self, id: u64, ticks: u32) {
        if let Some(m) = self.game.mobs.iter_mut().find(|m| u64::from(m.id) == id) {
            m.add_effect("minecraft:slowness", ticks as i32, 0);
        }
    }
    fn thaw(&mut self, _: u64, _: f32) { /* Arcana's cold-state adapter arrives with the ice spells. */
    }
    fn ignite_surface(&mut self, _: DVec3, _: DVec3) { /* Terrain ignition remains disabled until owned-fire protection is ported. */
    }
    fn knockback_resistance(&self, id: u64) -> f64 {
        self.game
            .mobs
            .iter()
            .find(|m| u64::from(m.id) == id)
            .map_or(0.0, |m| m.attrs.knockback_resistance)
    }
    fn hostile_projectiles(&self, _: u64, bounds: Bounds) -> Vec<MovingProjectile> {
        let allies = self.game.player_allies();
        self.game
            .creatures
            .projectiles
            .iter()
            .filter(|p| {
                matches!(
                    p.kind,
                    ProjectileKind::Arrow { .. } | ProjectileKind::Snowball | ProjectileKind::Egg
                ) && p.velocity.length_squared() > 0.01
            })
            .filter(|p| match p.owner {
                Some(Who::Player) => false,
                Some(Who::Mob(id)) => !allies.contains(&id),
                None => true,
            })
            .filter_map(|p| {
                let position = core(p.position);
                let half = if matches!(p.kind, ProjectileKind::Arrow { .. }) {
                    0.25
                } else {
                    0.125
                };
                let b = Bounds {
                    min: position - DVec3::new(half, 0.0, half),
                    max: position + DVec3::new(half, half * 2.0, half),
                };
                if b.max.cmplt(bounds.min).any() || b.min.cmpgt(bounds.max).any() {
                    return None;
                }
                Some(MovingProjectile {
                    id: u64::from(p.id),
                    bounds: b,
                    position,
                })
            })
            .collect()
    }
    fn discard_projectile(&mut self, id: u64) {
        self.game
            .creatures
            .projectiles
            .retain(|p| u64::from(p.id) != id);
    }
}
