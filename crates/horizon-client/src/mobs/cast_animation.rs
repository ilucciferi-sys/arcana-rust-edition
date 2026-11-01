//! Arcana's authored poses layered over native locomotion and equipment.
use super::{
    Mob,
    anim::{Poses, State},
};
use arcana_core::{
    Catalog,
    animation::{self, Library, Movement},
    casting::{CastEvent, Phase},
};
use glam::{EulerRot, Quat, Vec3};
use std::{sync::OnceLock, time::Instant};

#[derive(Clone, Debug)]
pub struct Playback {
    pub event: CastEvent,
    received: Instant,
    stopped: Option<Instant>,
}
impl Playback {
    pub fn new(event: CastEvent) -> Self {
        let stopped = matches!(event.phase, Phase::Cancelled | Phase::Finished).then(Instant::now);
        Self {
            event,
            received: Instant::now(),
            stopped,
        }
    }
    pub fn update(&mut self, event: CastEvent) {
        if event.start_tick < self.event.start_tick
            || event.start_tick == self.event.start_tick
                && (event.cast_id < self.event.cast_id
                    || event.cast_id == self.event.cast_id
                        && (event.server_tick < self.event.server_tick || self.stopped.is_some()))
        {
            return;
        }
        *self = Self::new(event);
    }
    pub fn age(&self) -> f32 {
        self.event.server_tick.saturating_sub(self.event.start_tick) as f32
            + if self.stopped.is_some() {
                0.0
            } else {
                self.received.elapsed().as_secs_f32() * 20.0
            }
    }
    pub fn fade(&self) -> f32 {
        self.stopped.map_or_else(
            || 1.0 - animation::smooth((self.received.elapsed().as_secs_f32() - 0.8) / 0.2),
            |at| 1.0 - animation::smooth(at.elapsed().as_secs_f32() * 5.0),
        )
    }
    pub fn alive(&self) -> bool {
        self.fade() > 0.0
    }
}
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog::bundled().expect("original Arcana catalog"))
}
fn quat(p: &[f32; 9]) -> Quat {
    Quat::from_euler(EulerRot::ZYX, p[5], p[4], p[3])
}
fn set_rotation(p: &mut [f32; 9], q: Quat) {
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    p[3..6].copy_from_slice(&[x, y, z]);
}
pub fn apply(m: &Mob, s: &State, poses: &mut Poses) {
    let Some(playback) = m.extra.cast_animation.as_ref().filter(|p| p.alive()) else {
        return;
    };
    if &*m.kind.id != "minecraft:player"
        || !m.alive()
        || m.extra.passenger
        || matches!(
            m.extra.avatar_pose,
            crate::pose::Pose::Sleeping
                | crate::pose::Pose::Swimming
                | crate::pose::Pose::FallFlying
        )
    {
        return;
    }
    let Some(spell) = catalog().get(&playback.event.spell_id) else {
        return;
    };
    let library = Library::bundled();
    let Some(binding) = library.bindings.get(&spell.id) else {
        return;
    };
    let age = playback.age();
    let phase = animation::phase(spell, age);
    let weight =
        animation::smooth(age / (spell.cast_time as f32 * 0.12).max(2.0)) * playback.fade();
    let lower = if binding.mode == Movement::Mobile {
        if m.on_ground {
            1.0 - (s.speed * 4.0).clamp(0.0, 1.0) * 0.92
        } else {
            0.12
        }
    } else {
        1.0
    };
    let left = m.main_arm_left();
    let sample = |name| library.sample(&spell.id, name, phase, left);
    let (mut torso, _) = sample("torso");
    let pitch = s.x_rot.clamp(-65.0, 65.0);
    let yaw = s.y_rot.clamp(-55.0, 55.0);
    if binding.aim {
        torso.0[0] += pitch * 0.18;
        torso.0[1] += yaw * 0.22;
    }
    if m.hurt_time > 0 {
        torso.0[0] -= m.hurt_time as f32 * 1.3;
        torso.0[2] += (m.hurt_time as f32 * 0.8).sin() * 4.0;
    }
    for (native, original) in [
        ("body", "torso"),
        ("head", "head"),
        ("right_arm", "rightArm"),
        ("left_arm", "leftArm"),
        ("right_leg", "rightLeg"),
        ("left_leg", "leftLeg"),
    ] {
        if !poses.has(native) {
            continue;
        }
        let (mut bone, bend) = sample(original);
        let arm = original.ends_with("Arm");
        let leg = original.ends_with("Leg");
        let blend = weight * if leg { lower } else { 1.0 };
        if binding.aim {
            if arm {
                bone.0[0] -= pitch * 0.45;
                bone.0[1] += yaw * 0.28;
            }
            if native == "head" {
                bone.0[0] += pitch * 0.65;
                bone.0[1] += yaw * 0.65;
            }
        }
        let target = if native == "body" {
            torso.rotation()
        } else if arm {
            torso.rotation() * bone.rotation()
        } else {
            bone.rotation()
        };
        let offset = if arm {
            animation::shoulder(torso, bone, native == "right_arm")
                - Vec3::new(if native == "right_arm" { -5.0 } else { 5.0 }, 2.0, 0.0)
        } else if native == "head" || native == "body" {
            animation::neck(torso)
        } else {
            bone.position()
        };
        let p = poses.get(native);
        let old = quat(p);
        set_rotation(p, old.slerp(if leg { old * target } else { target }, blend));
        for i in 0..3 {
            p[i] += offset[i] * blend;
        }
        if arm || leg {
            let hinge = if arm { 4.0 } else { 6.0 };
            let bend = Bend {
                axis: bend.0[0].to_radians(),
                angle: bend.0[1].to_radians() * blend,
                hinge,
            };
            poses.bends.insert(native.into(), bend);
            // Skin outer layers are children at the same local origin.
            let overlay = native.replace("arm", "sleeve").replace("leg", "pants");
            poses.bends.insert(overlay, bend);
        }
    }
    let root = sample("body").0;
    let root_weight = weight * lower;
    let root_rotation = Quat::IDENTITY.slerp(root.rotation(), root_weight);
    let shift = root.position() * Vec3::new(-1.0, -1.0, 1.0) * root_weight;
    for name in [
        "body",
        "head",
        "right_arm",
        "left_arm",
        "right_leg",
        "left_leg",
    ] {
        if !poses.has(name) {
            continue;
        }
        let p = poses.get(name);
        let position =
            root_rotation * (Vec3::from_slice(&p[..3]) - Vec3::Y * 12.0) + Vec3::Y * 12.0 + shift;
        p[..3].copy_from_slice(&position.to_array());
        set_rotation(p, root_rotation * quat(p));
    }
}

