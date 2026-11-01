//! Distinct Fire compositions over existing batched world geometry. Dimensions
//! and phase come from server snapshots; quality and distance only change detail.
use super::*;

fn shell(out: &mut Vec<Vertex>, at: Vec3, r: f32, w: f32, t: Rect, c: u32, time: f32) {
    for (u, v) in [(Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y), (Vec3::Z, Vec3::Y)] {
        ring(out, at, u, v, r, w, t, c, time);
    }
}
fn flame(out: &mut Vec<Vertex>, at: Vec3, height: f32, width: f32, t: Rect, color: u32, time: f32) {
    for i in 0..3 {
        let phase = time * 3.0 + i as f32 * 2.1;
        let tip = at + Vec3::Y * height + Vec3::new(phase.sin() * width, 0.0, phase.cos() * width);
        let middle = at
            + Vec3::Y * height * 0.5
            + Vec3::new(phase.cos() * width * 0.5, 0.0, phase.sin() * width * 0.5);
        ribbon(out, at, middle, width, t, color);
        ribbon(out, middle, tip, width * 0.45, t, color);
    }
}
impl Game<'_> {
    pub(in crate::game) fn arcana_school_geometry(
        &self,
        camera: DVec3,
        solid: &mut Vec<Vertex>,
        glow: &mut Vec<Vertex>,
    ) {
        let Some(texture) = self.gui.atlas.sprites.get("block/white_concrete").copied() else {
            return;
        };
        let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
        let quality = match std::env::var("ARCANA_QUALITY").as_deref() {
            Ok("performance") => 0,
            Ok("cinematic") => 2,
            _ => 1,
        };
        let intensity = if reduced { 0.4 } else { 1.0 };
        let amber = |a: f32| rgba(255, 94, 14, (a * intensity).clamp(0.0, 255.0) as u8);
        let gold = |a: f32| rgba(255, 206, 96, (a * intensity).clamp(0.0, 255.0) as u8);
        for v in self.arcana.school.views.values() {
            if v.visual.dimension != self.dim.id() {
                continue;
            }
            let e = &v.visual.effect;
            let elapsed = v.received.elapsed().as_secs_f32();
            if elapsed > 1.0 {
                continue;
            }
            let distance = e.position.distance(camera) as f32;
            if distance > 180.0 {
                continue;
            }
            let detail = if distance > 70.0 || quality == 0 {
                2
            } else if distance > 30.0 {
                3
            } else {
                5 + quality * 2
            };
            let age = e.age as f32 + elapsed * 20.0;
            let time = age / 20.0;
            let fade = ((e.life as f32 - age) / 16.0).clamp(0.0, 1.0) * (age / 8.0).clamp(0.0, 1.0);
            let mut at = (e.position - camera).as_vec3();
            let dir = e.direction.as_vec3();
            let side = dir.cross(Vec3::Y).normalize_or_zero();
            match e.spell_id.as_str() {
                "flame_wall" => {
                    let height = 12.0 * (age / 12.0).clamp(0.0, 1.0);
                    for i in -(detail * 2)..=(detail * 2) {
                        let x = i as f32 / (detail * 2) as f32 * 12.0;
                        let p = at + side * x;
                        flame(
                            solid,
                            p,
                            height * (0.7 + 0.13 * (time * 3.0 + x).sin()),
                            0.34,
                            texture,
                            rgba(245, 54, 5, (100.0 * fade * intensity) as u8),
                            time + x,
                        );
                        flame(
                            glow,
                            p,
                            height * (0.83 + 0.12 * (time * 4.0 + x).sin()),
                            0.22 + 0.08 * (time + x).sin().abs(),
                            texture,
                            amber(65.0 * fade),
                            time + x,
                        );
                        if i % 2 == 0 {
                            flame(
                                glow,
                                p,
                                height * 0.58,
                                0.065,
                                texture,
                                gold(75.0 * fade),
                                time - x,
                            );
                        }
                    }
                    for y in [0.1, height * 0.35, height * 0.8] {
                        ribbon(
                            glow,
                            at - side * 12.0 + Vec3::Y * y,
                            at + side * 12.0 + Vec3::Y * y,
                            0.018,
                            texture,
                            gold(50.0 * fade),
                        );
                    }
                }
                "flame_shield" => {
                    at += Vec3::Y;
                    for i in 0..detail + 2 {
                        let a = time * 1.3 + i as f32 * TAU / (detail + 2) as f32;
                        let p =
                            at + Vec3::new(a.cos() * 0.85, (a * 2.0).sin() * 0.35, a.sin() * 0.85);
                        flame(glow, p, 0.5, 0.07, texture, amber(125.0 * fade), time + a);
                    }
                    shell(
                        glow,
                        at,
                        0.82,
                        0.012,
                        texture,
                        gold(105.0 * fade),
                        time * 0.5,
                    );
                    ring(
                        glow,
                        at - Vec3::Y * 0.85,
                        Vec3::X,
                        Vec3::Z,
                        0.95,
                        0.025,
                        texture,
                        amber(130.0 * fade),
                        -time,
                    );
                }
                "fire_dash" => {
                    let length = dir.length();
                    let axis = dir.normalize_or_zero();
                    let side = axis.cross(Vec3::Y);
                    for i in 0..detail * 3 {
                        let f = i as f32 / (detail * 3) as f32;
                        let p = at + dir * f;
                        flame(
                            glow,
                            p,
                            1.4 * (1.0 - f * 0.5),
                            0.09,
                            texture,
                            amber(105.0 * fade),
                            time + f * 3.0,
                        );
                    }
                    for s in [-1.0, 1.0] {
                        ribbon(
                            glow,
                            at + Vec3::Y * 0.7 + side * s * 0.35,
                            at + axis * length + Vec3::Y * 0.7 + side * s * 0.35,
                            0.035,
                            texture,
                            gold(115.0 * fade),
                        );
                    }
                }
                "fire_ring" => {
                    let radius = (14.75_f32).min(age * 0.16);
                    for i in 0..detail * 6 {
                        let a = i as f32 * TAU / (detail * 6) as f32 + time * 0.05;
                        let p = at + Vec3::new(a.cos() * radius, 0.0, a.sin() * radius);
                        flame(
                            glow,
                            p,
                            1.8 + 0.5 * (a * 3.0 + time * 6.0).sin(),
                            0.11,
                            texture,
                            amber(95.0 * fade),
                            time + a,
                        );
                    }
                    for q in [0.93, 1.0, 1.05] {
                        ring(
                            glow,
                            at + Vec3::Y * 0.12,
                            Vec3::X,
                            Vec3::Z,
                            radius * q,
                            0.025,
                            texture,
                            gold(90.0 * fade),
                            time * 0.4,
                        );
                    }
                }
                "fire_blast" => {
                    let expand = (time / 0.5).clamp(0.0, 1.0);
                    let decay = (1.0 - time / 2.5).max(0.0);
                    shell(
                        glow,
                        at,
                        0.25 + expand * 7.75,
                        0.035,
                        texture,
                        gold(110.0 * decay),
                        time,
                    );
                    for i in 0..detail * 5 {
                        let a = i as f32 * 2.39996;
                        let d = Vec3::new(a.cos(), (i % 4) as f32 * 0.25, a.sin()).normalize();
                        let p = at + d * time.min(1.2) * 6.0 - Vec3::Y * time * time * 0.4;
                        ribbon(
                            glow,
                            p,
                            p - d * 0.6,
                            0.06 * decay,
                            texture,
                            amber(170.0 * decay),
                        );
                    }
                }
                "inferno_beam" => {
                    let end = (e.end - camera).as_vec3();
                    let axis = (end - at).normalize_or_zero();
                    let u = axis.cross(Vec3::Y).normalize_or_zero();
                    let up = u.cross(axis);
                    ribbon(
                        solid,
                        at,
                        end,
                        0.1,
                        texture,
                        rgba(255, 100, 12, (140.0 * fade * intensity) as u8),
                    );
                    ribbon(glow, at, end, 0.075, texture, gold(190.0 * fade));
                    let length = at.distance(end);
                    for spiral in 0..detail {
                        for i in 0..20 {
                            let q = i as f32 / 20.0;
                            let q2 = (i + 1) as f32 / 20.0;
                            let a = q * length * 2.5 - time * 12.0
                                + spiral as f32 * TAU / detail as f32;
                            let b = q2 * length * 2.5 - time * 12.0
                                + spiral as f32 * TAU / detail as f32;
                            let p = at.lerp(end, q) + (u * a.cos() + up * a.sin()) * 0.25;
                            let next = at.lerp(end, q2) + (u * b.cos() + up * b.sin()) * 0.25;
                            ribbon(glow, p, next, 0.035, texture, amber(125.0 * fade));
                        }
                    }
                    shell(
                        glow,
                        end,
                        0.4 + 0.1 * (time * 16.0).sin(),
                        0.025,
                        texture,
                        gold(150.0 * fade),
                        time * 2.0,
                    );
                }
                "fire_storm" => {
                    at += dir * 4.0 * elapsed.min(0.15);
                    for arm in 0..detail {
                        for level in 0..28 {
                            let q = level as f32 / 28.0;
                            let q2 = (level + 1) as f32 / 28.0;
                            let point = |q: f32| {
                                let a =
                                    q * TAU * 2.0 - time * 5.0 + arm as f32 * TAU / detail as f32;
                                let r = 0.65 + 7.0 * q.powf(1.2);
                                at + Vec3::new(
                                    (age * 0.025 + q * 3.8).sin() * 7.0 * 0.14 * q + a.cos() * r,
                                    q * 14.0,
                                    (age * 0.021 + q * 4.5).cos() * 7.0 * 0.11 * q + a.sin() * r,
                                )
                            };
                            ribbon(
                                solid,
                                point(q),
                                point(q2),
                                0.06 + q * 0.08,
                                texture,
                                rgba(245, 68, 8, (130.0 * fade * intensity) as u8),
                            );
                            ribbon(
                                glow,
                                point(q),
                                point(q2),
                                0.09 + q * 0.08,
                                texture,
                                if arm % 2 == 0 {
                                    amber(110.0 * fade)
                                } else {
                                    gold(85.0 * fade)
                                },
                            );
                        }
                    }
                    ring(
                        glow,
                        at + Vec3::Y * 0.12,
                        Vec3::X,
                        Vec3::Z,
                        2.0,
                        0.03,
                        texture,
                        amber(110.0 * fade),
                        time,
                    );
                }
                "meteor_rain" => {
                    let y = 14.0;
                    let r = 56.0;
                    for scale in [0.32, 0.65, 1.0] {
                        ring(
                            glow,
                            at + Vec3::Y * y,
                            Vec3::X,
                            Vec3::Z,
                            r * scale,
                            0.06,
                            texture,
                            amber(55.0 * fade),
                            time * 0.08,
                        );
                    }
                    for i in 0..detail * 3 {
                        let a = i as f32 * TAU / (detail * 3) as f32 + time * 0.15;
                        flame(
                            glow,
                            at + Vec3::new(a.cos() * r, y, a.sin() * r),
                            1.0,
                            0.1,
                            texture,
                            gold(100.0 * fade),
                            time + a,
                        );
                    }
                }
                "phoenix_form" => {
                    if e.stage == 2 {
                        let since = (age - (e.life - 40) as f32) / 20.0;
                        let r = (since * 30.0).min(20.0);
                        let a = (1.0 - since / 2.0).max(0.0);
                        shell(glow, at, r, 0.06, texture, gold(120.0 * a), time);
                        for i in 0..detail * 4 {
                            let theta = i as f32 * TAU / (detail * 4) as f32;
                            flame(
                                glow,
                                at + Vec3::new(theta.cos() * r, 0.0, theta.sin() * r),
                                4.0 * a,
                                0.15,
                                texture,
                                amber(120.0 * a),
                                time + theta,
                            );
                        }
                    } else {
                        if e.stage == 1 {
                            at += dir * 80.0 * elapsed.min(0.12);
                        } else {
                            at += Vec3::Y * 6.4 * elapsed.min(0.12);
                        }
                        let heading = dir.try_normalize().unwrap_or(Vec3::Z);
                        let center = at + Vec3::Y * 1.3;
                        super::phoenix::bird(
                            solid,
                            glow,
                            center,
                            heading,
                            time,
                            e.stage == 1,
                            detail,
                            texture,
                            fade * intensity,
                        );
                        if e.stage == 1 {
                            for i in 1..=detail {
                                flame(
                                    glow,
                                    at - heading * i as f32 * 0.9,
                                    1.2,
                                    0.15,
                                    texture,
                                    amber(110.0 * fade / (i as f32).sqrt()),
                                    time - i as f32,
                                );
                            }
                        }
                    }
                }
                "fire_avatar" => {
                    for i in 0..detail * 2 {
                        let a = i as f32 * TAU / (detail * 2) as f32 + time * 1.4;
                        flame(
                            glow,
                            at + Vec3::new(a.cos() * 0.55, 0.0, a.sin() * 0.55),
                            2.2 + 0.2 * (time * 8.0 + a).sin(),
                            0.08,
                            texture,
                            amber(100.0 * fade),
                            time + a,
                        );
                    }
                    for h in [0.25, 1.0, 1.8] {
                        ring(
                            glow,
                            at + Vec3::Y * h,
                            Vec3::X,
                            Vec3::Z,
                            0.7,
                            0.014,
                            texture,
                            gold(125.0 * fade),
                            time * (h + 0.5),
                        );
                    }
                }
                _ => {}
            }
        }
        if let Some(fires) = self
            .arcana
            .school
            .public_fire
            .get(self.dim.id())
            .filter(|f| f.received.elapsed().as_secs_f32() < 2.5)
        {
            let fire_texture = self
                .gui
                .atlas
                .sprites
                .get("block/fire_0")
                .copied()
                .unwrap_or(texture);
            for p in &fires.cells {
                let at = (DVec3::new(
                    f64::from(p[0]) + 0.5,
                    f64::from(p[1]),
                    f64::from(p[2]) + 0.5,
                ) - camera)
                    .as_vec3();
                if at.length_squared() > 100.0 * 100.0 {
                    continue;
                }
                for axis in [Vec3::X, Vec3::Z] {
                    quad(
                        solid,
                        [
                            at - axis * 0.48,
                            at + axis * 0.48,
                            at + axis * 0.36 + Vec3::Y * 1.25,
                            at - axis * 0.36 + Vec3::Y * 1.25,
                        ],
                        fire_texture,
                        rgba(255, 235, 220, if reduced { 85 } else { 220 }),
                    );
                }
            }
        }
    }
}
