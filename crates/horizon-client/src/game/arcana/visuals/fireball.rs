//! A dense rotating projectile shell, a long flame wake and one expanding
//! impact composition. All geometry follows authoritative projectile messages.
use super::*;
use crate::game::arcana::starters::StarterVisual;

pub(super) fn draw(
    m: &StarterVisual,
    at: Vec3,
    elapsed: f32,
    time: f32,
    texture: Rect,
    solid: &mut Vec<Vertex>,
    glow: &mut Vec<Vertex>,
) -> bool {
    if !matches!(
        m.spell_id.as_str(),
        "fireball" | "fireball_empowered" | "meteor_rain"
    ) {
        return false;
    }
    let begin = glow.len();
    let result = draw_body(m, at, elapsed, time, texture, glow);
    if m.spell_id == "meteor_rain" && m.stage == VisualStage::Flight {
        cube(
            solid,
            at,
            0.46,
            Quat::from_euler(glam::EulerRot::XYZ, time * 2.1, time * 1.7, time),
            texture,
            rgba(68, 31, 12, 255),
        );
    }
    let scale = if m.stage == VisualStage::FireballImpact {
        if m.spell_id == "fireball" {
            1.0
        } else {
            4.0 / 3.0
        }
    } else {
        match m.spell_id.as_str() {
            "meteor_rain" => 8.0 / 2.8,
            "fireball_empowered" => 1.5,
            _ => 1.0,
        }
    };
    if scale != 1.0 {
        for v in &mut glow[begin..] {
            v.pos = (at + (Vec3::from_array(v.pos) - at) * scale).to_array();
        }
    }
    result
}
fn draw_body(
    m: &StarterVisual,
    at: Vec3,
    elapsed: f32,
    time: f32,
    texture: Rect,
    glow: &mut Vec<Vertex>,
) -> bool {
    let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
    let intensity = if reduced { 0.45 } else { 1.0 };
    let amber = |a: f32| rgba(255, 85, 15, (a * intensity) as u8);
    let hot = |a: f32| rgba(255, 220, 130, (a * intensity) as u8);
    let direction = m.direction.as_vec3().normalize_or_zero();
    let reference = if direction.y.abs() > 0.95 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let u = direction.cross(reference).normalize_or_zero();
    let v = u.cross(direction).normalize_or_zero();
    if m.stage == VisualStage::Preparation {
        let charge = (m.age as f32 / 15.0 + elapsed / 0.75).clamp(0.0, 1.0);
        let r = 0.08 + charge * 0.18;
        cube(
            glow,
            at,
            r * 0.4,
            Quat::from_rotation_y(time * 6.0),
            texture,
            hot(120.0),
        );
        for i in 0..3 {
            ring(
                glow,
                at - direction * i as f32 * 0.06,
                u,
                v,
                r + i as f32 * 0.045,
                0.012,
                texture,
                amber(95.0),
                time * (2.0 + i as f32),
            );
        }
        for i in 0..9 {
            let a = i as f32 / 9.0 * TAU + time * 3.0;
            let d = u * a.cos() + v * a.sin();
            ribbon(
                glow,
                at + d * (r + 0.2),
                at + d * r,
                0.015,
                texture,
                hot(90.0),
            );
        }
        return true;
    }
    if m.stage == VisualStage::Flight {
        cube(
            glow,
            at,
            0.16,
            Quat::from_euler(glam::EulerRot::XYZ, time * 5.0, time * 7.0, time * 3.0),
            texture,
            hot(190.0),
        );
        for (a, b, phase) in [(u, v, 1.0), (u, direction, -1.4), (v, direction, 1.8)] {
            ring(
                glow,
                at,
                a,
                b,
                0.34,
                0.045,
                texture,
                amber(150.0),
                time * phase,
            );
        }
        for i in 0..14 {
            let q = i as f32 / 14.0;
            let a = time * 8.0 + i as f32 * 2.4;
            let radial = u * a.cos() + v * a.sin();
            let p = at - direction * (q * 1.7) + radial * (0.08 + q * 0.25);
            ribbon(
                glow,
                p,
                p - direction * (0.25 + q * 0.4) + radial * 0.04,
                0.03 * (1.0 - q * 0.65),
                texture,
                amber(150.0 * (1.0 - q)),
            );
        }
        ring(
            glow,
            at - direction * 0.45,
            u,
            v,
            0.42,
            0.014,
            texture,
            hot(90.0),
            -time * 3.0,
        );
        return true;
    }
    if m.stage == VisualStage::FireballImpact {
        let fade = (1.0 - elapsed / 1.8).max(0.0);
        let burst = (elapsed / 0.45).clamp(0.0, 1.0);
        let radius = 0.2 + burst * 2.8;
        if elapsed < 0.7 {
            let shell = (1.0 - elapsed / 0.7).max(0.0);
            for (a, b) in [(Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y), (Vec3::Z, Vec3::Y)] {
                ring(
                    glow,
                    at,
                    a,
                    b,
                    radius,
                    0.035 * shell,
                    texture,
                    amber(135.0 * shell),
                    time,
                );
            }
            ring(
                glow,
                at,
                Vec3::X,
                Vec3::Z,
                radius * 0.85,
                0.022,
                texture,
                hot(95.0 * shell),
                -time,
            );
        }
        if elapsed < 0.16 {
            cube(
                glow,
                at,
                0.24 * (1.0 - elapsed / 0.16),
                Quat::from_rotation_y(time),
                texture,
                hot(175.0 * (1.0 - elapsed / 0.16)),
            );
        }
        for i in 0..28 {
            let a = i as f32 * 2.39996 + m.cast_id as f32 * 0.13;
            let y = (i as f32 / 27.0) * 1.4 - 0.4;
            let d = Vec3::new(a.cos(), y, a.sin()).normalize();
            let distance = (elapsed * (1.8 + (i % 5) as f32 * 0.2)).min(3.0);
            let p = at + d * distance - Vec3::Y * elapsed * elapsed * 0.6;
            ribbon(
                glow,
                p,
                p - d * (0.15 + fade * 0.32) + Vec3::Y * 0.06,
                0.022 * fade,
                texture,
                if i % 4 == 0 {
                    hot(90.0 * fade)
                } else {
                    amber(130.0 * fade * fade)
                },
            );
        }
        return true;
    }
    true
}