/// Two-bone skinning with a short transition around the elbow/knee.
#[derive(Clone, Copy, Debug)]
pub struct Bend {
    pub axis: f32,
    pub angle: f32,
    pub hinge: f32,
}
impl Bend {
    pub fn transform(self, at: Vec3) -> Vec3 {
        let amount = animation::smooth((at.y - self.hinge + 1.0) * 0.5);
        let axis = Vec3::new(self.axis.cos(), 0.0, self.axis.sin());
        let pivot = Vec3::Y * self.hinge;
        Quat::from_axis_angle(axis, self.angle * amount) * (at - pivot) + pivot
    }
    pub fn hand(self) -> glam::Mat4 {
        let pivot = Vec3::Y * self.hinge / 16.0;
        glam::Mat4::from_translation(pivot)
            * glam::Mat4::from_axis_angle(
                Vec3::new(self.axis.cos(), 0.0, self.axis.sin()),
                self.angle,
            )
            * glam::Mat4::from_translation(-pivot)
    }
}

/// Split textured faces around a joint before skinning. UVs interpolate on the
/// original faces, so player skins and armour use their existing atlas entries.
pub fn deform(part: &super::Part, bend: Bend) -> Vec<super::Polygon> {
    fn clip(input: &[[f32; 5]], y: f32, above: bool) -> Vec<[f32; 5]> {
        let mut out = Vec::new();
        for i in 0..input.len() {
            let (a, b) = (input[i], input[(i + 1) % input.len()]);
            let inside = |v: [f32; 5]| if above { v[1] >= y } else { v[1] <= y };
            if inside(a) {
                out.push(a);
            }
            if inside(a) != inside(b) {
                let q = (y - a[1]) / (b[1] - a[1]);
                out.push(std::array::from_fn(|j| a[j] + (b[j] - a[j]) * q));
            }
        }
        out
    }
    let mut out = Vec::new();
    for face in &part.polygons {
        let edges = [
            f32::NEG_INFINITY,
            bend.hinge - 1.0,
            bend.hinge,
            bend.hinge + 1.0,
            f32::INFINITY,
        ];
        for band in edges.windows(2) {
            let vertices = clip(&clip(&face.vertices, band[0], true), band[1], false);
            if vertices.len() < 3 {
                continue;
            }
            let points: Vec<_> = vertices
                .iter()
                .map(|v| bend.transform(Vec3::from_slice(&v[..3])))
                .collect();
            let source_cross = (Vec3::from_slice(&face.vertices[1][..3])
                - Vec3::from_slice(&face.vertices[0][..3]))
            .cross(
                Vec3::from_slice(&face.vertices[2][..3]) - Vec3::from_slice(&face.vertices[0][..3]),
            );
            let sign = if source_cross.dot(Vec3::from_array(face.normal)) < 0.0 {
                -1.0
            } else {
                1.0
            };
            for i in 1..vertices.len() - 1 {
                let ids = [0, i, i + 1, i + 1];
                let normal = (points[i] - points[0])
                    .cross(points[i + 1] - points[0])
                    .normalize_or_zero()
                    * sign;
                if normal == Vec3::ZERO {
                    continue;
                }
                out.push(super::Polygon {
                    normal: normal.to_array(),
                    vertices: ids.map(|j| {
                        let v = vertices[j];
                        let p = points[j];
                        [p.x, p.y, p.z, v[3], v[4]]
                    }),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn event(tick: u64, phase: Phase) -> CastEvent {
        CastEvent {
            spell_id: "fireball".into(),
            cast_id: 7,
            start_tick: 100,
            server_tick: tick,
            phase,
        }
    }
    #[test]
    fn cancelled_pose_cannot_be_revived_by_reordered_heartbeat() {
        let mut pose = Playback::new(event(120, Phase::Preparation));
        pose.update(event(124, Phase::Cancelled));
        pose.update(event(122, Phase::Preparation));
        assert_eq!(pose.event.phase, Phase::Cancelled);
        let frozen = pose.age();
        pose.stopped = Some(Instant::now() - Duration::from_millis(100));
        assert!((pose.fade() - 0.5).abs() < 0.02);
        assert_eq!(pose.age(), frozen);
        pose.stopped = Some(Instant::now() - Duration::from_millis(210));
        assert!(!pose.alive());
    }
    #[test]
    fn missing_heartbeat_expires_without_stuck_limbs() {
        let mut pose = Playback::new(event(120, Phase::Preparation));
        pose.received = Instant::now() - Duration::from_millis(1010);
        assert!(!pose.alive());
        let mut next = event(130, Phase::Preparation);
        next.cast_id = 8;
        pose.update(next);
        assert!(pose.alive());
    }
    #[test]
    fn reconnect_can_restart_cast_ids_without_reusing_the_old_timeline() {
        let mut pose = Playback::new(event(124, Phase::Cancelled));
        let mut next = event(150, Phase::Preparation);
        next.cast_id = 1;
        next.start_tick = 150;
        pose.update(next);
        assert!(pose.alive());
        assert_eq!(pose.event.phase, Phase::Preparation);
        assert!(pose.age() < 1.0);
        pose.update(event(124, Phase::Cancelled));
        assert_eq!(pose.event.start_tick, 150);
    }
    #[test]
    fn skinned_elbow_keeps_shoulder_fixed_and_hand_attachment_consistent() {
        let bend = Bend {
            axis: 0.0,
            angle: -0.72,
            hinge: 4.0,
        };
        let upper = Vec3::new(1.0, -2.0, 0.0);
        let palm = Vec3::new(1.0, 10.0, 0.0);
        assert_eq!(bend.transform(upper), upper);
        assert!(
            (bend.hand().transform_point3(palm / 16.0) - bend.transform(palm) / 16.0).length()
                < 1e-6
        );
        assert!((bend.transform(palm) - palm).length() > 3.0);
        let part = super::super::Part {
            name: "right_arm".into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            children: vec![],
            polygons: vec![super::super::Polygon {
                normal: [0.0, 0.0, 1.0],
                vertices: [
                    [-2.0, -2.0, 0.0, 0.0, 0.0],
                    [2.0, -2.0, 0.0, 1.0, 0.0],
                    [2.0, 10.0, 0.0, 1.0, 1.0],
                    [-2.0, 10.0, 0.0, 0.0, 1.0],
                ],
            }],
        };
        let faces = deform(&part, bend);
        assert!(faces.len() >= 8);
        assert!(
            faces
                .iter()
                .flat_map(|f| &f.vertices)
                .all(|v| v.iter().all(|x| x.is_finite())
                    && (0.0..=1.0).contains(&v[3])
                    && (0.0..=1.0).contains(&v[4]))
        );
        assert!(
            faces
                .iter()
                .all(|f| (Vec3::from_array(f.normal).length() - 1.0).abs() < 1e-5)
        );
    }
}
