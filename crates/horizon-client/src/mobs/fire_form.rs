//! Native equivalent of ElementBodyLayer and CastingAnimation.fireFlight.
use super::{
    Mob, Part,
    anim::{Poses, State},
    cast_animation::{self, Bend},
    render::{self, Atlas},
};
use arcana_core::{
    DVec3,
    forms::{self, Kind},
};
use glam::{Mat4, Vec3};
use std::{sync::OnceLock, time::Instant};

#[derive(Clone, Debug)]
pub struct View {
    pub kind: Kind,
    pub age: u32,
    pub life: u32,
    pub born: u64,
    pub stage: u8,
    pub flying: bool,
    pub direction: DVec3,
    pub received: Instant,
}
impl View {
    pub fn age(&self) -> f32 {
        self.age as f32 + self.received.elapsed().as_secs_f32() * 20.0
    }
    pub fn alive(&self) -> bool {
        self.received.elapsed().as_secs_f32() < 1.0
            && self.age() < self.life as f32
            && self.stage < 2
    }
    pub fn opacity(&self) -> f32 {
        forms::opacity(self.age(), self.life)
    }
    pub fn texture(&self) -> String {
        format!(
            "arcana_four_elements:entity/body_fire_{}",
            forms::texture_frame(self.age())
        )
    }
    pub fn colors(&self) -> [u32; 2] {
        let a = self.opacity();
        let pulse = if std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some() {
            80.0
        } else {
            200.0 + 40.0 * (self.age() * 0.13).sin()
        };
        [
            ((a * 255.0) as u32) << 24 | 0x00FF_FFFF,
            ((a * pulse) as u32) << 24 | 0x00FF_FFFF,
        ]
    }
}
pub fn active(m: &Mob) -> Option<&View> {
    (&*m.kind.id == "minecraft:player" && m.alive()).then_some(())?;
    m.extra.fire_form.as_ref().filter(|f| f.alive())
}
pub fn apply(m: &Mob, s: &State, p: &mut Poses) {
    let Some(form) = active(m).filter(|f| f.flying) else {
        return;
    };
    if m.extra.passenger
        || m.extra.cast_animation.as_ref().is_some_and(|c| c.alive())
        || matches!(
            m.extra.avatar_pose,
            crate::pose::Pose::Sleeping
                | crate::pose::Pose::Swimming
                | crate::pose::Pose::FallFlying
        )
    {
        return;
    }
    let body_yaw =
        m.previous_body_yaw + super::living::wrap(m.body_yaw - m.previous_body_yaw) * s.partial;
    let pose = forms::Flight {
        velocity: DVec3::new(
            m.position.x - m.previous.x,
            m.position.y - m.previous.y,
            m.position.z - m.previous.z,
        ),
        age: form.age(),
        pitch: s.x_rot,
        yaw: s.y_rot,
        body_yaw,
        dive: (form.kind == Kind::Phoenix && form.stage == 1).then_some(form.direction),
    }
    .pose();
    for (name, angles) in [
        ("body", [0.0; 3]),
        ("head", pose.head),
        ("right_arm", pose.arms[0]),
        ("left_arm", pose.arms[1]),
        ("right_leg", pose.legs[0]),
        ("left_leg", pose.legs[1]),
    ] {
        if !p.has(name) {
            continue;
        }
        cast_animation::set_rotation(p.get(name), arcana_core::animation::rotation(angles));
    }
    for (part, overlay, angle, hinge) in [
        ("right_arm", "right_sleeve", pose.elbow, 4.0),
        ("left_arm", "left_sleeve", pose.elbow, 4.0),
        ("right_leg", "right_pants", pose.knee, 6.0),
        ("left_leg", "left_pants", pose.knee, 6.0),
    ] {
        let bend = Bend {
            axis: 0.0,
            angle: angle.to_radians(),
            hinge,
        };
        p.bends.insert(part.into(), bend);
        p.bends.insert(overlay.into(), bend);
    }
    cast_animation::apply_root(p, pose.root, Vec3::ZERO);
}

