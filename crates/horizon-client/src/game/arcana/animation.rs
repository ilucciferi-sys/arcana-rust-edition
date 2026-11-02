//! Client timeline follows public server events; UUIDs survive entity respawns.
use super::*;
use horizon_client::mobs::{self, cast_animation::Playback};

impl Game<'_> {
    pub(in crate::game) fn arcana_forget_pose(&mut self, uuid: Option<[i32; 4]>) {
        if let Some(uuid) = uuid {
            self.arcana.poses.remove(&uuid);
            if self
                .remote
                .as_ref()
                .is_some_and(|r| owner_words(r.profile()) == uuid)
                && let Some(m) = self.player_model.as_mut()
            {
                m.extra.cast_animation = None;
            }
        }
    }
    pub(in crate::game) fn arcana_respawn_pose(&mut self) {
        self.arcana_forget_pose(self.remote.as_ref().map(|r| owner_words(r.profile())));
    }
    pub(super) fn arcana_pose_event(
        &mut self,
        owner: Option<[i32; 4]>,
        cast: &CastEvent,
        body_yaw: Option<f32>,
    ) {
        let Some(owner) = owner else {
            return;
        };
        if catalog().get(&cast.spell_id).is_none() {
            return;
        }
        if !self.arcana.pose_history.accept(owner, cast) {
            return;
        }
        if let Some(pose) = self.arcana.poses.get_mut(&owner) {
            if pose.update(cast.clone()) {
                pose.body_yaw = body_yaw;
            }
        } else if !matches!(cast.phase, Phase::Cancelled | Phase::Finished)
            && self.arcana.poses.len() < 256
        {
            let mut pose = Playback::new(cast.clone());
            pose.body_yaw = body_yaw;
            self.arcana.poses.insert(owner, pose);
        }
    }
    pub(super) fn arcana_pose_tick(&mut self) {
        if self.arcana.pose_dimension != self.dim.id() {
            self.arcana.poses.clear();
            self.arcana.pose_dimension = self.dim.id().into();
        }
        self.arcana.poses.retain(|_, p| p.alive());
        let forms = self.arcana_body_views();
        if let Some(m) = self.player_model.as_mut() {
            m.extra.cast_animation = self
                .remote
                .as_ref()
                .and_then(|r| self.arcana.poses.get(&owner_words(r.profile())))
                .cloned();
            m.extra.fire_form = self
                .remote
                .as_ref()
                .and_then(|r| forms.get(&owner_words(r.profile())))
                .cloned();
        }
        for m in &mut self.mobs {
            if &*m.kind.id == "minecraft:player" {
                m.extra.cast_animation = self
                    .persist
                    .uuids
                    .get(&m.id)
                    .and_then(|uuid| self.arcana.poses.get(uuid))
                    .cloned();
                m.extra.fire_form = self
                    .persist
                    .uuids
                    .get(&m.id)
                    .and_then(|uuid| forms.get(uuid))
                    .cloned();
            }
        }
    }
    pub(super) fn arcana_pose_debug(&self) -> Vec<serde_json::Value> {
        self.player_model.iter().chain(self.mobs.iter()).filter_map(|m| {
            let playback = m.extra.cast_animation.as_ref().filter(|p| p.alive())?;
            let (key,_) = mobs::render::body(m);
            let model = m.kind.layer(&key).unwrap_or(&m.kind.model);
            let mut poses = mobs::anim::Poses::new(model);
            mobs::anim::setup(m,&mobs::anim::state(m,1.0),&mut poses,&self.res.mob_data().animations);
            let bones: HashMap<_,_> = ["body","head","right_arm","left_arm","right_leg","left_leg"]
                .into_iter().map(|name| (name,*poses.get(name))).collect();
            let bends: HashMap<_,_> = poses.bends.iter().map(|(name,b)| (name,b.angle)).collect();
            let position = core(m.position);
            let palm = mobs::render::player_palm(m,1.0,position,self.res.mob_data(),m.main_arm_left())
                .map(|p| position + p.as_dvec3());
            Some(serde_json::json!({"entity":m.id,"own":self.player_model.as_ref().is_some_and(|own| std::ptr::eq(m,own)),
                "spell":playback.event.spell_id,"cast_id":playback.event.cast_id,"phase":playback.event.phase,
                "age":playback.age(),"fade":playback.fade(),"bones":bones,"bends":bends,"palm":palm,"left":m.main_arm_left(),"slim":m.slim}))
        }).collect()
    }
    pub(in crate::game) fn arcana_cast_arms(
        &self,
        alpha: f32,
        light: u32,
        out: &mut Vec<horizon_client::mesher::Vertex>,
    ) -> bool {
        let Some(m) = self.player_model.as_ref().filter(|m| {
            m.extra.cast_animation.as_ref().is_some_and(|p| p.alive())
                || mobs::fire_form::active(m).is_some()
        }) else {
            return false;
        };
        if self.potions.effects.get("invisibility").is_some() {
            return false;
        }
        if m.extra.cast_animation.as_ref().is_none_or(|p| !p.alive()) {
            let to_world = horizon_client::camera::view_matrix(self.yaw, self.pitch).inverse();
            for left in [false, true] {
                self.first_person_arm(
                    left,
                    to_world * horizon_client::first_person::player_arm_pose(left, 0.0, 0.0),
                    light,
                    out,
                );
            }
            return true;
        }
        let (key, texture) = mobs::render::body(m);
        let model = m.kind.layer(&key).unwrap_or(&m.kind.model);
        let Some(uv) = self.gui.atlas.sprites.get(&texture) else {
            return false;
        };
        let camera = glam::DVec3::from_array(self.eye_position().unwrap_or_default());
        let base = mobs::render::base_transform(m, alpha, camera);
        let mut poses = mobs::anim::Poses::new(model);
        mobs::anim::setup(
            m,
            &mobs::anim::state(m, alpha),
            &mut poses,
            &self.res.mob_data().animations,
        );
        for part in ["head", "body", "left_leg", "right_leg"] {
            poses.visible(part, false);
        }
        let lookup = |name: &str| {
            self.gui
                .atlas
                .sprites
                .get(name)
                .map(|r| [r.u0, r.v0, r.u1, r.v1])
        };
        let start = out.len();
        if !mobs::fire_form::render(m, model, base, &poses, &lookup, true, out) {
            mobs::render::draw(
                model,
                base,
                &poses,
                [uv.u0, uv.v0, uv.u1, uv.v1],
                0xFFFF_FFFF,
                light,
                0,
                out,
            );
        }
        // A world-space casting pose can put a forearm through the eye. Clip
        // this view-model pass only; the world rig and palm origin stay exact.
        let raw = out.split_off(start);
        let (sy, cy) = self.yaw.to_radians().sin_cos();
        let (sp, cp) = self.pitch.to_radians().sin_cos();
        let forward = glam::Vec3::new(-sy * cp, -sp, cy * cp);
        for face in raw.as_chunks::<4>().0 {
            let points = std::array::from_fn(|i| {
                [
                    face[i].pos[0],
                    face[i].pos[1],
                    face[i].pos[2],
                    face[i].uv[0],
                    face[i].uv[1],
                ]
            });
            horizon_vfx::preparation::clip_near_eye(points, forward, 0.28, |polygon| {
                for p in polygon {
                    let mut vertex = face[0];
                    vertex.pos = p[..3].try_into().unwrap();
                    vertex.uv = p[3..].try_into().unwrap();
                    out.push(vertex);
                }
            });
        }
        true
    }
}
