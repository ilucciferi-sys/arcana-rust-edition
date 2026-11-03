//! Stateless, bounded falling-rock, impact and vortex compositions. The host
//! supplies camera-relative positions and authoritative clocks; no collision,
//! damage, random spawning or gameplay state lives here.
use crate::{Quality, preparation::Material};
use glam::{Quat, Vec3};
use std::f32::consts::TAU;

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
    Rock {
        at: Vec3,
        size: f32,
        rotation: Quat,
        color: [u8; 4],
    },
}
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub quality: Quality,
    pub reduced: bool,
    pub distance: f32,
}
impl Style {
    fn tier(self) -> usize {
        if self.reduced || self.distance > 70.0 {
            return 0;
        }
        match self.quality {
            Quality::Performance => 0,
            Quality::High => 1,
            Quality::Cinematic if self.distance <= 35.0 => 2,
            Quality::Cinematic => 1,
        }
    }
    pub fn budget(self) -> usize {
        [192, 384, 640][self.tier()]
    }
    fn visible(self) -> bool {
        self.distance.is_finite() && (0.0..=180.0).contains(&self.distance)
    }
}
#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub lines: usize,
    pub planes: usize,
    pub rocks: usize,
}
impl Stats {
    pub fn total(self) -> usize {
        self.lines + self.planes + self.rocks
    }
}
struct Paint<F> {
    emit: F,
    style: Style,
    stats: Stats,
    opacity: f32,
}
impl<F: FnMut(Mark)> Paint<F> {
    fn mark(&mut self, mark: Mark) {
        if self.stats.total() >= self.style.budget() {
            return;
        }
        match mark {
            Mark::Line { .. } => self.stats.lines += 1,
            Mark::Plane { .. } => self.stats.planes += 1,
            Mark::Rock { .. } => self.stats.rocks += 1,
        }
        (self.emit)(mark);
    }
    fn color(&self, hot: bool, alpha: f32) -> [u8; 4] {
        let a = (alpha * self.opacity * if self.style.reduced { 0.38 } else { 1.0 })
            .clamp(0.0, 255.0) as u8;
        if hot {
            [255, 195, 70, a]
        } else {
            [255, 74, 8, a]
        }
    }
    fn line(&mut self, a: Vec3, b: Vec3, width: f32, hot: bool, alpha: f32) {
        self.mark(Mark::Line {
            a,
            b,
            width,
            color: self.color(hot, alpha),
        });
    }
    fn plane(&mut self, at: Vec3, x: Vec3, y: Vec3, material: Material, alpha: f32) {
        self.mark(Mark::Plane {
            at,
            x,
            y,
            material,
            color: self.color(false, alpha),
        });
    }
    fn ring(&mut self, at: Vec3, radius: f32, width: f32, alpha: f32, phase: f32) {
        let n = [20, 32, 40][self.style.tier()];
        for i in 0..n {
            let a = phase + i as f32 / n as f32 * TAU;
            let b = phase + (i + 1) as f32 / n as f32 * TAU;
            self.line(
                at + radial(a) * radius,
                at + radial(b) * radius,
                width,
                false,
                alpha,
            );
        }
    }
}
fn radial(a: f32) -> Vec3 {
    Vec3::new(a.cos(), 0.0, a.sin())
}
fn frame(time: f32, seed: u64) -> Material {
    Material::Flame(((time * 13.0) as u64 + seed % 8) as u8 % 8)
}
fn seed_phase(seed: u64) -> f32 {
    (seed % 4093) as f32 * 0.173
}
fn valid(at: Vec3, time: f32, style: Style) -> bool {
    at.is_finite() && time.is_finite() && time >= 0.0 && style.visible()
}
fn paint<F: FnMut(Mark)>(style: Style, opacity: f32, emit: F) -> Paint<F> {
    Paint {
        emit,
        style,
        stats: Stats::default(),
        opacity,
    }
}

