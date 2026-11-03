//! Native adapter for rain, falling meteors, their contacts and the Fire vortex.
use super::*;
use horizon_vfx::{
    Quality,
    preparation::Material,
    tempest::{self, Mark, Stats, Style},
};

const FRAME_BUDGET: usize = 8192;
#[derive(Clone, Copy)]
enum Scene {
    Meteor {
        at: Vec3,
        direction: Vec3,
        time: f32,
    },
    Impact {
        at: Vec3,
        time: f32,
    },
    Vortex {
        at: Vec3,
        time: f32,
        remaining: f32,
    },
    Canopy {
        at: Vec3,
        time: f32,
        remaining: f32,
    },
}
impl Scene {
    fn draw(self, id: u64, style: Style, emit: impl FnMut(Mark)) -> Stats {
        match self {
            Self::Meteor {
                at,
                direction,
                time,
            } => tempest::meteor(at, direction, time, id, style, emit),
            Self::Impact { at, time } => tempest::impact(at, time, id, style, emit),
            Self::Vortex {
                at,
                time,
                remaining,
            } => tempest::vortex(at, time, remaining, id, style, emit),
            Self::Canopy {
                at,
                time,
                remaining,
            } => tempest::canopy(at, time, remaining, id, style, emit),
        }
    }
}
impl Game<'_> {
    fn arcana_tempests(&self, camera: DVec3, mut visit: impl FnMut(&str, u64, &str, Scene, Style)) {
        let quality = match std::env::var("ARCANA_QUALITY").as_deref() {
            Ok("performance") => Quality::Performance,
            Ok("cinematic") => Quality::Cinematic,
            _ => Quality::High,
        };
        let reduced = std::env::var_os("ARCANA_REDUCED_EFFECTS").is_some();
        // Contacts first: under load a new impact takes priority over a sky seal.
        // Sort by public IDs: HashMap iteration must not select different effects
        // for rendering and diagnostics when the frame budget is exhausted.
        let mut bolts: Vec<_> = self
            .arcana
            .starter_views
            .values()
            .filter(|v| v.message.spell_id == "meteor_rain")
            .collect();
        bolts.sort_unstable_by_key(|v| {
            (
                v.message.stage != VisualStage::FireballImpact,
                v.message.owner,
                v.message.cast_id,
            )
        });
        for v in bolts {
            let m = &v.message;
            let elapsed = v.received.elapsed().as_secs_f32();
            if m.dimension != self.dim.id() {
                continue;
            }
            let scene = match m.stage {
                VisualStage::Flight if elapsed < 0.35 => Scene::Meteor {
                    at: (m.position - camera).as_vec3()
                        + m.velocity.as_vec3() * 20.0 * elapsed.min(0.12),
                    direction: m.direction.as_vec3(),
                    time: m.age as f32 / 20.0 + elapsed,
                },
                VisualStage::FireballImpact if elapsed < 1.8 => Scene::Impact {
                    at: (m.position - camera).as_vec3(),
                    time: elapsed,
                },
                _ => continue,
            };
            let distance = m.position.distance(camera) as f32;
            if distance > 180.0 {
                continue;
            }
            visit(
                "meteor_rain",
                m.cast_id,
                if m.stage == VisualStage::Flight {
                    "flight"
                } else {
                    "impact"
                },
                scene,
                Style {
                    quality,
                    reduced,
                    distance,
                },
            );
        }
        let mut fields: Vec<_> = self
            .arcana
            .school
            .views
            .values()
            .filter(|v| {
                matches!(
                    v.visual.effect.spell_id.as_str(),
                    "fire_storm" | "meteor_rain"
                )
            })
            .collect();
        fields.sort_unstable_by_key(|v| {
            (
                v.visual.effect.spell_id == "meteor_rain",
                v.visual.owner,
                v.visual.id,
            )
        });
        for v in fields {
            let e = &v.visual.effect;
            let elapsed = v.received.elapsed().as_secs_f32();
            if v.visual.dimension != self.dim.id() || elapsed > 1.0 {
                continue;
            }
            let distance = e.position.distance(camera) as f32;
            if distance > 180.0 {
                continue;
            }
            let time = e.age as f32 / 20.0 + elapsed;
            let remaining = e.life as f32 / 20.0 - time;
            let at = (e.position - camera).as_vec3();
            let (stage, scene) = if e.spell_id == "fire_storm" {
                (
                    "vortex",
                    Scene::Vortex {
                        at: at + e.direction.as_vec3() * 4.0 * elapsed.min(0.15),
                        time,
                        remaining,
                    },
                )
            } else {
                (
                    "canopy",
                    Scene::Canopy {
                        at,
                        time,
                        remaining,
                    },
                )
            };
            visit(
                &e.spell_id,
                v.visual.id,
                stage,
                scene,
                Style {
                    quality,
                    reduced,
                    distance,
                },
            );
        }
    }
    pub(in crate::game) fn arcana_tempest_geometry(
        &self,
        camera: DVec3,
        solid: &mut Vec<Vertex>,
        glow: &mut Vec<Vertex>,
    ) {
        let atlas = &self.gui.atlas.sprites;
        let Some(white) = atlas.get("block/white_concrete").copied() else {
            return;
        };
        let rock = atlas.get("block/deepslate").copied().unwrap_or(white);
        let mut used = 0;
        self.arcana_tempests(camera, |_, id, _, scene, style| {
            if used >= FRAME_BUDGET {
                return;
            }
            scene.draw(id, style, |mark| {
                if used >= FRAME_BUDGET {
                    return;
                }
                used += 1;
                match mark {
                    Mark::Line { a, b, width, color } => {
                        let start = solid.len();
                        ribbon(
                            solid,
                            a,
                            b,
                            width,
                            white,
                            rgba(color[0], color[1], color[2], color[3].saturating_mul(2)),
                        );
                        translucent(&mut solid[start..]);
                        ribbon(
                            glow,
                            a,
                            b,
                            width * 1.2,
                            white,
                            rgba(color[0], color[1], color[2], color[3] / 4),
                        );
                    }
                    Mark::Plane {
                        at,
                        x,
                        y,
                        material,
                        color,
                    } => {
                        if let Some(texture) = atlas.get(effect_name(material)) {
                            // Both existing passes are double-sided. Keep UVs
                            // attached to the axes: reversing winding toward
                            // the camera would flip the flame vertically as a
                            // rotating sheet crosses the viewer.
                            let points = [at - x - y, at + x - y, at + x + y, at - x + y];
                            let flame = matches!(material, Material::Flame(_));
                            if flame {
                                let start = solid.len();
                                quad(
                                    solid,
                                    points,
                                    *texture,
                                    rgba(color[0], color[1], color[2], color[3].saturating_mul(3)),
                                );
                                translucent(&mut solid[start..]);
                            }
                            quad(
                                glow,
                                points,
                                *texture,
                                rgba(
                                    color[0],
                                    color[1],
                                    color[2],
                                    if flame { color[3] / 4 } else { color[3] },
                                ),
                            );
                        }
                    }
                    Mark::Rock {
                        at,
                        size,
                        rotation,
                        color,
                    } => cube(
                        solid,
                        at,
                        size,
                        rotation,
                        rock,
                        rgba(color[0], color[1], color[2], color[3]),
                    ),
                }
            });
        });
    }
    pub(in super::super) fn arcana_tempest_debug(&self) -> Vec<serde_json::Value> {
        let camera = DVec3::from_array(self.eye_position().unwrap_or_default());
        let mut result = vec![];
        let mut used = 0;
        self.arcana_tempests(camera,|spell,id,stage,scene,style| {
            if used>=FRAME_BUDGET {return;}
            let mut stats=Stats::default();
            scene.draw(id,style,|mark| {
                if used>=FRAME_BUDGET {return;}
                used+=1;
                match mark {Mark::Line{..}=>stats.lines+=1,Mark::Plane{..}=>stats.planes+=1,Mark::Rock{..}=>stats.rocks+=1}
            });
            let (at,time)=match scene {Scene::Meteor{at,time,..}|Scene::Impact{at,time}|Scene::Vortex{at,time,..}|Scene::Canopy{at,time,..}=>(at,time)};
            if stats.total()>0 {result.push(serde_json::json!({"spell":spell,"id":id,"stage":stage,"position":camera+at.as_dvec3(),"age_seconds":time,"marks":stats.total(),"planes":stats.planes,"rocks":stats.rocks,"budget":style.budget(),"frame_budget":FRAME_BUDGET,"reduced":style.reduced,"distance":style.distance}));}
        });
        result
    }
}

fn translucent(vertices: &mut [Vertex]) {
    for v in vertices {
        v.layer = horizon_client::gfx::TRANSLUCENT_ITEM_OVERLAY;
    }
}

pub(super) fn effect_name(material: Material) -> &'static str {
    match material {
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
    }
}
