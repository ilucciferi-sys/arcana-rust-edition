//! Transport-independent server events and owner-only magic snapshots.
use crate::{casting::CastEvent, solar::SolarOutcome};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};

pub const PROTOCOL: u32 = 1;
pub const MAX_PACKET: usize = 16 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum Action {
    Cast { spell_id: String },
    Select { spell_id: String },
    Cancel,
    Sync,
    Dash,
    Primary,
    Secondary,
    Extinguish { at: [i32; 3] },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct FireStatus {
    pub shield: bool,
    pub avatar: bool,
    pub phoenix: bool,
    pub flight: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MagicSnapshot {
    pub protocol: u32,
    pub server_tick: u64,
    pub state: crate::state::MagicState,
    pub cast: Option<CastEvent>,
    pub notice: Option<String>,
    #[serde(default)]
    pub ritual: Option<crate::ritual::Activation>,
    #[serde(default)]
    pub fire: FireStatus,
}
impl MagicSnapshot {
    pub fn decode(bytes: &[u8], catalog: &crate::Catalog) -> Result<Self, String> {
        if bytes.len() > MAX_PACKET {
            return Err("snapshot too large".into());
        }
        let mut value: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if value.protocol != PROTOCOL
            || value.notice.as_ref().is_some_and(|n| n.len() > 256)
            || value.ritual.as_ref().is_some_and(|r| !r.valid())
        {
            return Err("invalid snapshot".into());
        }
        value.state = crate::state::MagicState::load_json(
            &value.state.save_json().map_err(|e| e.to_string())?,
            catalog,
        )?;
        Ok(value)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerEvent {
    pub protocol: u32,
    pub session: u64,
    pub event_id: u64,
    pub cast: CastEvent,
    pub solar: Option<SolarOutcome>,
}
impl ServerEvent {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PACKET {
            return Err("packet too large".into());
        }
        let e: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if e.protocol != PROTOCOL
            || e.cast.spell_id.len() > 128
            || e.cast.server_tick < e.cast.start_tick
            || e.solar
                .as_ref()
                .is_some_and(|s| !s.start.is_finite() || !s.end.is_finite())
        {
            return Err("invalid server event".into());
        }
        Ok(e)
    }
}

/// Bounded deduplication accepts reordered events without replaying impact flashes.
#[derive(Debug)]
pub struct Replica {
    session: u64,
    newest: u64,
    seen: BTreeSet<u64>,
    order: VecDeque<u64>,
}
impl Replica {
    pub fn new(session: u64) -> Self {
        Self {
            session,
            newest: 0,
            seen: BTreeSet::new(),
            order: VecDeque::new(),
        }
    }
    pub fn accept(&mut self, event: &ServerEvent) -> bool {
        if event.protocol != PROTOCOL
            || event.session != self.session
            || self.newest.saturating_sub(event.event_id) >= 256
            || !self.seen.insert(event.event_id)
        {
            return false;
        }
        self.newest = self.newest.max(event.event_id);
        self.order.push_back(event.event_id);
        while self.order.len() > 256 {
            if let Some(id) = self.order.pop_front() {
                self.seen.remove(&id);
            }
        }
        true
    }
    pub fn age_ticks(event: &ServerEvent, estimated_server_tick: f64) -> f64 {
        (estimated_server_tick - event.cast.start_tick as f64).max(0.0)
    }
}
