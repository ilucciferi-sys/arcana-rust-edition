//! Bounded, allocation-free charge geometry. The caller supplies the cast clock
//! and posed anchors; no gameplay, spell IDs or network state live here.
use crate::Quality;
use glam::Vec3;
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Spark,
    Orb,
    Touch,
    Push,
    Ground,
    Guard,
    Ring,
    Blast,
    Beam,
    Vortex,
    Wings,
    Meteors,
    Spear,
    Avatar,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Sigil,
    Halo,
    Flame(u8),
}
#[derive(Clone, Copy, Debug)]
pub enum Mark {
    Line {
        a: Vec3,
        b: Vec3,
        width: f32,
        color: [u8; 4],
    },
    Plane {
        at: Vec3,
        x: Vec3,
        y: Vec3,
        material: Material,
        color: [u8; 4],
    },
}
#[derive(Clone, Copy)]
pub struct Anchors {
    pub hand: Vec3,
    pub other: Vec3,
    pub feet: Vec3,
    pub forward: Vec3,
}
#[derive(Clone, Copy)]
pub struct View {
    pub quality: Quality,
    pub first_person: bool,
    pub reduced: bool,
}
#[derive(Clone, Copy)]
pub struct Charge {
    pub family: Family,
    pub tier: u8,
    pub progress: f32,
    pub age: f32,
    pub opacity: f32,
}
fn smooth(q: f32) -> f32 {
    let q = q.clamp(0.0, 1.0);
    q * q * (3.0 - 2.0 * q)
}
/// Preserve a flat effect's orientation but select the visible winding for
/// renderers whose translucent entity batch culls back faces.
pub fn camera_facing_quad(at: Vec3, x: Vec3, y: Vec3) -> [Vec3; 4] {
    let mut points = [at - x - y, at + x - y, at + x + y, at - x + y];
    if x.cross(y).dot(-at) < 0.0 {
        points.reverse();
    }
    points
}
/// Clip one textured view-model quad against a near plane, retaining UVs. A
/// fixed scratch polygon bounds the work even for a bent arm's triangle quad.
pub fn clip_near_eye(
    points: [[f32; 5]; 4],
    forward: Vec3,
    near: f32,
    mut emit: impl FnMut([[f32; 5]; 4]),
) {
    let Some(forward) = forward.try_normalize() else {
        return;
    };
    if !near.is_finite() || points.iter().flatten().any(|v| !v.is_finite()) {
        return;
    }
    let mut polygon = [[0.0; 5]; 8];
    let mut count = 0;
    for i in 0..4 {
        let (a, b) = (points[i], points[(i + 1) % 4]);
        let da = Vec3::from_slice(&a[..3]).dot(forward) - near;
        let db = Vec3::from_slice(&b[..3]).dot(forward) - near;
        if da >= 0.0 {
            polygon[count] = a;
            count += 1;
        }
        if (da >= 0.0) != (db >= 0.0) {
            let q = da / (da - db);
            polygon[count] = std::array::from_fn(|j| a[j] + (b[j] - a[j]) * q);
            count += 1;
        }
    }
    if count == 4 {
        emit(polygon[..4].try_into().unwrap());
    } else {
        for i in 1..count.saturating_sub(1) {
            emit([polygon[0], polygon[i], polygon[i + 1], polygon[i + 1]]);
        }
    }
}
impl Charge {
    pub fn sample(family: Family, tier: u8, age: f32, duration: f32, fade: f32) -> Option<Self> {
        if ![age, duration, fade].iter().all(|v| v.is_finite()) || age < 0.0 || duration <= 0.0 {
            return None;
        }
        let opacity =
            smooth(age / 2.0) * (1.0 - smooth((age - duration) / 4.0)) * fade.clamp(0.0, 1.0);
        (opacity > 0.001).then_some(Self {
            family,
            tier: tier.clamp(1, 5),
            progress: (age / duration).clamp(0.0, 1.0),
            age,
            opacity,
        })
    }
    pub fn draw(self, anchors: Anchors, view: View, mut emit: impl FnMut(Mark)) -> usize {
        if ![anchors.hand, anchors.other, anchors.feet, anchors.forward]
            .iter()
            .all(|p| p.is_finite())
            || anchors.feet.length_squared() > 96.0 * 96.0
        {
            return 0;
        }
        let distant = anchors.feet.length_squared() > 32.0 * 32.0;
        let segments = if distant || view.reduced {
            12
        } else {
            match view.quality {
                Quality::Performance => 12,
                Quality::High => 24,
                Quality::Cinematic => 32,
            }
        };
        let budget = match view.quality {
            Quality::Performance => 240,
            Quality::High => 480,
            Quality::Cinematic => 640,
        };
        let mut count = 0;
        let mut draw = |mark| {
            let alpha = match mark {
                Mark::Line { color, .. } | Mark::Plane { color, .. } => color[3],
            };
            if count < budget && alpha > 0 {
                emit(mark);
                count += 1;
            }
        };
        let strength = self.opacity
            * if view.reduced { 0.38 } else { 1.0 }
            * if view.first_person { 0.65 } else { 1.0 };
        let amber = |a: f32| [255, 112, 40, (a * strength).clamp(0.0, 255.0) as u8];
        let gold = |a: f32| [255, 225, 150, (a * strength).clamp(0.0, 255.0) as u8];
        let dir = anchors.forward.try_normalize().unwrap_or(Vec3::Z);
        let side = dir
            .cross(if dir.y.abs() > 0.9 { Vec3::X } else { Vec3::Y })
            .normalize();
        let up = side.cross(dir).normalize();
        let q = self.progress;
        let build = smooth((q - 0.08) / 0.74);
        let turn = self.age * if q < 0.82 { 0.025 } else { 0.008 };
        let hand = anchors.hand + dir * 0.025;
        let raw_radius = if view.first_person {
            0.14 + self.tier as f32 * 0.018
        } else {
            0.26 + self.tier as f32 * 0.10
        };
        // Limit the angular size when a casting pose puts the palm near the eye.
        let radius = if view.first_person {
            raw_radius.min(hand.length() * 0.12).max(0.012)
        } else {
            raw_radius
        };
        seal(
            &mut draw,
            [hand, side, up],
            radius,
            build,
            turn,
            segments,
            amber(140.0),
        );
        if !distant && !view.reduced {
            seal(
                &mut draw,
                [anchors.other, side, up],
                radius * 0.55,
                build,
                -turn,
                segments,
                gold(65.0),
            );
        }
        let form = smooth((q - 0.30) / 0.52);
        let size =
            (0.035 + form * if view.first_person { 0.07 } else { 0.14 }) * radius / raw_radius;
        let centre = hand + dir * 0.08;
        let line = |a, b, width, color| Mark::Line { a, b, width, color };
        match self.family {
            Family::Spark | Family::Orb => {
                let orbits = if self.family == Family::Spark { 1 } else { 3 };
                for i in 0..orbits {
                    let a = self.age * 0.11 + i as f32 * TAU / orbits as f32;
                    let tip = centre + (side * a.cos() + up * a.sin()) * (size * 1.7);
                    draw(line(tip, centre, 0.014, gold(125.0 * form)));
                }
            }
            Family::Touch => {
                for i in 0..6 {
                    let a = i as f32 * TAU / 6.0 + turn;
                    let ray = side * a.cos() + up * a.sin();
                    draw(line(
                        centre + ray * size,
                        centre + ray * size * 2.8,
                        0.009,
                        gold(145.0 * build),
                    ));
                }
            }
            Family::Push | Family::Beam => {
                for i in 0..3 {
                    let at = hand + dir * (0.08 + i as f32 * 0.10);
                    arc(
                        &mut draw,
                        [at, side, up],
                        radius
                            * if self.family == Family::Beam {
                                0.75 - i as f32 * 0.18
                            } else {
                                0.6 + i as f32 * 0.16
                            },
                        build,
                        -turn,
                        segments,
                        amber(90.0),
                    );
                }
            }
            Family::Ground => {
                for i in 0..5 {
                    let x = (i as f32 - 2.0) * radius * 0.34;
                    draw(line(
                        centre + side * x - up * size,
                        centre + side * x + up * (size + form * 0.12),
                        0.012,
                        amber(150.0 * build),
                    ));
                }
            }
            Family::Guard => {
                for i in 0..3 {
                    let a = turn + i as f32 * TAU / 3.0;
                    let normal = side * a.cos() + dir * a.sin();
                    arc(
                        &mut draw,
                        [centre, normal, up],
                        radius * 0.65,
                        build,
                        -turn,
                        segments,
                        gold(80.0),
                    );
                }
            }
            Family::Avatar => {
                for i in 0..2 {
                    let a = turn + i as f32 * TAU / 2.0;
                    let ray = side * a.cos() + up * a.sin();
                    draw(line(
                        centre - ray * radius * 0.75,
                        centre + ray * radius * 0.75,
                        0.018,
                        gold(100.0 * build),
                    ));
                }
                if !view.first_person {
                    for i in 0..3 {
                        arc(
                            &mut draw,
                            [
                                anchors.feet + Vec3::Y * (0.35 + i as f32 * 0.45),
                                Vec3::X,
                                Vec3::Z,
                            ],
                            0.6 + form * 0.15,
                            build,
                            turn,
                            segments,
                            amber(70.0),
                        );
                    }
                }
            }
            Family::Ring => {
                for i in 0..8 {
                    let a = i as f32 * TAU / 8.0 - turn;
                    let ray = side * a.cos() + up * a.sin();
                    draw(line(
                        centre + ray * radius * 0.55,
                        centre + ray * radius * (0.65 + 0.35 * build),
                        0.012,
                        amber(125.0),
                    ));
                }
            }
            Family::Blast => {
                for i in 0..12 {
                    let a = i as f32 * TAU / 12.0 + self.age * 0.07;
                    let radial = side * a.cos() + up * a.sin();
                    draw(line(
                        centre + radial * radius,
                        centre + radial * radius * (1.0 - form * 0.8),
                        0.012,
                        gold(100.0 * build),
                    ));
                }
            }
            Family::Vortex => {
                let mut previous = centre;
                for i in 0..segments {
                    let h = i as f32 / segments as f32;
                    let a = h * TAU * 2.0 - self.age * 0.16;
                    let next = centre
                        + (side * a.cos() + dir * a.sin()) * radius * h
                        + up * h * radius * 1.2;
                    draw(line(previous, next, 0.009, amber(130.0 * build)));
                    previous = next;
                }
            }
            Family::Wings => {
                for sign in [-1.0, 1.0] {
                    for i in 0..5 {
                        let h = i as f32 / 4.0;
                        let tip = centre
                            + side * sign * radius * (0.35 + 0.8 * h)
                            + up * radius * (1.0 - h * h) * form;
                        draw(line(centre, tip, 0.012, gold(100.0 * build)));
                    }
                }
            }
            Family::Meteors => {
                for i in 0..4 {
                    let a = i as f32 * TAU / 4.0 + turn;
                    let at = centre + (side * a.cos() + up * a.sin()) * radius * 0.65;
                    draw(line(
                        at,
                        at + up * size * 1.5 - dir * size,
                        0.024,
                        amber(170.0 * form),
                    ));
                }
            }
            Family::Spear => {} // The independent GPU lance supplies its charging core.
        }
        if self.family != Family::Spear && form > 0.01 {
            let frame = (self.age / 2.0) as u8 % 8;
            for i in 0..if distant || view.reduced { 1 } else { 3 } {
                let a = i as f32 * TAU / 3.0 + self.age * 0.04;
                let at = centre + (side * a.cos() + dir * a.sin()) * size * 0.45;
                draw(Mark::Plane {
                    at,
                    x: side * size,
                    y: up * size * 1.7,
                    material: Material::Flame(frame),
                    color: [255, 255, 255, (115.0 * strength * form) as u8],
                });
            }
        }
        let feet_seal = self.tier >= 3
            || matches!(
                self.family,
                Family::Ground | Family::Guard | Family::Avatar | Family::Wings
            );
        if feet_seal {
            seal(
                &mut draw,
                [anchors.feet + Vec3::Y * 0.035, Vec3::X, Vec3::Z],
                0.65 + self.tier as f32 * 0.22,
                build,
                turn * 0.42,
                segments,
                amber(105.0),
            );
        }
        if self.tier == 5 && !distant && !view.first_person && !view.reduced {
            for i in 0..3 {
                seal(
                    &mut draw,
                    [
                        anchors.feet + Vec3::Y * (2.9 + i as f32 * 1.25),
                        Vec3::X,
                        Vec3::Z,
                    ],
                    1.5 + i as f32 * 0.62,
                    smooth((q - 0.08 - i as f32 * 0.09) / 0.74),
                    turn * if i % 2 == 0 { 0.4 } else { -0.3 },
                    segments,
                    amber(36.0),
                );
            }
        }
        count
    }
}
fn arc(
    draw: &mut impl FnMut(Mark),
    basis: [Vec3; 3],
    r: f32,
    build: f32,
    turn: f32,
    segments: u32,
    color: [u8; 4],
) {
    let [at, u, v] = basis;
    for i in 0..(segments as f32 * build).ceil() as u32 {
        let point = |q: f32| at + (u * q.cos() + v * q.sin()) * r;
        draw(Mark::Line {
            a: point(i as f32 * TAU / segments as f32 + turn),
            b: point((i as f32 + 0.86) * TAU / segments as f32 + turn),
            width: 0.009,
            color,
        });
    }
}
fn seal(
    draw: &mut impl FnMut(Mark),
    basis: [Vec3; 3],
    r: f32,
    build: f32,
    turn: f32,
    segments: u32,
    color: [u8; 4],
) {
    let [at, u, v] = basis;
    arc(draw, basis, r, build, turn, segments, color);
    arc(draw, basis, r * 0.75, build, -turn, segments, color);
    let mut tint = color;
    tint[3] = (color[3] as f32 * ((build - 0.6) / 0.4).clamp(0.0, 1.0)) as u8;
    let (s, c) = turn.sin_cos();
    draw(Mark::Plane {
        at,
        x: (u * c + v * s) * r,
        y: (v * c - u * s) * r,
        material: Material::Sigil,
        color: tint,
    });
    tint[3] = (tint[3] as f32 * 0.15) as u8;
    draw(Mark::Plane {
        at,
        x: u * r * 1.04,
        y: v * r * 1.04,
        material: Material::Halo,
        color: tint,
    });
    for i in 0..6 {
        if build <= (i as f32 + 1.0) / 8.0 {
            continue;
        }
        let a = i as f32 * TAU / 6.0 - turn;
        let centre = at + (u * a.cos() + v * a.sin()) * r * 0.86;
        draw(Mark::Plane {
            at: centre,
            x: u * r * 0.10,
            y: v * r * 0.10,
            material: Material::Sigil,
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seals_face_the_viewer_from_both_sides() {
        for at in [Vec3::new(0.0, -2.0, 1.0), Vec3::new(0.0, 2.0, 1.0)] {
            let p = camera_facing_quad(at, Vec3::X, Vec3::Z);
            assert!((p[1] - p[0]).cross(p[2] - p[0]).dot(-at) > 0.0);
        }
    }
    #[test]
    fn near_eye_clipping_removes_blocking_arms_and_interpolates_skin_uvs() {
        let quad = [
            [-0.2, -0.2, 0.1, 0.0, 0.0],
            [0.2, -0.2, 0.6, 1.0, 0.0],
            [0.2, 0.2, 0.6, 1.0, 1.0],
            [-0.2, 0.2, 0.1, 0.0, 1.0],
        ];
        let mut clipped = vec![];
        clip_near_eye(quad, Vec3::Z, 0.28, |p| clipped.extend(p));
        assert!(!clipped.is_empty());
        assert!(clipped.iter().all(|p| p[2] >= 0.27999
            && p.iter().all(|v| v.is_finite())
            && (0.0..=1.0).contains(&p[3])
            && (0.0..=1.0).contains(&p[4])));
        assert!(clipped.iter().any(|p| p[3] > 0.0 && p[3] < 1.0));
        let mut hidden = quad;
        for p in &mut hidden {
            p[2] = 0.05;
        }
        clip_near_eye(hidden, Vec3::Z, 0.28, |_| {
            panic!("near geometry must be clipped")
        });
    }
    const FAMILIES: [Family; 14] = [
        Family::Spark,
        Family::Orb,
        Family::Touch,
        Family::Push,
        Family::Ground,
        Family::Guard,
        Family::Ring,
        Family::Blast,
        Family::Beam,
        Family::Vortex,
        Family::Wings,
        Family::Meteors,
        Family::Spear,
        Family::Avatar,
    ];
    fn anchors() -> Anchors {
        Anchors {
            hand: Vec3::new(0.3, -0.4, 0.7),
            other: Vec3::new(-0.3, -0.4, 0.7),
            feet: Vec3::new(0.0, -1.6, 0.0),
            forward: Vec3::Z,
        }
    }
    #[test]
    fn invalid_clock_and_finished_charge_emit_nothing() {
        for age in [-1.0, f32::NAN, 34.0, 300.0] {
            assert!(Charge::sample(Family::Beam, 4, age, 30.0, 1.0).is_none());
        }
        assert!(Charge::sample(Family::Orb, 2, 12.0, 15.0, 0.0).is_none());
    }
    #[test]
    fn a_palm_close_to_the_eye_keeps_its_first_person_seal_small() {
        let mut a = anchors();
        a.hand = Vec3::new(0.01, 0.01, 0.1);
        let c = Charge::sample(Family::Orb, 5, 65.0, 70.0, 1.0).unwrap();
        let mut checked = false;
        c.draw(
            a,
            View {
                quality: Quality::High,
                first_person: true,
                reduced: false,
            },
            |mark| {
                if let Mark::Plane {
                    at,
                    x,
                    y,
                    material: Material::Sigil,
                    ..
                } = mark
                    && at.distance(a.hand) < 0.03
                {
                    assert!(x.length() < 0.03 && y.length() < 0.03);
                    checked = true;
                }
            },
        );
        assert!(checked);
    }
    #[test]
    fn all_families_are_finite_bounded_and_distinct() {
        let mut signatures = std::collections::HashSet::new();
        for family in FAMILIES {
            let charge = Charge::sample(family, 5, 50.0, 70.0, 1.0).unwrap();
            let mut signature = String::new();
            let count = charge.draw(
                anchors(),
                View {
                    quality: Quality::Cinematic,
                    first_person: false,
                    reduced: false,
                },
                |mark| {
                    match mark {
                        Mark::Line { a, b, width, .. } => {
                            assert!(a.is_finite() && b.is_finite() && width > 0.0)
                        }
                        Mark::Plane { at, x, y, .. } => {
                            assert!(at.is_finite() && x.is_finite() && y.is_finite())
                        }
                    }
                    signature.push_str(&format!("{mark:?}"));
                },
            );
            assert!(count > 0 && count <= 640);
            assert!(signatures.insert(signature), "{family:?}");
        }
    }
    #[test]
    fn reduced_and_distant_views_use_less_geometry_and_expire_at_range() {
        let c = Charge::sample(Family::Avatar, 5, 60.0, 70.0, 1.0).unwrap();
        let view = View {
            quality: Quality::High,
            first_person: false,
            reduced: false,
        };
        let full = c.draw(anchors(), view, |_| {});
        let low = c.draw(
            anchors(),
            View {
                reduced: true,
                ..view
            },
            |_| {},
        );
        let mut far = anchors();
        far.feet.z = 50.0;
        assert!(low < full && c.draw(far, view, |_| {}) < full);
        far.feet.z = 100.0;
        assert_eq!(c.draw(far, view, |_| {}), 0);
    }
}
