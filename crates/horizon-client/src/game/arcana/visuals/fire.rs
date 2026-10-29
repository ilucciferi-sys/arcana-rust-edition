//! A target-bound burning glyph and a wide forward heat wave have different
//! silhouettes. Both use Horizon's existing additive material and depth test.
use super::*;
use crate::game::arcana::starters::StarterVisual;

pub(super) fn draw(
    m: &StarterVisual,
    at: Vec3,
    elapsed: f32,
    time: f32,
    texture: Rect,
    glow: &mut Vec<Vertex>,
) -> bool {
    if !["ignition", "fire_push"].contains(&m.spell_id.as_str()) {
        return false;
    }
    let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
    let intensity = if reduced { 0.45 } else { 1.0 };
    let amber = |alpha: f32| rgba(255, 110, 20, (alpha * intensity) as u8);
    let gold = |alpha: f32| rgba(255, 205, 95, (alpha * intensity) as u8);
    let direction = m.direction.as_vec3().normalize_or_zero();
    let reference = if direction.y.abs() > 0.95 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let side = direction.cross(reference).normalize_or_zero();
    let up = side.cross(direction).normalize_or_zero();
    if m.stage == VisualStage::Preparation {
        if m.spell_id == "ignition" {
            ring(
                glow,
                at,
                side,
                up,
                0.22,
                0.012,
                texture,
                gold(110.0),
                time * 3.0,
            );
            for i in 0..6 {
                let a = time * 3.0 + i as f32 / 6.0 * TAU;
                let ray = side * a.cos() + up * a.sin();
                ribbon(
                    glow,
                    at + ray * 0.1,
                    at + ray * 0.3,
                    0.012,
                    texture,
                    amber(90.0),
                );
            }
        } else {
            for i in 0..3 {
                ring(
                    glow,
                    at + direction * i as f32 * 0.13,
                    side,
                    up,
                    0.12 + i as f32 * 0.055,
                    0.008,
                    texture,
                    amber(90.0),
                    -time * 2.0,
                );
            }
        }
        return true;
    }
    if m.stage == VisualStage::Ignition {
        let fade = (1.0 - elapsed / 1.2).clamp(0.0, 1.0);
        let base = at + Vec3::Y * 0.04;
        let radius = 0.45 + (elapsed * 4.0).min(1.0) * 0.95;
        ring(
            glow,
            base,
            Vec3::X,
            Vec3::Z,
            radius,
            0.018 * fade,
            texture,
            amber(165.0 * fade),
            time * 2.0,
        );
        ring(
            glow,
            base + Vec3::Y * 0.025,
            Vec3::X,
            Vec3::Z,
            radius * 0.66,
            0.012 * fade,
            texture,
            gold(135.0 * fade),
            -time,
        );
        // Six radial strokes lock the rune to the server's chosen impact point.
        for i in 0..6 {
            let a = i as f32 / 6.0 * TAU + time * 0.3;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            ribbon(
                glow,
                base + d * radius * 0.72,
                base + d * radius * 1.16 + Vec3::Y * 0.03,
                0.022 * fade,
                texture,
                gold(110.0 * fade),
            );
        }
        for i in 0..16 {
            let phase = (elapsed * 1.5 + i as f32 / 16.0).fract();
            let angle = time * 3.0 + i as f32 * 2.4;
            let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
            let point = base + radial * (0.2 + phase * 0.55) + Vec3::Y * (phase * 2.5);
            ribbon(
                glow,
                point,
                point + Vec3::Y * (0.12 + fade * 0.35) - radial * 0.12,
                0.025 * fade,
                texture,
                amber(125.0 * fade * (1.0 - phase)),
            );
        }
        if elapsed < 0.2 {
            ribbon(
                glow,
                base,
                base + Vec3::Y * 2.7,
                0.035 * fade,
                texture,
                gold(120.0 * (1.0 - elapsed / 0.2)),
            );
        }
        return true;
    }
    if m.stage == VisualStage::FirePush {
        let range = crate::game::arcana::catalog()
            .get(&m.spell_id)
            .map_or(12.0, |s| s.radius as f32);
        let fade = (1.0 - elapsed / 4.0).max(0.0);
        let origin = at + Vec3::Y * 0.12;
        // The cap has the same .6 angular threshold as the combat cone.
        if elapsed < 0.85 {
            for i in 0..4 {
                let q = ((elapsed * 1.7) - i as f32 * 0.08).clamp(0.0, 1.0);
                if q == 0.0 {
                    continue;
                }
                let r = range * q;
                let a = (1.0 - elapsed / 0.85).max(0.0);
                ring(
                    glow,
                    origin + direction * (r * 0.6),
                    side,
                    up,
                    r * 0.8,
                    0.022,
                    texture,
                    amber(115.0 * a),
                    time + i as f32,
                );
                ring(
                    glow,
                    origin + direction * (r * 0.78),
                    side,
                    up,
                    r * 0.4,
                    0.012,
                    texture,
                    gold(80.0 * a),
                    -time,
                );
            }
        }
        let forward = (direction * Vec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
        let lateral = forward.cross(Vec3::Y);
        for i in 0..32 {
            let angle = ((i as f32 * 2.39996).sin()) * 0.9273;
            let r = 0.7 + (i as f32 / 31.0) * range * (elapsed * 2.0).min(1.0);
            let outward = forward * angle.cos() + lateral * angle.sin();
            let drift = ((elapsed * 0.65 + i as f32 * 0.17).fract()) * 1.4;
            let p = origin + outward * r + Vec3::Y * drift;
            let color = if i % 3 == 0 {
                gold(80.0 * fade * fade)
            } else {
                amber(95.0 * fade * fade)
            };
            ribbon(
                glow,
                p,
                p - outward * (0.15 + fade * 0.4) + Vec3::Y * 0.12,
                0.018 * fade,
                texture,
                color,
            );
        }
        return true;
    }
    true
}
