//! Server-owned forms attached to native player rigs, including late observers.
use super::*;
use horizon_client::mobs::{self, Mob, fire_form::View};
impl Game<'_> {
    pub(in crate::game) fn arcana_form_arm(
        &self,
        left: bool,
        base: glam::Mat4,
        out: &mut Vec<horizon_client::mesher::Vertex>,
    ) -> bool {
        let Some(m) = self.player_model.as_ref() else {
            return false;
        };
        let Some(form) = mobs::fire_form::active(m) else {
            return false;
        };
        let Some(arm) = self.res.player_arm(m.slim, left).as_ref() else {
            return false;
        };
        let Some(body) = self.gui.atlas.sprites.get(&form.texture()) else {
            return false;
        };
        let Some(glow) = self
            .gui
            .atlas
            .sprites
            .get("arcana_four_elements:entity/body_fire_glow")
        else {
            return false;
        };
        let pose = |p: &mobs::Part| {
            let mut pose = p.pose;
            pose[3] = 0.0;
            pose[4] = 0.0;
            pose[5] = if p.name == arm.name {
                if left { -0.1 } else { 0.1 }
            } else {
                0.0
            };
            pose
        };
        for (uv, color) in [body, glow].into_iter().zip(form.colors()) {
            let start = out.len();
            mobs::render_model(
                arm,
                base,
                &pose,
                [uv.u0, uv.v0, uv.u1, uv.v1],
                0xf0f0,
                0,
                out,
            );
            for vertex in &mut out[start..] {
                vertex.color = color;
                vertex.normal = [0; 4];
            }
        }
        true
    }
    pub(super) fn arcana_body_views(&self) -> HashMap<[i32; 4], View> {
        let mut forms: HashMap<[i32; 4], View> = HashMap::new();
        for viewed in self.arcana.school.views.values() {
            let v = &viewed.visual;
            if v.dimension != self.dim.id() || v.expired {
                continue;
            }
            let Some(body) = &v.body else {
                continue;
            };
            let Some(kind) =
                arcana_core::forms::Kind::from_spell(&v.effect.spell_id, v.effect.stage)
            else {
                continue;
            };
            let form = View {
                kind,
                age: v.effect.age,
                life: v.effect.life,
                born: v.effect.born,
                stage: v.effect.stage,
                flying: body.flying,
                direction: v.effect.direction,
                received: viewed.received,
            };
            if form.alive()
                && forms
                    .get(&body.uuid)
                    .is_none_or(|old| old.born <= form.born)
            {
                forms.insert(body.uuid, form);
            }
        }
        forms
    }
    pub(super) fn arcana_form_model(&self, uuid: [i32; 4]) -> Option<&Mob> {
        if self
            .remote
            .as_ref()
            .is_some_and(|r| owner_words(r.profile()) == uuid)
        {
            return self.player_model.as_ref();
        }
        self.mobs.iter().find(|m| {
            &*m.kind.id == "minecraft:player" && self.persist.uuids.get(&m.id) == Some(&uuid)
        })
    }
    pub(super) fn arcana_form_debug(&self) -> Vec<serde_json::Value> {
        self.player_model.iter().chain(self.mobs.iter()).filter_map(|m| {
            let form=mobs::fire_form::active(m)?;
            let (key,_)=mobs::render::body(m);
            let model=m.kind.layer(&key).unwrap_or(&m.kind.model);
            let mut poses=mobs::anim::Poses::new(model);
            mobs::anim::setup(m,&mobs::anim::state(m,1.0),&mut poses,&self.res.mob_data().animations);
            let bones:HashMap<_,_>=["head","body","right_arm","left_arm","right_leg","left_leg"].into_iter().map(|name|(name,*poses.get(name))).collect();
            let texture=form.texture();
            Some(serde_json::json!({"entity":m.id,"own":self.player_model.as_ref().is_some_and(|own|std::ptr::eq(own,m)),
                "kind":format!("{:?}",form.kind),"stage":form.stage,"flying":form.flying,"age":form.age(),"opacity":form.opacity(),
                "texture":texture,"material_ready":self.gui.atlas.sprites.contains_key(&texture),"bones":bones}))
        }).collect()
    }
}
