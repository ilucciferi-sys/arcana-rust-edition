//! Fire preparation follows public cast clocks and the actual native palms.
use super::*;
use horizon_vfx::{
    Quality,
    preparation::{Anchors, Charge, Family, Mark, Material, View},
};

fn family(id: &str) -> Option<Family> {
    Some(match id {
        "spark" => Family::Spark,
        "fireball" => Family::Orb,
        "ignition" => Family::Touch,
        "fire_push" => Family::Push,
        "flame_wall" => Family::Ground,
        "flame_shield" => Family::Guard,
        "fire_ring" => Family::Ring,
        "fire_blast" => Family::Blast,
        "inferno_beam" => Family::Beam,
        "fire_storm" => Family::Vortex,
        "phoenix_form" => Family::Wings,
        "meteor_rain" => Family::Meteors,
        "solar_spear" => Family::Spear,
        "fire_avatar" => Family::Avatar,
        _ => return None, // Dash retains its original instantaneous, passive action.
    })
}
pub(super) fn is_fire(id: &str) -> bool {
    family(id).is_some()
}
impl Game<'_> {
    fn arcana_charges(
        &self,
        camera: DVec3,
        alpha: f32,
        mut draw: impl FnMut(&horizon_client::mobs::Mob, Charge, Anchors, View),
    ) {
        let quality = match std::env::var("ARCANA_QUALITY").as_deref() {
            Ok("performance") => Quality::Performance,
            Ok("cinematic") => Quality::Cinematic,
            _ => Quality::High,
        };
        let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
        for m in self.player_model.iter().chain(self.mobs.iter()) {
            let own = self
                .player_model
                .as_ref()
                .is_some_and(|own| std::ptr::eq(m, own));
            if &*m.kind.id != "minecraft:player" || !m.alive() {
                continue;
            }
            if own
                && self
                    .player
                    .as_ref()
                    .is_some_and(|p| p.health.dead_or_dying())
            {
                continue;
            }
            let Some(play) = m.extra.cast_animation.as_ref().filter(|p| p.alive()) else {
                continue;
            };
            let Some(family) = family(&play.event.spell_id) else {
                continue;
            };
            let Some(spell) = super::super::catalog().get(&play.event.spell_id) else {
                continue;
            };
            let Some(charge) = Charge::sample(
                family,
                spell.level,
                play.age(),
                spell.cast_time as f32,
                play.fade(),
            ) else {
                continue;
            };
            let position = DVec3::new(
                m.previous.x + (m.position.x - m.previous.x) * alpha as f64,
                m.previous.y + (m.position.y - m.previous.y) * alpha as f64,
                m.previous.z + (m.position.z - m.previous.z) * alpha as f64,
            );
            if position.distance_squared(camera) > 96.0 * 96.0 {
                continue;
            }
            let palm = |left| {
                horizon_client::mobs::render::player_palm(
                    m,
                    alpha,
                    camera,
                    self.res.mob_data(),
                    left,
                )
            };
            let Some(hand) = palm(m.main_arm_left()) else {
                continue;
            };
            let Some(other) = palm(!m.main_arm_left()) else {
                continue;
            };
            let yaw = m.previous_head_yaw
                + horizon_client::mobs::living::wrap(m.head_yaw - m.previous_head_yaw) * alpha;
            let pitch = m.previous_head_pitch + (m.head_pitch - m.previous_head_pitch) * alpha;
            let (sy, cy) = yaw.to_radians().sin_cos();
            let (sp, cp) = pitch.to_radians().sin_cos();
            let anchors = Anchors {
                hand,
                other,
                feet: (position - camera).as_vec3(),
                forward: Vec3::new(-sy * cp, -sp, cy * cp),
            };
            draw(
                m,
                charge,
                anchors,
                View {
                    quality,
                    reduced,
                    first_person: own && self.camera_type.first_person(),
                },
            );
        }
    }
    pub(in crate::game) fn arcana_charge_geometry(
        &self,
        camera: DVec3,
        alpha: f32,
        glow: &mut Vec<Vertex>,
    ) {
        let atlas = &self.gui.atlas.sprites;
        let Some(white) = atlas.get("block/white_concrete").copied() else {
            return;
        };
        self.arcana_charges(camera, alpha, |_, charge, anchors, view| {
            charge.draw(anchors, view, |mark| match mark {
                Mark::Line { a, b, width, color } => ribbon(
                    glow,
                    a,
                    b,
                    width,
                    white,
                    rgba(color[0], color[1], color[2], color[3]),
                ),
                Mark::Plane {
                    at,
                    x,
                    y,
                    material,
                    color,
                } => {
                    let name = match material {
                        Material::Sigil => "arcana_four_elements:effect/sigil_fire",
                        Material::Halo => "arcana_four_elements:effect/halo",
                        Material::Flame(frame) => [
                            "arcana_four_elements:effect/flame_0",
                            "arcana_four_elements:effect/flame_1",
                            "arcana_four_elements:effect/flame_2",
                            "arcana_four_elements:effect/flame_3",
                            "arcana_four_elements:effect/flame_4",
                            "arcana_four_elements:effect/flame_5",
                            "arcana_four_elements:effect/flame_6",
                            "arcana_four_elements:effect/flame_7",
                        ][frame as usize % 8],
                    };
                    let texture = atlas.get(name);
                    if let Some(texture) = texture {
                        quad(
                            glow,
                            horizon_vfx::preparation::camera_facing_quad(at, x, y),
                            *texture,
                            rgba(color[0], color[1], color[2], color[3]),
                        );
                    }
                }
            });
        });
    }
    pub(in super::super) fn arcana_charge_debug(&self) -> Vec<serde_json::Value> {
        let camera = DVec3::from_array(self.eye_position().unwrap_or_default());
        let mut result = vec![];
        let textures_ready = ["sigil_fire", "halo", "flame_0", "flame_7"]
            .iter()
            .all(|n| {
                self.gui
                    .atlas
                    .sprites
                    .contains_key(&format!("arcana_four_elements:effect/{n}"))
            });
        self.arcana_charges(camera,1.0,|m,charge,anchors,view| {
            let mut planes=0;
            let count=charge.draw(anchors,view,|mark| if matches!(mark,Mark::Plane {..}) {planes+=1;});
            if count==0 {return;}
            result.push(serde_json::json!({"entity":m.id,"own":self.player_model.as_ref().is_some_and(|own|std::ptr::eq(m,own)),
                "spell":m.extra.cast_animation.as_ref().unwrap().event.spell_id,"family":format!("{:?}",charge.family),
                "progress":charge.progress,"opacity":charge.opacity,"marks":count,"planes":planes,
                "palm":camera+anchors.hand.as_dvec3(),"feet":camera+anchors.feet.as_dvec3(),
                "first_person":view.first_person,"reduced":view.reduced,
                "textures_ready":textures_ready}));
        });
        result
    }
}
