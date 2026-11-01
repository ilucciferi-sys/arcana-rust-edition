//! Original CastProfiles choreography, sampled independently of the renderer.
use crate::Spell;
use glam::{EulerRot, Quat, Vec3};
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::OnceLock,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Movement {
    #[default]
    Mobile,
    Limited,
    Rooted,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Binding {
    pub clip: String,
    pub mode: Movement,
    pub family: String,
    pub aim: bool,
}
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Bone(pub [f32; 6]);
impl Bone {
    pub fn rotation(self) -> Quat {
        rotation(self.0[..3].try_into().unwrap())
    }
    pub fn position(self) -> Vec3 {
        Vec3::from_slice(&self.0[3..])
    }
    fn mirror(mut self) -> Self {
        for i in [1, 2, 3] {
            self.0[i] = -self.0[i];
        }
        self
    }
}
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct Bend(pub [f32; 2]);
#[derive(Debug, Deserialize)]
struct Key {
    time: f32,
    bones: HashMap<String, Bone>,
    #[serde(default)]
    bends: HashMap<String, Bend>,
}
#[derive(Debug, Deserialize)]
struct Clip {
    frames: Vec<Key>,
    markers: HashMap<String, f32>,
}
#[derive(Debug, Deserialize)]
pub struct Library {
    schema: u32,
    pub bindings: HashMap<String, Binding>,
    clips: HashMap<String, Clip>,
}
impl Library {
    pub fn bundled() -> &'static Self {
        static LIBRARY: OnceLock<Library> = OnceLock::new();
        LIBRARY.get_or_init(|| {
            Self::parse(include_str!("../../../assets/data/cast_animations.json"))
                .expect("validated original Arcana choreography")
        })
    }
    pub fn parse(json: &str) -> Result<Self, String> {
        let library: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if library.schema != 1 {
            return Err("unknown choreography schema".into());
        }
        for binding in library.bindings.values() {
            let clip = library.clips.get(&binding.clip).ok_or("missing clip")?;
            if clip.frames.is_empty()
                || clip.markers.get("SPELL_RELEASE") != Some(&1.0)
                || clip.frames.windows(2).any(|w| w[0].time >= w[1].time)
                || clip.frames.iter().any(|k| {
                    !k.time.is_finite()
                        || !(0.0..=2.0).contains(&k.time)
                        || k.bones.values().any(|b| b.0.iter().any(|n| !n.is_finite()))
                        || k.bends.values().any(|b| b.0.iter().any(|n| !n.is_finite()))
                })
            {
                return Err(format!("invalid choreography: {}", binding.clip));
            }
        }
        Ok(library)
    }
    pub fn sample(&self, spell: &str, part: &str, time: f32, left: bool) -> (Bone, Bend) {
        let Some(clip) = self
            .bindings
            .get(spell)
            .and_then(|b| self.clips.get(&b.clip))
        else {
            return (Bone::default(), Bend::default());
        };
        let swapped;
        let source = if left && (part.starts_with("right") || part.starts_with("left")) {
            swapped = if let Some(suffix) = part.strip_prefix("right") {
                format!("left{suffix}")
            } else {
                format!("right{}", &part[4..])
            };
            &swapped
        } else {
            part
        };
        let mut first = &clip.frames[0];
        let mut second = first;
        for key in &clip.frames {
            second = key;
            if key.time >= time {
                break;
            }
            first = key;
        }
        let q = if first.time == second.time {
            0.0
        } else {
            smooth((time - first.time) / (second.time - first.time))
        };
        let a = first.bones.get(source).copied().unwrap_or_default();
        let b = second.bones.get(source).copied().unwrap_or_default();
        let bone = Bone(std::array::from_fn(|i| a.0[i] + (b.0[i] - a.0[i]) * q));
        let a = first.bends.get(source).copied().unwrap_or_default();
        let b = second.bends.get(source).copied().unwrap_or_default();
        let bend = Bend(std::array::from_fn(|i| a.0[i] + (b.0[i] - a.0[i]) * q));
        (if left { bone.mirror() } else { bone }, bend)
    }
}
pub fn smooth(q: f32) -> f32 {
    let q = q.clamp(0.0, 1.0);
    q * q * (3.0 - 2.0 * q)
}
pub fn phase(spell: &Spell, age: f32) -> f32 {
    if age <= spell.cast_time as f32 {
        (age / spell.cast_time as f32).max(0.0)
    } else {
        1.0 + ((age - (spell.cast_time + spell.channel_ticks()) as f32)
            / spell.recovery_ticks().max(1) as f32)
            .clamp(0.0, 1.0)
    }
}
pub fn rotation(degrees: [f32; 3]) -> Quat {
    let [x, y, z] = degrees.map(f32::to_radians);
    Quat::from_euler(EulerRot::ZYX, z, y, x)
}
/// Waist hierarchy over the vanilla sibling parts, in model pixels.
pub fn shoulder(torso: Bone, arm: Bone, right: bool) -> Vec3 {
    torso.rotation() * (Vec3::new(if right { -5.0 } else { 5.0 }, -10.0, 0.0) + arm.position())
        + Vec3::Y * 12.0
}
pub fn neck(torso: Bone) -> Vec3 {
    torso.rotation() * (Vec3::NEG_Y * 12.0) + Vec3::Y * 12.0
}