/// Irregular dark rock, molten seams and an animated tapered wake. Its centre
/// follows the host's projectile; the wake never creates extra projectiles.
pub fn meteor(
    at: Vec3,
    direction: Vec3,
    time: f32,
    seed: u64,
    style: Style,
    emit: impl FnMut(Mark),
) -> Stats {
    if !valid(at, time, style) || !direction.is_finite() {
        return Stats::default();
    }
    let d = direction.try_normalize().unwrap_or(-Vec3::Y);
    let u = d
        .cross(if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y })
        .normalize();
    let v = u.cross(d);
    let phase = seed_phase(seed);
    let mut p = paint(style, 1.0, emit);
    p.mark(Mark::Rock {
        at,
        size: 0.49,
        rotation: Quat::from_euler(glam::EulerRot::XYZ, time * 1.7 + phase, time * 2.1, phase),
        color: [94, 70, 60, 255],
    });
    for i in 0..[3, 5, 7][style.tier()] {
        let a = phase + i as f32 * 2.39996 + time * 0.8;
        let r = u * a.cos() + v * a.sin();
        p.mark(Mark::Rock {
            at: at + r * 0.38 - d * 0.12,
            size: 0.23,
            rotation: Quat::from_rotation_y(a),
            color: [110, 78, 62, 255],
        });
        p.line(
            at + r * 0.53 + d * 0.2,
            at + r * 0.55 - d * 0.42,
            0.022,
            true,
            155.0,
        );
    }
    // Crossed texture sheets, offset frames and a cooler outer wake preserve
    // the rock silhouette instead of replacing it with a glowing wire sphere.
    let n = [4, 7, 10][style.tier()];
    for i in 0..n {
        let q = i as f32 / n as f32;
        let a = phase + time * 1.8 + q * 4.0;
        let r = u * a.cos() + v * a.sin();
        let center = at - d * (0.5 + q * 6.5) + r * (q * 0.25);
        let width = 0.6 * (1.0 - q * 0.7);
        for axis in [r, d.cross(r)] {
            p.plane(
                center,
                axis * width,
                -d * (0.85 + q * 0.4),
                frame(time - q * 0.12, seed + i as u64),
                95.0 * (1.0 - q),
            );
        }
        p.line(
            center,
            center - d * (0.8 + q),
            0.04 * (1.0 - q),
            i % 3 == 0,
            115.0 * (1.0 - q),
        );
    }
    for i in 0..[8, 18, 28][style.tier()] {
        let q = (time * 0.8 + i as f32 * 0.618).fract();
        let a = phase + i as f32 * 2.4;
        let r = u * a.cos() + v * a.sin();
        let at = at - d * (0.7 + q * 7.0) + r * (0.35 + q * 0.9);
        p.line(at, at - d * 0.25, 0.013, i % 4 == 0, 120.0 * (1.0 - q));
    }
    p.stats
}

/// One contact: brief flash, a four-metre wave, ballistic rubble and cooling
/// embers. The host calls this only for an authoritative impact message.
pub fn impact(at: Vec3, time: f32, seed: u64, style: Style, emit: impl FnMut(Mark)) -> Stats {
    if !valid(at, time, style) || time >= 1.8 {
        return Stats::default();
    }
    let fade = (1.0 - time / 1.8).max(0.0);
    let mut p = paint(style, fade, emit);
    let phase = seed_phase(seed);
    if time < 0.65 {
        let r = 0.15 + (time / 0.65).sqrt() * 3.85;
        p.ring(
            at + Vec3::Y * 0.12,
            r,
            0.045,
            120.0 * (1.0 - time / 0.65),
            phase,
        );
        if style.tier() > 0 {
            p.ring(at + Vec3::Y * 0.15, r * 0.82, 0.018, 75.0, phase);
        }
    }
    if time < 0.22 {
        p.plane(
            at + Vec3::Y * 0.15,
            Vec3::X * (0.8 + time * 6.0),
            Vec3::Z * (0.8 + time * 6.0),
            Material::Halo,
            150.0 * (1.0 - time / 0.22),
        );
    }
    // Several flame tongues leave the centre open between them.
    for i in 0..[5, 9, 13][style.tier()] {
        let a = phase + i as f32 * 2.39996;
        let r = radial(a);
        let center = at + r * (0.2 + time * 1.8) + Vec3::Y * (0.6 + time * 0.4);
        p.plane(
            center,
            Vec3::new(-r.z, 0.0, r.x) * 0.45 * fade,
            Vec3::Y * (0.8 + time) * fade,
            frame(time, seed + i as u64),
            120.0 * fade,
        );
    }
    for i in 0..[12, 24, 40][style.tier()] {
        let a = phase + i as f32 * 2.39996;
        let dir = radial(a);
        let speed = 1.6 + (i % 5) as f32 * 0.5;
        let height = (time * (2.5 + (i % 4) as f32 * 0.5) - 2.8 * time * time).max(0.08);
        let pos = at + dir * speed * time + Vec3::Y * height;
        if i % 3 == 0 {
            p.mark(Mark::Rock {
                at: pos,
                size: 0.09 * fade,
                rotation: Quat::from_euler(glam::EulerRot::XYZ, a + time * 4.0, a, time * 3.0),
                color: [100, 71, 55, 255],
            });
        }
        p.line(
            pos,
            pos - dir * (0.13 + fade * 0.22) + Vec3::Y * 0.08,
            0.018 * fade,
            i % 4 == 0,
            140.0 * fade,
        );
    }
    p.stats
}