fn shell(source: &Part) -> Part {
    let mut model = source.clone();
    fn inflate(part: &mut Part) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for v in part.polygons.iter().flat_map(|f| &f.vertices) {
            let p = Vec3::from_slice(&v[..3]);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let center = (lo + hi) * 0.5;
        for v in part.polygons.iter_mut().flat_map(|f| &mut f.vertices) {
            let point = Vec3::from_slice(&v[..3]);
            v[..3].copy_from_slice(&(point + (point - center).signum() * 0.3).to_array());
        }
        for child in &mut part.children {
            inflate(child);
        }
    }
    inflate(&mut model);
    model
}
fn model(slim: bool, source: &Part) -> &Part {
    // Prepared player geometry is immutable; packs replace its textures.
    static WIDE: OnceLock<Part> = OnceLock::new();
    static SLIM: OnceLock<Part> = OnceLock::new();
    if slim { &SLIM } else { &WIDE }.get_or_init(|| shell(source))
}
/// Return true when the form owns this body pass, including an invisible body.
#[allow(clippy::too_many_arguments)]
pub fn render(
    m: &Mob,
    source: &Part,
    base: Mat4,
    poses: &Poses,
    atlas: Atlas,
    body_visible: bool,
    out: &mut Vec<crate::mesher::Vertex>,
) -> bool {
    let Some(form) = active(m) else {
        return false;
    };
    let Some(uv) = atlas(&form.texture()) else {
        return false;
    };
    let Some(glow) = atlas("arcana_four_elements:entity/body_fire_glow") else {
        return false;
    };
    if !body_visible || m.invisible {
        return true;
    }
    let model = model(m.slim, source);
    let mut poses = poses.clone();
    for name in [
        "hat",
        "jacket",
        "right_sleeve",
        "left_sleeve",
        "right_pants",
        "left_pants",
    ] {
        poses.visible(name, false);
    }
    let [color, glow_color] = form.colors();
    let start = out.len();
    render::draw(model, base, &poses, uv, color, 0xf0f0, 0, out);
    render::draw(model, base, &poses, glow, glow_color, 0xf0f0, 0, out);
    for vertex in &mut out[start..] {
        vertex.normal = [0; 4];
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn view() -> View {
        View {
            kind: Kind::Avatar,
            age: 20,
            life: 1200,
            born: 10,
            stage: 0,
            flying: true,
            direction: DVec3::Z,
            received: Instant::now(),
        }
    }
    fn rig() -> Part {
        let mut model = Part {
            name: "root".into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            polygons: vec![],
            children: vec![],
        };
        for (name, at) in [
            ("body", [0.0, 0.0, 0.0]),
            ("head", [0.0, 0.0, 0.0]),
            ("right_arm", [-5.0, 2.0, 0.0]),
            ("left_arm", [5.0, 2.0, 0.0]),
            ("right_leg", [-1.9, 12.0, 0.0]),
            ("left_leg", [1.9, 12.0, 0.0]),
        ] {
            let mut pose = model.pose;
            pose[..3].copy_from_slice(&at);
            model.children.push(Part {
                name: name.into(),
                pose,
                children: vec![],
                polygons: vec![],
            });
        }
        model.children[0].polygons.push(super::super::Polygon {
            normal: [0.0, 0.0, 1.0],
            vertices: [
                [-4.0, 0.0, -2.0, 0.0, 0.0],
                [4.0, 0.0, -2.0, 1.0, 0.0],
                [4.0, 12.0, 2.0, 1.0, 1.0],
                [-4.0, 12.0, 2.0, 0.0, 1.0],
            ],
        });
        model
    }
    fn player() -> Mob {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        std::sync::Arc::make_mut(&mut kind).id = "minecraft:player".into();
        let mut mob = Mob::new(1, kind, horizon_core::geometry::Vec3::ZERO, 0.0);
        mob.extra.fire_form = Some(view());
        mob
    }
    #[test]
    fn flight_rotates_the_whole_skeleton_once_and_casts_take_priority() {
        let mut mob = player();
        let model = rig();
        let state = State {
            pos: 0.0,
            speed: 0.0,
            age: 20.0,
            y_rot: 0.0,
            x_rot: 0.0,
            swing: 0.0,
            baby: false,
            partial: 1.0,
        };
        let mut poses = Poses::new(&model);
        apply(&mob, &state, &mut poses);
        assert!((poses.get("body")[3].to_degrees() - 8.0).abs() < 1e-4);
        assert!((poses.get("head")[3].to_degrees() - 1.6).abs() < 1e-4);
        let source = Vec3::new(-5.0, -10.0, 0.0);
        let shoulder = Vec3::from_slice(&poses.get("right_arm")[..3]) - Vec3::Y * 12.0;
        assert!((shoulder - source).length() > 1.0);
        assert!((shoulder.length() - source.length()).abs() < 1e-4);
        mob.extra.cast_animation = Some(cast_animation::Playback::new(
            arcana_core::casting::CastEvent {
                spell_id: "fireball".into(),
                cast_id: 1,
                start_tick: 0,
                server_tick: 4,
                phase: arcana_core::casting::Phase::Preparation,
            },
        ));
        let mut poses = Poses::new(&model);
        apply(&mob, &state, &mut poses);
        assert_eq!(poses.get("body")[3], 0.0);
        assert!(poses.bends.is_empty());
    }
    #[test]
    fn material_owns_the_body_pass_and_expiry_restores_the_normal_renderer() {
        let mut mob = player();
        let model = rig();
        let poses = Poses::new(&model);
        let atlas = |name: &str| {
            Some(if name.ends_with("glow") {
                [0.6, 0.6, 0.8, 0.8]
            } else {
                [0.1, 0.1, 0.3, 0.3]
            })
        };
        let mut out = vec![];
        assert!(render(
            &mob,
            &model,
            Mat4::IDENTITY,
            &poses,
            &atlas,
            true,
            &mut out
        ));
        assert_eq!(out.len(), 8);
        assert!(out.iter().all(|v| v.normal == [0; 4] && v.light == 0xf0f0));
        assert!(out[..4].iter().all(|v| v.uv[0] <= 0.3));
        out.clear();
        assert!(render(
            &mob,
            &model,
            Mat4::IDENTITY,
            &poses,
            &atlas,
            false,
            &mut out
        ));
        assert!(out.is_empty());
        mob.extra.fire_form.as_mut().unwrap().age = 2400;
        assert!(!render(
            &mob,
            &model,
            Mat4::IDENTITY,
            &poses,
            &atlas,
            true,
            &mut out
        ));
    }
    #[test]
    fn stale_or_finished_form_releases_the_body() {
        let mut form = view();
        assert!(form.alive());
        form.received = Instant::now() - Duration::from_millis(1010);
        assert!(!form.alive());
        form.received = Instant::now();
        form.stage = 2;
        assert!(!form.alive());
        form.stage = 0;
        form.age = form.life;
        assert!(!form.alive());
    }
    #[test]
    fn shell_keeps_pivots_uvs_and_inflates_each_side() {
        let source = Part {
            name: "right_arm".into(),
            pose: [-5.0, 2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            children: vec![],
            polygons: vec![super::super::Polygon {
                normal: [0.0, 0.0, 1.0],
                vertices: [
                    [-2.0, -2.0, -2.0, 0.0, 0.0],
                    [2.0, -2.0, -2.0, 1.0, 0.0],
                    [2.0, 10.0, 2.0, 1.0, 1.0],
                    [-2.0, 10.0, 2.0, 0.0, 1.0],
                ],
            }],
        };
        let grown = shell(&source);
        assert_eq!(grown.pose, source.pose);
        for (a, b) in grown.polygons[0]
            .vertices
            .iter()
            .zip(source.polygons[0].vertices)
        {
            assert_eq!(&a[3..], &b[3..]);
            for i in 0..3 {
                assert!(((a[i] - b[i]).abs() - 0.3).abs() < 1e-5);
            }
        }
    }
}
