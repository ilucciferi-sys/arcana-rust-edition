//! CastProfiles movement modes: mobile, 38% speed, or fixed translation.
use super::*;

pub(in crate::game) use arcana_core::animation::Movement as Mode;
pub(super) fn cast_mode(cast: Option<&CastEvent>, tick: u64) -> Mode {
    let profiles = arcana_core::animation::Library::bundled();
    let Some(cast) = cast else {
        return Mode::Mobile;
    };
    let Some(spell) = catalog().get(&cast.spell_id) else {
        return Mode::Mobile;
    };
    if tick.saturating_sub(cast.start_tick)
        >= u64::from(spell.cast_time + spell.channel_ticks() + 3)
    {
        return Mode::Mobile;
    }
    profiles
        .bindings
        .get(&cast.spell_id)
        .map_or(Mode::Mobile, |p| p.mode)
}
pub(in crate::game) fn apply(player: &mut horizon_client::motion::PlayerMotion, mode: Mode) {
    if let Err(e) = player.set_modifier(
        "minecraft:movement_speed",
        "arcana:limited_cast",
        -0.62,
        "add_multiplied_total",
        mode == Mode::Limited,
    ) {
        log::warn!("Arcana cast movement: {e}");
    }
}
impl Game<'_> {
    pub(super) fn arcana_update_cast_motion(&mut self, actor: &mut Actor, tick: u64) {
        let mode = if self.arcana_available() {
            cast_mode(actor.caster.status(tick).as_ref(), tick)
        } else {
            Mode::Mobile
        };
        if let Some(p) = self.player.as_mut() {
            if mode != actor.movement {
                apply(p, mode);
            }
            if mode == Mode::Rooted {
                p.velocity = Vec3::ZERO;
            }
        }
        actor.movement = mode;
    }
    pub(in crate::game) fn arcana_rooted(&self, uuid: u128) -> bool {
        let tick = self.world.game_time.max(0) as u64;
        self.arcana
            .actors
            .get(&uuid)
            .is_some_and(|a| cast_mode(a.caster.status(tick).as_ref(), tick) == Mode::Rooted)
    }
    pub(in crate::game) fn arcana_client_cast_rooted(&mut self) -> bool {
        let mode = self.arcana.snapshot.as_ref().map_or(Mode::Mobile, |s| {
            let tick = s.server_tick
                + (self.arcana.snapshot_received.elapsed().as_secs_f64() * 20.0) as u64;
            cast_mode(s.cast.as_ref(), tick)
        });
        if mode != self.arcana.school.movement {
            if let Some(p) = self.player.as_mut() {
                apply(p, mode);
            }
            self.arcana.school.movement = mode;
        }
        mode == Mode::Rooted
    }
}
