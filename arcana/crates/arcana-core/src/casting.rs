use crate::{Catalog, Spell, state::MagicState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    UnknownSpell,
    UnsupportedMechanic,
    Unavailable,
    WrongSchool,
    Locked,
    Busy,
    Cooldown,
    NoMana,
    InvalidDirection,
    RateLimited,
    ClockWentBackwards,
}

/// These facts must be obtained from the server's player, not the action packet.
#[derive(Debug, Clone, Copy)]
pub struct CastPermission {
    pub available: bool,
    pub ritual_active: bool,
    pub direction_valid: bool,
}
impl Default for CastPermission {
    fn default() -> Self {
        Self {
            available: true,
            ritual_active: false,
            direction_valid: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Preparation,
    Release,
    Channeling,
    Recovery,
    Finished,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastEvent {
    pub spell_id: String,
    pub cast_id: u64,
    pub start_tick: u64,
    pub server_tick: u64,
    pub phase: Phase,
}

#[derive(Debug)]
struct ActiveCast {
    spell: Spell,
    id: u64,
    start: u64,
    last_tick: u64,
    released: bool,
}
#[derive(Debug, Default)]
pub struct Caster {
    active: Option<ActiveCast>,
    next_id: u64,
    last_request: Option<u64>,
}

impl Caster {
    pub fn idle(&self) -> bool {
        self.active.is_none()
    }
    pub fn request(
        &mut self,
        state: &mut MagicState,
        catalog: &Catalog,
        id: &str,
        tick: u64,
        permission: CastPermission,
    ) -> Result<CastEvent, Rejection> {
        if self.last_request.is_some_and(|last| tick < last) {
            return Err(Rejection::ClockWentBackwards);
        }
        if self.last_request.is_some_and(|last| tick - last < 4) {
            return Err(Rejection::RateLimited);
        }
        self.last_request = Some(tick);
        let spell = catalog.get(id).ok_or(Rejection::UnknownSpell)?;
        if !permission.available {
            return Err(Rejection::Unavailable);
        }
        if state.school() != Some(spell.school) {
            return Err(Rejection::WrongSchool);
        }
        if state.level() < spell.level || !state.equipped(id) {
            return Err(Rejection::Locked);
        }
        if self.active.is_some() || permission.ritual_active {
            return Err(Rejection::Busy);
        }
        if state.cooldown(id) > 0 {
            return Err(Rejection::Cooldown);
        }
        if state.mana() + 1e-6 < spell.mana {
            return Err(Rejection::NoMana);
        }
        if !permission.direction_valid {
            return Err(Rejection::InvalidDirection);
        }
        if !spell.executable() {
            return Err(Rejection::UnsupportedMechanic);
        }
        state.reserve(id, spell.mana, spell.cooldown);
        self.next_id += 1;
        self.active = Some(ActiveCast {
            spell: spell.clone(),
            id: self.next_id,
            start: tick,
            last_tick: tick,
            released: false,
        });
        Ok(self.event(tick, Phase::Preparation).unwrap())
    }
    fn event(&self, tick: u64, phase: Phase) -> Option<CastEvent> {
        self.active.as_ref().map(|a| CastEvent {
            spell_id: a.spell.id.clone(),
            cast_id: a.id,
            start_tick: a.start,
            server_tick: tick,
            phase,
        })
    }
    /// Crossing a release boundary emits exactly once, including after a skipped tick.
    pub fn advance(&mut self, tick: u64) -> Result<Vec<CastEvent>, Rejection> {
        let Some(a) = &mut self.active else {
            return Ok(Vec::new());
        };
        if tick < a.last_tick {
            return Err(Rejection::ClockWentBackwards);
        }
        if tick == a.last_tick {
            return Ok(Vec::new());
        }
        a.last_tick = tick;
        let age = tick - a.start;
        let end = u64::from(a.spell.cast_time + a.spell.channel_ticks() + a.spell.recovery_ticks());
        let mut phases = Vec::new();
        if !a.released && age >= u64::from(a.spell.cast_time) {
            a.released = true;
            phases.push(Phase::Release);
        }
        if age >= end {
            phases.push(Phase::Finished);
        }
        let events = phases
            .into_iter()
            .filter_map(|p| self.event(tick, p))
            .collect();
        if age >= end {
            self.active = None;
        }
        Ok(events)
    }
    /// Before release: refund mana and retain cooldown. After a non-channel
    /// release: finish recovery. A forced lifecycle cancellation always cleans up.
    pub fn cancel(&mut self, state: &mut MagicState, tick: u64, forced: bool) -> Option<CastEvent> {
        let a = self.active.as_ref()?;
        if tick < a.last_tick {
            return None;
        }
        let before_release = !a.released && tick - a.start < u64::from(a.spell.cast_time);
        if !forced && !before_release && a.spell.channel_ticks() == 0 {
            return None;
        }
        if before_release {
            state.restore_mana(a.spell.mana);
        }
        let event = self.event(tick, Phase::Cancelled);
        self.active = None;
        event
    }
}
