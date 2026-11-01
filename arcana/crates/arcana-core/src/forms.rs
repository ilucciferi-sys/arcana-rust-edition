//! Fire form presentation, independent of native models and transport.
use glam::{DVec3, EulerRot, Quat};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Avatar,
    Phoenix,
}
impl Kind {
    pub fn from_spell(id: &str, stage: u8) -> Option<Self> {
        match (id, stage) {
            ("fire_avatar", 0) => Some(Self::Avatar),
            ("phoenix_form", 0 | 1) => Some(Self::Phoenix),
            _ => None,
        }
    }
}
pub fn opacity(age: f32, life: u32) -> f32 {
    (age / 10.0).min((life as f32 - age) / 20.0).clamp(0.0, 1.0)
}
pub fn texture_frame(age: f32) -> usize {
    ((age.max(0.0) / 3.0) as usize) % 8
}

pub struct Flight {
    pub velocity: DVec3,
    pub age: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub body_yaw: f32,
    pub dive: Option<DVec3>,
}
pub struct Pose {
    pub root: Quat,
    pub head: [f32; 3],
    /// Right, left. Degrees, as in the original CastingAnimation.fireFlight.
    pub arms: [[f32; 3]; 2],
    pub legs: [[f32; 3]; 2],
    pub elbow: f32,
    pub knee: f32,
}
impl Flight {
    pub fn pose(&self) -> Pose {
        if let Some(direction) = self.dive.and_then(DVec3::try_normalize) {
            let lean = direction.y.clamp(-1.0, 1.0).acos() as f32;
            let yaw = if direction.x * direction.x + direction.z * direction.z > 1e-8 {
                (-direction.x).atan2(direction.z) as f32 - self.body_yaw.to_radians()
            } else {
                0.0
            };
            return Pose {
                root: Quat::from_euler(EulerRot::ZYX, 0.0, yaw, lean),
                head: [-90.0, 0.0, 0.0],
                arms: [[-8.0, -3.0, 12.0], [-8.0, 3.0, -12.0]],
                legs: [[4.0, 0.0, 2.0], [4.0, 0.0, -2.0]],
                elbow: 8.0,
                knee: 4.0,
            };
        }
        let speed = (self.velocity.x.hypot(self.velocity.z) as f32 / 0.8).clamp(0.0, 1.0);
        let lean = 8.0 + 26.0 * crate::animation::smooth(speed);
        let wave = (self.age * 0.075).sin();
        let spread = 28.0 - speed * 8.0 + wave * 1.5;
        Pose {
            root: Quat::from_rotation_x(lean.to_radians()),
            head: [
                -lean * 0.8 + self.pitch.clamp(-40.0, 40.0) * 0.4,
                self.yaw.clamp(-45.0, 45.0) * 0.65,
                0.0,
            ],
            arms: [
                [-12.0 + speed * 24.0, -7.0, spread],
                [-12.0 + speed * 24.0, 7.0, -spread],
            ],
            legs: [[4.0 + speed * 5.0, 0.0, 3.0], [speed * 5.0, 0.0, -3.0]],
            elbow: 10.0 + speed * 4.0,
            knee: 5.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_live_form_stages_replace_the_body() {
        assert_eq!(Kind::from_spell("phoenix_form", 0), Some(Kind::Phoenix));
        assert_eq!(Kind::from_spell("phoenix_form", 1), Some(Kind::Phoenix));
        assert_eq!(Kind::from_spell("phoenix_form", 2), None);
        assert_eq!(Kind::from_spell("flame_shield", 0), None);
        assert_eq!(Kind::from_spell("fire_avatar", 0), Some(Kind::Avatar));
    }
    #[test]
    fn material_fades_and_cycles_all_original_frames() {
        assert_eq!(opacity(0.0, 160), 0.0);
        assert_eq!(opacity(5.0, 160), 0.5);
        assert_eq!(opacity(20.0, 160), 1.0);
        assert_eq!(opacity(150.0, 160), 0.5);
        assert_eq!(opacity(160.0, 160), 0.0);
        assert_eq!(
            (0..8)
                .map(|i| texture_frame(i as f32 * 3.0))
                .collect::<Vec<_>>(),
            (0..8).collect::<Vec<_>>()
        );
        assert_eq!(texture_frame(24.0), 0);
    }
    #[test]
    fn hover_and_powered_flight_keep_a_straight_spine() {
        let mut flight = Flight {
            velocity: DVec3::ZERO,
            age: 0.0,
            pitch: 0.0,
            yaw: 0.0,
            body_yaw: 0.0,
            dive: None,
        };
        let hover = flight.pose();
        assert!((hover.root.to_euler(EulerRot::XYZ).0.to_degrees() - 8.0).abs() < 1e-4);
        assert_eq!(hover.arms[0], [-12.0, -7.0, 28.0]);
        flight.velocity = DVec3::X * 0.8;
        let fast = flight.pose();
        assert!((fast.root.to_euler(EulerRot::XYZ).0.to_degrees() - 34.0).abs() < 1e-4);
        assert_eq!(fast.elbow, 14.0);
        assert_eq!(fast.arms[0], [12.0, -7.0, 20.0]);
    }
    #[test]
    fn vertical_dive_has_a_finite_aligned_spine_and_gaze() {
        use glam::Vec3;
        for direction in [
            DVec3::NEG_Y,
            DVec3::Y,
            DVec3::Z,
            DVec3::new(0.4, -0.8, 0.2).normalize(),
        ] {
            let pose = Flight {
                velocity: direction * 4.0,
                age: 22.0,
                pitch: 55.0,
                yaw: 40.0,
                body_yaw: 0.0,
                dive: Some(direction),
            }
            .pose();
            assert!(pose.root.is_finite());
            // Native player base at yaw 0: Ry(180) * Scale(-1,-1,1).
            let to_world = |v: Vec3| {
                Quat::from_rotation_y(std::f32::consts::PI) * (v * Vec3::new(-1.0, -1.0, 1.0))
            };
            let spine = to_world(pose.root * Vec3::NEG_Y);
            let gaze = to_world(pose.root * crate::animation::rotation(pose.head) * Vec3::NEG_Z);
            assert!((spine - direction.as_vec3()).length() < 1e-5);
            assert!((gaze - spine).length() < 1e-5);
        }
    }
}
