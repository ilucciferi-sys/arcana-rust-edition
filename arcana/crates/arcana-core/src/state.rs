use crate::{Catalog, School};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const XP_THRESHOLDS: [u32; 5] = [0, 100, 350, 800, 1600];
pub fn level_for_xp(xp: u32) -> u8 {
    XP_THRESHOLDS
        .iter()
        .filter(|threshold| xp >= **threshold)
        .count() as u8
}

/// A Rust save, not a Java NBT converter. Casts and presentation clocks are transient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MagicState {
    schema: u32,
    school: Option<School>,
    xp: u32,
    mana: f64,
    selected: Option<String>,
    equipped: BTreeSet<String>,
    cooldowns: BTreeMap<String, u32>,
    resurrection_charges: u8,
    resurrection_tier: u8,
    owned_undead: BTreeSet<String>,
}

impl Default for MagicState {
    fn default() -> Self {
        Self {
            schema: 1,
            school: None,
            xp: 0,
            mana: 0.0,
            selected: None,
            equipped: BTreeSet::new(),
            cooldowns: BTreeMap::new(),
            resurrection_charges: 0,
            resurrection_tier: 0,
            owned_undead: BTreeSet::new(),
        }
    }
}

impl MagicState {
    pub fn school(&self) -> Option<School> {
        self.school
    }
    pub fn level(&self) -> u8 {
        level_for_xp(self.xp)
    }
    pub fn xp(&self) -> u32 {
        self.xp
    }
    pub fn mana(&self) -> f64 {
        self.mana
    }
    pub fn max_mana(&self) -> f64 {
        100.0 + f64::from(self.level() - 1) * 15.0
    }
    pub fn equipped(&self, id: &str) -> bool {
        self.equipped.contains(id)
    }
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }
    pub fn cooldown(&self, id: &str) -> u32 {
        self.cooldowns.get(id).copied().unwrap_or(0)
    }
    pub fn army_limit(&self) -> usize {
        if self.school == Some(School::Necromancy) {
            [1, 3, 6, 9, 12][usize::from(self.level() - 1)]
        } else {
            0
        }
    }
    pub fn resurrection_charges(&self) -> u8 {
        self.resurrection_charges
    }
    pub fn spend_resurrection(&mut self) -> bool {
        if self.school != Some(School::Necromancy) || self.resurrection_charges == 0 {
            return false;
        }
        self.resurrection_charges -= 1;
        true
    }

    pub fn awaken(&mut self, school: School, catalog: &Catalog) -> bool {
        if self.school.is_some() {
            return false;
        }
        self.school = Some(school);
        self.refresh(catalog);
        self.fill_mana();
        true
    }
    pub fn add_xp(&mut self, amount: u32, catalog: &Catalog) {
        self.xp = self.xp.saturating_add(amount).min(i32::MAX as u32);
        self.refresh(catalog);
    }
    pub fn set_level(&mut self, level: u8, catalog: &Catalog) {
        self.xp = XP_THRESHOLDS[usize::from(level.clamp(1, 5) - 1)];
        self.refresh(catalog);
    }
    fn refresh(&mut self, catalog: &Catalog) {
        self.equipped = catalog
            .spells()
            .iter()
            .filter(|s| Some(s.school) == self.school && s.level <= self.level())
            .map(|s| s.id.clone())
            .collect();
        if !self.selected.as_ref().is_some_and(|s| self.equipped(s)) {
            self.selected = catalog
                .spells()
                .iter()
                .find(|s| Some(s.school) == self.school)
                .map(|s| s.id.clone());
        }
        if self.school == Some(School::Necromancy) {
            let entitlement = if self.level() == 5 { 2 } else { 1 };
            if entitlement > self.resurrection_tier {
                self.resurrection_charges += entitlement - self.resurrection_tier;
                self.resurrection_tier = entitlement;
            }
        }
        self.mana = self.mana.clamp(0.0, self.max_mana());
    }
    pub fn select(&mut self, id: &str) -> bool {
        if !self.equipped(id) {
            return false;
        }
        self.selected = Some(id.into());
        true
    }
    pub fn fill_mana(&mut self) {
        self.mana = self.max_mana();
    }
    pub fn spend_mana(&mut self, amount: f64) -> bool {
        if !amount.is_finite() || amount < 0.0 || self.mana + 1e-6 < amount {
            return false;
        }
        self.mana = (self.mana - amount).max(0.0);
        true
    }
    /// Server-only passive Fire dash. It has no casting reservation or mana cost.
    pub fn passive_fire_dash(&mut self, idle: bool, available: bool) -> bool {
        if !idle
            || !available
            || self.school != Some(School::Fire)
            || !self.equipped("fire_dash")
            || self.cooldown("fire_dash") > 0
        {
            return false;
        }
        self.reserve("fire_dash", 0.0, 60);
        true
    }
    pub fn restore_mana(&mut self, amount: f64) {
        if amount.is_finite() {
            self.mana = (self.mana + amount).clamp(0.0, self.max_mana());
        }
    }
    pub(crate) fn reserve(&mut self, id: &str, mana: f64, cooldown: u32) {
        self.mana = (self.mana - mana).max(0.0);
        self.cooldowns.insert(id.into(), cooldown);
    }
    pub fn clear_cooldowns(&mut self) {
        self.cooldowns.clear();
    }
    /// Call once for each authoritative 20 Hz tick, never for a rendered frame.
    pub fn tick(&mut self, casting: bool) {
        self.cooldowns.retain(|_, ticks| {
            *ticks = ticks.saturating_sub(1);
            *ticks > 0
        });
        if !casting && self.school.is_some() {
            self.restore_mana(2.0 / 20.0);
        }
    }
    pub fn save_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn load_json(json: &str, catalog: &Catalog) -> Result<Self, String> {
        let mut state: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if state.schema != 1
            || !state.mana.is_finite()
            || state.owned_undead.len() > 12
            || state.resurrection_tier > 2
            || state.resurrection_charges > state.resurrection_tier
            || state.cooldowns.len() > 76
        {
            return Err("invalid save".into());
        }
        state.xp = state.xp.min(i32::MAX as u32);
        state
            .cooldowns
            .retain(|id, ticks| catalog.get(id).is_some() && *ticks <= 72000);
        if state.school.is_none() {
            state.mana = 0.0;
            state.cooldowns.clear();
        }
        state.refresh(catalog);
        Ok(state)
    }
}