/// Remember terminal casts after their visible fade has been removed. The
/// transport's 256-event reorder window bounds how many tombstones are needed.
#[derive(Default)]
pub struct CastHistory(VecDeque<([i32; 4], u64, u64)>);
impl CastHistory {
    pub fn accept(&mut self, owner: [i32; 4], event: &crate::casting::CastEvent) -> bool {
        let key = (owner, event.cast_id, event.start_tick);
        if matches!(
            event.phase,
            crate::casting::Phase::Cancelled | crate::casting::Phase::Finished
        ) {
            if !self.0.contains(&key) {
                self.0.push_back(key);
                if self.0.len() > 256 {
                    self.0.pop_front();
                }
            }
            true
        } else {
            !self.0.contains(&key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Catalog;
    #[test]
    fn terminal_cast_is_remembered_after_the_render_pose_is_removed() {
        use crate::casting::{CastEvent, Phase};
        let mut history = CastHistory::default();
        let mut event = CastEvent {
            spell_id: "fireball".into(),
            cast_id: 1,
            start_tick: 10,
            server_tick: 12,
            phase: Phase::Preparation,
        };
        assert!(history.accept([1, 2, 3, 4], &event));
        event.phase = Phase::Cancelled;
        assert!(history.accept([1, 2, 3, 4], &event));
        event.phase = Phase::Preparation;
        assert!(!history.accept([1, 2, 3, 4], &event));
        assert!(history.accept([1, 2, 3, 5], &event));
        event.cast_id = 2;
        assert!(history.accept([1, 2, 3, 4], &event));
        // A reconnect creates a fresh caster; its local IDs may restart.
        event.cast_id = 1;
        event.start_tick = 20;
        event.server_tick = 20;
        assert!(history.accept([1, 2, 3, 4], &event));
    }
    #[test]
    fn all_spells_have_finite_original_choreography() {
        let library = Library::bundled();
        assert_eq!(library.bindings.len(), 85);
        for spell in Catalog::bundled().unwrap().spells() {
            assert!(library.bindings.contains_key(&spell.id), "{}", spell.id);
            for i in 0..=200 {
                for part in [
                    "body", "torso", "head", "rightArm", "leftArm", "rightLeg", "leftLeg",
                ] {
                    let (bone, bend) = library.sample(&spell.id, part, i as f32 * 0.01, false);
                    assert!(bone.0.iter().chain(&bend.0).all(|v| v.is_finite()));
                }
            }
        }
    }
    #[test]
    fn release_and_channel_never_drift() {
        let catalog = Catalog::bundled().unwrap();
        let beam = catalog.get("inferno_beam").unwrap();
        assert_eq!(phase(beam, beam.cast_time as f32), 1.0);
        assert_eq!(
            phase(beam, (beam.cast_time + beam.channel_ticks() / 2) as f32),
            1.0
        );
        assert_eq!(
            phase(
                beam,
                (beam.cast_time + beam.channel_ticks() + beam.recovery_ticks()) as f32
            ),
            2.0
        );
    }
    #[test]
    fn left_hand_swaps_limbs_and_mirrors_rotation_and_offset() {
        let library = Library::bundled();
        let (right, bend) = library.sample("fireball", "rightArm", 0.82, false);
        let (left, mirrored_bend) = library.sample("fireball", "leftArm", 0.82, true);
        assert_eq!(left.0, right.mirror().0);
        assert_eq!(bend.0, mirrored_bend.0);
        assert_ne!(
            right.0,
            library.sample("fireball", "leftArm", 0.82, false).0.0
        );
    }
    #[test]
    fn malformed_keys_and_release_marker_are_rejected() {
        let source = include_str!("../../../assets/data/cast_animations.json");
        for malformed in [
            source.replacen("\"time\": 0.18", "\"time\": 0", 1),
            source.replacen("\"SPELL_RELEASE\": 1", "\"SPELL_RELEASE\": 0.9", 1),
        ] {
            assert!(Library::parse(&malformed).is_err());
        }
    }
    #[test]
    fn waist_rotation_moves_shoulders_and_neck_together() {
        let torso = Bone([30.0, 20.0, 0.0, 0.0, 0.0, 0.0]);
        let right = shoulder(torso, Bone::default(), true);
        let left = shoulder(torso, Bone::default(), false);
        assert!(((right - left).length() - 10.0).abs() < 1e-5);
        assert!((neck(torso) - Vec3::Y * 12.0).length() > 11.99);
        assert_eq!(
            shoulder(Bone::default(), Bone::default(), true),
            Vec3::new(-5.0, 2.0, 0.0)
        );
    }
}
