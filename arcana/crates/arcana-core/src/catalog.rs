use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum School {
    Fire,
    Water,
    Earth,
    Air,
    Necromancy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spell {
    pub id: String,
    pub school: School,
    pub level: u8,
    pub mana: f64,
    pub cooldown: u32,
    pub cast_time: u32,
    pub range: f64,
    #[serde(rename = "type")]
    pub kind: String,
    pub animation: String,
    pub mechanic: String,
    pub power: f64,
    pub radius: f64,
    pub duration: u32,
    pub speed: f64,
    pub name: String,
    pub description: String,
}

impl Spell {
    pub fn channel_ticks(&self) -> u32 {
        if self.kind == "CHANNEL" {
            self.duration
        } else {
            0
        }
    }

    pub fn recovery_ticks(&self) -> u32 {
        match self.id.as_str() {
            "spark" | "water_bolt" | "stone_bolt" | "wind_gust" => 6,
            _ if self.level == 5 => 16,
            _ => 8,
        }
    }

    pub fn executable(&self) -> bool {
        self.id == "solar_spear" && self.mechanic == "SUN_SPEAR"
            || self.id == "ignition" && self.mechanic == "IGNITE"
            || self.id == "fire_push" && self.mechanic == "PUSH"
            || ["spark", "water_bolt", "stone_bolt", "wind_gust"].contains(&self.id.as_str())
                && self.mechanic == "STARTER"
    }
}

#[derive(Debug)]
pub struct Catalog(Vec<Spell>);

impl Catalog {
    pub fn bundled() -> Result<Self, String> {
        Self::parse(include_str!("../../../assets/data/spells.json"))
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        let spells: Vec<Spell> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut ids = BTreeSet::new();
        for s in &spells {
            if s.id.is_empty()
                || !s
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || !ids.insert(s.id.as_str())
                || !(1..=5).contains(&s.level)
                || ![s.mana, s.range, s.power, s.radius, s.speed]
                    .iter()
                    .all(|n| n.is_finite() && *n >= 0.0)
                || (s.mana == 0.0 && s.mechanic != "DASH")
                || s.cast_time == 0
                || s.cooldown == 0
                || s.duration == 0
                || ![
                    "PROJECTILE",
                    "AREA",
                    "BUFF",
                    "MOVEMENT",
                    "CHANNEL",
                    "ULTIMATE",
                    "SUMMON",
                ]
                .contains(&s.kind.as_str())
            {
                return Err(format!("invalid spell: {}", s.id));
            }
        }
        if spells.len() != 76 {
            return Err("expected all 76 original definitions".into());
        }
        Ok(Self(spells))
    }

    pub fn spells(&self) -> &[Spell] {
        &self.0
    }
    pub fn get(&self, id: &str) -> Option<&Spell> {
        self.0.iter().find(|s| s.id == id)
    }
}
