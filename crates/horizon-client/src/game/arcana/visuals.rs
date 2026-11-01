//! Small spell compositions use Horizon's existing geometry/material batches.
//! Solar's high-density effects keep using the separate GPU VFX extension.
mod fire;
mod fireball;
mod school;
use super::{Game, starters::VisualStage};
use glam::{DVec3, Quat, Vec3};
use horizon_client::{
    hud::{Rect, rgba},
    mesher::Vertex,
};
use std::f32::consts::TAU;

fn quad(out: &mut Vec<Vertex>, points: [Vec3; 4], texture: Rect, color: u32) {
    for (pos, uv) in points.into_iter().zip([
        [texture.u0, texture.v0],
        [texture.u1, texture.v0],
        [texture.u1, texture.v1],
        [texture.u0, texture.v1],
    ]) {
        out.push(Vertex {
            pos: pos.to_array(),
            uv,
            layer: 0,
            color,
            light: 0xf0f0,
            normal: [0; 4],
            glint_uv: [0.0; 2],
        });
    }
}
fn ring(
    out: &mut Vec<Vertex>,
    at: Vec3,
    u: Vec3,
    v: Vec3,
    radius: f32,
    width: f32,
    texture: Rect,
    color: u32,
    offset: f32,
) {
    for i in 0..32 {
        let a = i as f32 / 32.0 * TAU + offset;
        let b = (i + 1) as f32 / 32.0 * TAU + offset;
        let p = u * a.cos() + v * a.sin();
        let q = u * b.cos() + v * b.sin();
        quad(
            out,
            [
                at + p * (radius - width),
                at + p * (radius + width),
                at + q * (radius + width),
                at + q * (radius - width),
            ],
            texture,
            color,
        );
    }
}
fn cube(out: &mut Vec<Vertex>, at: Vec3, size: f32, rotation: Quat, texture: Rect, color: u32) {
    for [a, b, c, d] in [
        [0, 1, 3, 2],
        [4, 6, 7, 5],
        [0, 4, 5, 1],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 5, 7, 3],
    ] {
        let point = |index: i32| {
            at + rotation
                * (Vec3::new(
                    if index & 1 == 0 { -1.0 } else { 1.0 },
                    if index & 2 == 0 { -1.0 } else { 1.0 },
                    if index & 4 == 0 { -1.0 } else { 1.0 },
                ) * size)
        };
        quad(
            out,
            [point(a), point(b), point(c), point(d)],
            texture,
            color,
        );
    }
}
fn ribbon(out: &mut Vec<Vertex>, a: Vec3, b: Vec3, width: f32, texture: Rect, color: u32) {
    let direction = (b - a).normalize_or_zero();
    let side = direction
        .cross((-a).normalize_or_zero())
        .normalize_or_zero()
        * width;
    quad(
        out,
        [a - side, a + side, b + side * 0.2, b - side * 0.2],
        texture,
        color,
    );
}
impl Game<'_> {
    pub(in crate::game) fn arcana_ritual_geometry(
        &self,
        camera: DVec3,
        solid: &mut Vec<Vertex>,
        glow: &mut Vec<Vertex>,
    ) {
        use arcana_core::School;
        let Some(texture) = self.gui.atlas.sprites.get("block/white_concrete").copied() else {
            return;
        };
        let rock = self
            .gui
            .atlas
            .sprites
            .get("block/deepslate")
            .copied()
            .unwrap_or(texture);
        let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
        for view in self.arcana.ritual_views.values() {
            let m = &view.message;
            if m.rite.dimension != self.dim.id() {
                continue;
            }
            let at = (DVec3::from_array(m.rite.center.map(f64::from)) + DVec3::new(0.5, 1.04, 0.5)
                - camera)
                .as_vec3();
            if at.length_squared() > 96.0 * 96.0 {
                continue;
            }
            let elapsed = view.received.elapsed().as_secs_f32();
            let progress = ((m.rite.progress as f32 + elapsed.min(0.5) * 20.0)
                / m.rite.duration as f32)
                .min(1.0);
            let fade = if m.finished {
                (1.0 - elapsed / 1.5).max(0.0)
            } else {
                (progress * 5.0).min(1.0)
            };
            let rise = ((progress - 0.2) / 0.8).clamp(0.0, 1.0);
            let time = (m.rite.progress as f32 / 20.0 + elapsed) * 1.4;
            let alpha = (110.0 * fade * if reduced { 0.5 } else { 1.0 }) as u8;
            let rgb = match m.rite.school {
                School::Fire => [255, 100, 25],
                School::Water => [35, 165, 245],
                School::Earth => [195, 150, 75],
                _ => [200, 225, 235],
            };
            let color = rgba(rgb[0], rgb[1], rgb[2], alpha);
            for (radius, sign) in [(2.1, 1.0), (1.25, -1.0)] {
                ring(
                    glow,
                    at,
                    Vec3::X,
                    Vec3::Z,
                    radius,
                    0.016,
                    texture,
                    color,
                    time * sign,
                );
                for i in 0..12 {
                    let a = i as f32 / 12.0 * TAU + time * 0.15 * sign;
                    let radial = Vec3::new(a.cos(), 0.0, a.sin());
                    ribbon(
                        glow,
                        at + radial * radius,
                        at + radial * (radius - 0.16) + Vec3::Y * 0.02,
                        0.025,
                        texture,
                        color,
                    );
                }
            }
            for i in 0..3 {
                ring(
                    glow,
                    at + Vec3::Y * (0.6 + rise * (i as f32 + 1.0)),
                    Vec3::X,
                    Vec3::Z,
                    0.65 + rise * (0.25 + i as f32 * 0.2),
                    0.009,
                    texture,
                    color,
                    time,
                );
            }
            for i in 0..12 {
                let a = i as f32 / 12.0 * TAU
                    + time
                        * if m.rite.school == School::Air {
                            1.8
                        } else {
                            0.65
                        };
                let radial = Vec3::new(a.cos(), 0.0, a.sin());
                let height = (time * 0.5 + i as f32 * 0.19).fract() * (1.0 + rise * 3.0);
                let p = at + radial * (1.3 + 0.3 * (time + i as f32).sin()) + Vec3::Y * height;
                match m.rite.school {
                    School::Earth => cube(
                        solid,
                        p,
                        (0.07 + rise * 0.08) * fade,
                        Quat::from_rotation_y(a),
                        rock,
                        rgba(175, 155, 120, 255),
                    ),
                    School::Water => {
                        ring(glow, p, radial, Vec3::Y, 0.06, 0.012, texture, color, time);
                        ribbon(
                            glow,
                            p,
                            p - radial * 0.18 - Vec3::Y * 0.1,
                            0.025,
                            texture,
                            color,
                        );
                    }
                    School::Fire => ribbon(
                        glow,
                        p,
                        p + Vec3::Y * (0.2 + rise * 0.5) - radial * 0.18,
                        0.04,
                        texture,
                        color,
                    ),
                    _ => ribbon(
                        glow,
                        p,
                        p - Vec3::new(-a.sin(), -0.1, a.cos()) * 0.65,
                        0.018,
                        texture,
                        color,
                    ),
                }
            }
            if progress > 0.7 {
                let reach = (progress - 0.7) / 0.3 * 7.0;
                ribbon(
                    glow,
                    at + Vec3::Y * 0.1,
                    at + Vec3::Y * reach,
                    if reduced { 0.025 } else { 0.05 },
                    texture,
                    color,
                );
            }
            if m.finished {
                for i in 0..3 {
                    ring(
                        glow,
                        at + Vec3::Y * (0.2 + i as f32 * 0.4),
                        Vec3::X,
                        Vec3::Z,
                        0.5 + elapsed * (2.0 + i as f32),
                        0.025 * fade,
                        texture,
                        color,
                        time,
                    );
                }
            }
        }
    }
    pub(in crate::game) fn arcana_starter_geometry(
        &self,
        camera: DVec3,
        solid: &mut Vec<Vertex>,
        glow: &mut Vec<Vertex>,
    ) {
        let Some(texture) = self
            .gui
            .atlas
            .sprites
            .get("block/white_concrete")
            .or_else(|| self.gui.atlas.sprites.get("block/quartz_block_top"))
            .copied()
        else {
            return;
        };
        let rock = self
            .gui
            .atlas
            .sprites
            .get("block/deepslate")
            .copied()
            .unwrap_or(texture);
        for view in self.arcana.starter_views.values() {
            let m = &view.message;
            let elapsed = view.received.elapsed().as_secs_f32();
            let lifetime = match m.stage {
                VisualStage::Flight => 0.35,
                VisualStage::Preparation => 0.22,
                VisualStage::Ignition => 1.2,
                VisualStage::FirePush => 4.0,
                VisualStage::FireballImpact => 1.8,
                _ => 0.8,
            };
            if elapsed > lifetime || m.dimension != self.dim.id() {
                continue;
            }
            let moving = m.stage == VisualStage::Flight;
            let at = (m.position - camera).as_vec3()
                + if moving {
                    m.velocity.as_vec3() * 20.0 * elapsed.min(0.12)
                } else {
                    Vec3::ZERO
                };
            let dir = m.direction.as_vec3().normalize_or_zero();
            let up = if dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
            let u = dir.cross(up).normalize_or_zero();
            let v = u.cross(dir).normalize_or_zero();
            let time = m.age as f32 / 20.0 + elapsed;
            if fire::draw(m, at, elapsed, time, texture, glow) {
                continue;
            }
            if fireball::draw(m, at, elapsed, time, texture, solid, glow) {
                continue;
            }
            let color = match m.spell_id.as_str() {
                "spark" => rgba(255, 105, 20, 170),
                id if super::catalog()
                    .get(id)
                    .is_some_and(|s| s.school == arcana_core::School::Fire) =>
                {
                    rgba(255, 105, 20, 150)
                }
                "water_bolt" => rgba(60, 180, 245, 165),
                "stone_bolt" => rgba(205, 180, 110, 130),
                _ => rgba(220, 230, 235, 100),
            };
            if m.stage == VisualStage::Preparation {
                let radius = 0.2 + time * 0.5;
                ring(glow, at, u, v, radius, 0.008, texture, color, time * 2.0);
                ring(glow, at, u, v, radius * 0.72, 0.005, texture, color, -time);
                continue;
            }
            if m.stage == VisualStage::Gust {
                let reach = catalog_range(&m.spell_id);
                for step in 0..5 {
                    let q = (elapsed * 1.8 + step as f32 * 0.09).min(1.0);
                    let center = at + dir * (q * reach);
                    ring(
                        glow,
                        center,
                        u,
                        v,
                        0.2 + q * 2.7,
                        0.012 * (1.0 - elapsed / lifetime).max(0.0),
                        texture,
                        color,
                        elapsed * 3.0,
                    );
                }
                continue;
            }
            if m.stage == VisualStage::Impact {
                let alpha = ((1.0 - elapsed / lifetime).max(0.0) * 150.0) as u8;
                let impact = match m.spell_id.as_str() {
                    "spark" => rgba(255, 145, 35, alpha),
                    "water_bolt" => rgba(125, 225, 255, alpha),
                    _ => rgba(205, 185, 140, alpha),
                };
                ring(
                    glow,
                    at,
                    u,
                    v,
                    elapsed * 2.8 + 0.12,
                    0.018,
                    texture,
                    impact,
                    0.0,
                );
                for i in 0..10 {
                    let a = i as f32 / 10.0 * TAU;
                    let direction = u * a.cos() + v * a.sin() + dir * 0.2;
                    let point = at + direction * elapsed * 2.4 - Vec3::Y * elapsed * elapsed;
                    if m.spell_id == "stone_bolt" {
                        cube(
                            solid,
                            point,
                            0.035 * (1.0 - elapsed / lifetime),
                            Quat::from_rotation_y(time + i as f32),
                            rock,
                            rgba(155, 145, 130, 255),
                        );
                    } else {
                        ribbon(glow, point, point - direction * 0.12, 0.02, texture, impact);
                    }
                }
                continue;
            }
            match m.spell_id.as_str() {
                "spark" => {
                    cube(
                        glow,
                        at,
                        0.07,
                        Quat::from_rotation_z(time * 8.0),
                        texture,
                        rgba(255, 220, 125, 230),
                    );
                    for i in 0..6 {
                        let a = time * 10.0 + i as f32 / 6.0 * TAU;
                        let offset = (u * a.cos() + v * a.sin()) * 0.1;
                        ribbon(
                            glow,
                            at + offset,
                            at - dir * (0.35 + i as f32 * 0.08) + offset * 0.35,
                            0.025,
                            texture,
                            color,
                        );
                    }
                }
                "water_bolt" => {
                    for i in 0..5 {
                        let q = i as f32 / 5.0;
                        ring(
                            glow,
                            at - dir * (q * 0.4),
                            u,
                            v,
                            0.15 * (1.0 - q * 0.6),
                            0.018,
                            texture,
                            color,
                            time * 3.0,
                        );
                    }
                    for i in 0..3 {
                        let angle = time * 4.0 + i as f32 / 3.0 * TAU;
                        let offset = (u * angle.cos() + v * angle.sin()) * 0.13;
                        ribbon(
                            glow,
                            at + offset,
                            at - dir * 0.8 + offset * 0.2,
                            0.03,
                            texture,
                            rgba(80, 170, 225, 90),
                        );
                    }
                    ring(
                        glow,
                        at,
                        u,
                        dir,
                        0.12,
                        0.02,
                        texture,
                        rgba(200, 245, 255, 180),
                        time,
                    );
                }
                "stone_bolt" => {
                    cube(
                        solid,
                        at,
                        0.17,
                        Quat::from_euler(glam::EulerRot::XYZ, time * 5.0, time * 3.0, 0.2),
                        rock,
                        rgba(180, 165, 130, 255),
                    );
                    for i in 0..4 {
                        let a = time * 5.0 + i as f32 / 4.0 * TAU;
                        cube(
                            solid,
                            at + (u * a.cos() + v * a.sin()) * 0.25 - dir * 0.16,
                            0.045,
                            Quat::from_rotation_y(a),
                            rock,
                            rgba(160, 145, 115, 255),
                        );
                    }
                    ribbon(glow, at - dir * 0.2, at - dir * 0.6, 0.012, texture, color);
                }
                _ => {}
            }
        }
    }
}
fn catalog_range(id: &str) -> f32 {
    super::catalog().get(id).map_or(8.0, |s| s.range as f32)
}