fn vortex_point(at: Vec3, q: f32, angle: f32, age: f32, scale: f32) -> Vec3 {
    // Same centre-line bend and 14m height as the authoritative hit volume.
    let bend = Vec3::new(
        (age * 0.025 + q * 3.8).sin() * 0.98 * q,
        0.0,
        (age * 0.021 + q * 4.5).cos() * 0.77 * q,
    );
    at + bend + radial(angle) * (0.65 + 7.0 * q.powf(1.2)) * scale + Vec3::Y * q * 14.0
}
pub fn vortex(
    at: Vec3,
    time: f32,
    remaining: f32,
    seed: u64,
    style: Style,
    emit: impl FnMut(Mark),
) -> Stats {
    if !valid(at, time, style) || !remaining.is_finite() || remaining <= 0.0 {
        return Stats::default();
    }
    let fade = (time / 0.4).min(1.0) * (remaining / 0.8).min(1.0);
    let mut p = paint(style, fade, emit);
    let phase = seed_phase(seed);
    let arms = [3, 5, 7][style.tier()];
    let levels = [16, 24, 32][style.tier()];
    // Continuous broad tongues alternate with fine hot filaments; two radii
    // give the cone depth without an opaque wall around targets.
    for arm in 0..arms {
        for level in 0..levels {
            let q = level as f32 / levels as f32;
            let next = (level + 1) as f32 / levels as f32;
            let a = q * TAU * 1.5 - time * 4.3 + phase + arm as f32 * TAU / arms as f32;
            let b = next * TAU * 1.5 - time * 4.3 + phase + arm as f32 * TAU / arms as f32;
            let scale = if arm % 2 == 0 { 1.0 } else { 0.78 };
            let x = vortex_point(at, q, a, time * 20.0, scale);
            let y = vortex_point(at, next, b, time * 20.0, scale);
            p.line(
                x,
                y,
                0.055 + q * 0.045,
                arm % 3 == 0,
                85.0 * (1.0 - q * 0.45),
            );
            if level % 2 == 0 {
                let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
                p.plane(
                    x,
                    tangent * (0.42 + q * 0.95),
                    Vec3::Y * (0.55 + q * 0.8),
                    frame(time + q, seed + arm as u64),
                    90.0 * (1.0 - q * 0.35),
                );
            }
        }
    }
    p.ring(at + Vec3::Y * 0.12, 2.0, 0.028, 90.0, -time);
    // Spiral motes rise; debris is explicitly decorative, never world blocks.
    for i in 0..[16, 36, 64][style.tier()] {
        let q = (time * 0.24 + i as f32 * 0.618).fract();
        let a = phase + i as f32 * 2.39996 - time * (3.0 + q);
        let x = vortex_point(at, q, a, time * 20.0, 0.9);
        let y = vortex_point(at, (q + 0.018).min(1.0), a - 0.13, time * 20.0, 0.9);
        p.line(x, y, 0.016, i % 4 == 0, 115.0 * (1.0 - q * 0.6));
        if i % 6 == 0 && q < 0.5 {
            p.mark(Mark::Rock {
                at: x,
                size: 0.07 + q * 0.12,
                rotation: Quat::from_rotation_y(a),
                color: [96, 70, 54, 255],
            });
        }
    }
    p.stats
}

/// Sky aperture stays anchored to the server's rain centre. It does not predict
/// landing points or invent extra falling rocks.
pub fn canopy(
    at: Vec3,
    time: f32,
    remaining: f32,
    seed: u64,
    style: Style,
    emit: impl FnMut(Mark),
) -> Stats {
    if !valid(at, time, style) || !remaining.is_finite() || remaining <= 0.0 {
        return Stats::default();
    }
    let mut p = paint(
        style,
        (time / 0.6).min(1.0) * (remaining / 0.8).min(1.0),
        emit,
    );
    let center = at + Vec3::Y * 14.0;
    let phase = seed_phase(seed) + time * 0.08;
    let x = radial(phase) * 18.0;
    let z = radial(phase + TAU * 0.25) * 18.0;
    p.plane(center, x, z, Material::Sigil, 42.0);
    for scale in [0.32, 0.65, 1.0] {
        p.ring(center, 56.0 * scale, 0.045, 45.0, phase);
    }
    for i in 0..[12, 24, 36][style.tier()] {
        let a = phase + i as f32 * TAU / [12, 24, 36][style.tier()] as f32;
        let r = radial(a);
        p.line(center + r * 53.0, center + r * 56.0, 0.06, i % 3 == 0, 80.0);
        if i % 2 == 0 {
            p.plane(
                center + r * 36.4,
                Vec3::new(-r.z, 0.0, r.x) * 0.6,
                Vec3::Y * 0.9,
                frame(time, seed + i as u64),
                85.0,
            );
        }
    }
    p.stats
}

