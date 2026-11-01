//! A feathered silhouette, with a separate downstroke, glide and dive pose.
use super::*;

pub(super) fn bird(
    solid: &mut Vec<Vertex>,
    glow: &mut Vec<Vertex>,
    center: Vec3,
    direction: Vec3,
    time: f32,
    diving: bool,
    detail: i32,
    texture: Rect,
    fade: f32,
) {
    let forward = direction.try_normalize().unwrap_or(Vec3::Z);
    let reference = if forward.y.abs() > 0.97 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let right = forward.cross(reference).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    let flap = if diving {
        0.15
    } else {
        (time * 5.2).sin() * 0.7
    };
    let span = if diving { 2.9 } else { 4.6 };
    let amber = rgba(255, 92, 8, (130.0 * fade) as u8);
    let gold = rgba(255, 202, 76, (145.0 * fade) as u8);
    let red = rgba(224, 39, 3, (155.0 * fade) as u8);
    let feathers = 7 + detail;
    for sign in [-1.0, 1.0] {
        let leading = |q: f32| {
            center
                + right * sign * (0.25 + q * span)
                + up * (flap * q * q + 0.25 * q)
                + forward * (0.15 + (q * std::f32::consts::PI).sin() * 0.6 - q * q * 0.6)
        };
        for i in 0..feathers {
            let q = i as f32 / feathers as f32;
            let next = (i + 1) as f32 / feathers as f32;
            let a = leading(q);
            let b = leading(next);
            let length = 0.65 + (q * std::f32::consts::PI).sin() * 1.05 + q * 0.5;
            let trailing = forward * length + up * (0.12 + q * 0.3);
            let points = [a, b, b - trailing, a - trailing * 0.88];
            quad(solid, points, texture, red);
            quad(
                solid,
                [points[3], points[2], points[1], points[0]],
                texture,
                red,
            );
            ribbon(glow, a, b, 0.06, texture, gold);
            let root = (a + b) * 0.5 - forward * 0.2;
            let tip = (a + b) * 0.5 - trailing * (1.15 + q * 0.35) - right * sign * q * 0.18;
            ribbon(solid, root, tip, 0.13 + q * 0.06, texture, amber);
            ribbon(glow, root, tip, 0.055, texture, gold);
            // Small detached embers break the contour without obscuring the wing.
            if i % 2 == 0 {
                let age = (time * 1.6 + q * 3.7).fract();
                let ember = tip - forward * age * 1.8 + up * age * 0.6;
                cube(
                    glow,
                    ember,
                    0.035 * (1.0 - age),
                    Quat::from_rotation_y(time + q),
                    texture,
                    rgba(255, 132, 24, (110.0 * fade * (1.0 - age)) as u8),
                );
            }
        }
    }
    let breast = center - up * 0.18;
    let head = center + forward * 0.8 + up * 0.2;
    ribbon(solid, breast - forward * 1.0, head, 0.34, texture, amber);
    ribbon(glow, breast - forward * 0.6, head, 0.14, texture, gold);
    ribbon(
        glow,
        head,
        head + forward * 0.55 - up * 0.12,
        0.11,
        texture,
        gold,
    );
    for sign in [-1.0, 1.0] {
        ribbon(
            glow,
            head - right * sign * 0.06,
            head - forward * 0.4 + up * 0.55 + right * sign * 0.13,
            0.07,
            texture,
            gold,
        );
    }
    for i in -2_i32..=2 {
        let spread = i as f32;
        let root = breast - forward * 0.6 + right * spread * 0.12;
        let tip = root - forward * (2.7 - spread.abs() * 0.4)
            + right * spread * 0.4
            + up * ((time * 4.0 + spread * 0.5).sin() * 0.18 - 0.5);
        ribbon(solid, root, tip, 0.22, texture, red);
        ribbon(glow, root, tip, 0.08, texture, amber);
    }
}
