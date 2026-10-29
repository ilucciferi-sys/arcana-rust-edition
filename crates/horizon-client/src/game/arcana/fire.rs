//! Combat adapter for the two first-level fire spatial attacks.
use super::*;
use starters::VisualStage;

impl Game<'_> {
    pub(super) fn arcana_release_fire(
        &mut self,
        actor: &mut Actor,
        event: &CastEvent,
        direction: DVec3,
    ) -> bool {
        if !["ignition", "fire_push"].contains(&event.spell_id.as_str()) {
            return false;
        }
        let Some(position) = self.player.as_ref().map(|p| core(p.position)) else {
            return true;
        };
        let Some(eye) = self.eye_position().map(DVec3::from_array) else {
            return true;
        };
        let caster = u64::from(self.lan_acting.unwrap_or(0));
        let owner = self.acting_player_uuid();
        let spell = catalog().get(&event.spell_id).unwrap();
        let mut world = PilotWorld { game: self, owner };
        if event.spell_id == "ignition" {
            match arcana_core::fire::ignite(&mut world, caster, spell, position, eye, direction) {
                Ok(result) => {
                    actor.state.add_xp(result.xp, catalog());
                    if result.surface_hit && !result.surface_ignited {
                        actor.notice = Some((
                            "Поджигание блоков пока недоступно.".into(),
                            self.world.game_time.max(0) as u64 + 100,
                        ));
                    }
                    log::info!(
                        "Arcana ignition: target={:?}, damaged={}, terrain={}, xp={}",
                        result.target,
                        result.damaged,
                        result.surface_ignited,
                        result.xp
                    );
                    self.arcana_starter_visual(
                        event.cast_id,
                        &event.spell_id,
                        result.position,
                        direction,
                        DVec3::ZERO,
                        0,
                        VisualStage::Ignition,
                    );
                }
                Err(error) => log::warn!("Arcana ignition rejected: {error}"),
            }
        } else {
            match arcana_core::fire::push(&mut world, caster, spell, position, eye, direction) {
                Ok(result) => {
                    actor.state.add_xp(result.xp, catalog());
                    log::info!(
                        "Arcana fire_push: affected={}, damaged={}, pushed={}, xp={}",
                        result.affected,
                        result.damaged,
                        result.pushed,
                        result.xp
                    );
                    self.arcana_starter_visual(
                        event.cast_id,
                        &event.spell_id,
                        position,
                        direction,
                        DVec3::ZERO,
                        0,
                        VisualStage::FirePush,
                    );
                }
                Err(error) => log::warn!("Arcana fire_push rejected: {error}"),
            }
        }
        true
    }
}
impl arcana_core::fire::FireWorld for PilotWorld<'_, '_> {
    fn ignite_ray(&mut self, _: DVec3, _: DVec3, _: f64) -> bool {
        // ArcaneFireBlock attaches to stone, retains ownership and respects place
        // hooks. Vanilla fire cannot substitute for that not-yet-ported block.
        false
    }
}