#[cfg(test)]
mod tests {
    use super::*;
    fn style(quality: Quality) -> Style {
        Style {
            quality,
            reduced: false,
            distance: 20.0,
        }
    }
    fn finite(mark: Mark) {
        match mark {
            Mark::Line { a, b, width, .. } => {
                assert!(a.is_finite() && b.is_finite() && width.is_finite() && width >= 0.0)
            }
            Mark::Plane { at, x, y, .. } => {
                assert!(at.is_finite() && x.is_finite() && y.is_finite())
            }
            Mark::Rock {
                at, size, rotation, ..
            } => assert!(at.is_finite() && size > 0.0 && rotation.is_finite()),
        }
    }
    #[test]
    fn bounded_finite_through_all_phases_and_profiles() {
        for quality in [Quality::Performance, Quality::High, Quality::Cinematic] {
            let s = style(quality);
            for i in 0..180 {
                let t = i as f32 / 100.0;
                for stats in [
                    meteor(Vec3::Z * 10.0, -Vec3::Y, t, 42, s, finite),
                    impact(Vec3::Z * 10.0, t, 42, s, finite),
                    vortex(Vec3::Z * 10.0, t, 30.0 - t, 42, s, finite),
                    canopy(Vec3::Z * 10.0, t, 18.0 - t, 42, s, finite),
                ] {
                    assert!(stats.total() > 0 && stats.total() <= s.budget());
                }
            }
        }
    }
    #[test]
    fn cull_invalid_and_ended_without_geometry() {
        for s in [
            Style {
                distance: 181.0,
                ..style(Quality::High)
            },
            Style {
                distance: f32::NAN,
                ..style(Quality::High)
            },
        ] {
            assert_eq!(
                meteor(Vec3::ZERO, -Vec3::Y, 1.0, 0, s, |_| panic!()).total(),
                0
            );
        }
        assert_eq!(
            impact(Vec3::ZERO, 1.8, 0, style(Quality::High), |_| panic!()).total(),
            0
        );
        assert_eq!(
            vortex(Vec3::ZERO, 1.0, 0.0, 0, style(Quality::High), |_| panic!()).total(),
            0
        );
        assert_eq!(
            canopy(Vec3::NAN, 1.0, 2.0, 0, style(Quality::High), |_| panic!()).total(),
            0
        );
    }
    #[test]
    fn quality_reduces_work_and_flashes() {
        let high = style(Quality::High);
        let reduced = Style {
            reduced: true,
            ..high
        };
        let mut peak = 0;
        let a = vortex(Vec3::ZERO, 2.0, 20.0, 9, high, |_| {});
        let b = vortex(Vec3::ZERO, 2.0, 20.0, 9, reduced, |mark| {
            let color = match mark {
                Mark::Line { color, .. } | Mark::Plane { color, .. } | Mark::Rock { color, .. } => {
                    color
                }
            };
            if !matches!(mark, Mark::Rock { .. }) {
                peak = peak.max(color[3]);
            }
        });
        assert!(b.total() < a.total());
        assert!(peak <= 44);
        let far = Style {
            distance: 90.0,
            ..high
        };
        assert_eq!(
            vortex(Vec3::ZERO, 2.0, 20.0, 9, far, |_| {}).total(),
            b.total()
        );
    }
    #[test]
    fn meteor_wake_is_behind_server_projectile_and_impact_wave_is_bounded() {
        meteor(
            Vec3::ZERO,
            -Vec3::Y,
            1.0,
            6,
            style(Quality::Cinematic),
            |mark| {
                if let Mark::Plane { at, .. } = mark {
                    assert!(at.y >= 0.0);
                }
            },
        );
        impact(Vec3::ZERO, 0.3, 6, style(Quality::High), |mark| {
            if let Mark::Line { a, b, .. } = mark {
                assert!(a.length() < 5.0 && b.length() < 5.0);
            }
        });
    }
}
