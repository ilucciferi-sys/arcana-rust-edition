//! Server origins and frame-local emitters use the same native posed skeleton.
use super::*;
use horizon_client::mobs::{self, Mob, cast_animation};

#[derive(Clone, Copy)]
pub(in crate::game) struct Body {
    pub(super) yaw: f32,
    speed: f32,
    walk: f32,
    age: u32,
    left: bool,
    slim: bool,
    crouching: bool,
}
impl Body {
    pub(in crate::game) fn guest(host: &LanHost, conn: u32) -> Option<Self> {
        let g = host.hub.guest(conn)?;
        let m = host.models.get(&conn);
        Some(Self {
            yaw: m.map_or(g.yaw, |m| m.body_yaw),
            speed: m.map_or(0.0, |m| m.limb_speed),
            walk: m.map_or(0.0, |m| m.limb_position),
            age: m.map_or(0, |m| m.age),
            left: g.info.main_hand == 0,
            slim: m.filter(|m| m.skin.is_some()).map_or_else(
                || horizon_client::tab_list::default_skin(g.uuid).contains("/slim/"),
                |m| m.slim,
            ),
            crouching: g.sneaking,
        })
    }
}
#[derive(Clone, Serialize)]
pub(super) struct Origin {
    uuid: [i32; 4],
    spell: String,
    cast_id: u64,
    tick: u64,
    age: u64,
    left: bool,
    slim: bool,
    palm: DVec3,
    start: DVec3,
    clipped: bool,
}
impl Game<'_> {
    pub(super) fn arcana_hand_clipped(&self) -> bool {
        self.arcana
            .origins
            .get(&owner_words(self.lan_acting_uuid.unwrap_or(0)))
            .is_some_and(|o| o.clipped)
    }
    pub(super) fn arcana_context(&mut self, event: Option<&CastEvent>) -> Option<(DVec3, DVec3)> {
        let (eye, direction) = self.aim()?;
        let eye = DVec3::from_array(eye);
        let direction = DVec3::from_array(direction);
        let body = self.arcana.acting_body?;
        let scale = self.player.as_mut()?.attribute("minecraft:scale").ok()? as f32;
        let p = self.player.as_ref()?;
        let position = core(p.position);
        let mut m = Mob::new(
            0,
            self.res.player_type().clone()?,
            native(position),
            self.yaw,
        );
        m.body_yaw = body.yaw;
        m.previous_body_yaw = body.yaw;
        m.head_yaw = self.yaw;
        m.previous_head_yaw = self.yaw;
        m.head_pitch = self.pitch;
        m.previous_head_pitch = self.pitch;
        m.on_ground = p.ground.on_ground;
        m.limb_speed = body.speed;
        m.previous_limb_speed = body.speed;
        m.limb_position = body.walk;
        m.age = body.age;
        m.hurt_time = p.health.hurt_animation_time;
        m.slim = body.slim;
        m.extra.main_hand_left = body.left;
        m.extra.crouching = body.crouching;
        m.extra.model_scale = scale;
        m.extra.main_hand = self.inventory[self.selected]
            .as_ref()
            .map(|s| s.item.clone());
        m.extra.off_hand = self.offhand.as_ref().map(|s| s.item.clone());
        m.extra.cast_animation = event.cloned().map(cast_animation::Playback::at_tick);
        // Keep world coordinates in f64: the renderer only receives a local offset.
        let palm = position
            + mobs::render::player_palm(&m, 1.0, position, self.res.mob_data(), body.left)?
                .as_dvec3();
        let owner = self.acting_player_uuid();
        let world = PilotWorld { game: self, owner };
        let start = match world.clip_blocks(eye, palm) {
            Ok(Some(hit)) => hit - (palm - eye).normalize_or_zero() * 0.02,
            Ok(None) => palm,
            Err(_) => eye,
        };
        if let Some(event) = event {
            // One record per current player, for the local validation/control API.
            self.arcana.origins.insert(
                owner,
                Origin {
                    uuid: owner,
                    spell: event.spell_id.clone(),
                    cast_id: event.cast_id,
                    tick: event.server_tick,
                    age: event.server_tick.saturating_sub(event.start_tick),
                    left: body.left,
                    slim: body.slim,
                    palm,
                    start,
                    clipped: start.distance_squared(palm) > 1e-10,
                },
            );
        }
        Some((start, direction))
    }
    pub(super) fn arcana_palm(
        &self,
        owner: Option<[i32; 4]>,
        spell: &str,
        cast_id: u64,
        alpha: f32,
        camera: DVec3,
    ) -> Option<glam::Vec3> {
        let owner = owner?;
        let m = if self
            .remote
            .as_ref()
            .is_some_and(|r| owner_words(r.profile()) == owner)
        {
            self.player_model.as_ref()?
        } else {
            self.mobs
                .iter()
                .find(|m| self.persist.uuids.get(&m.id) == Some(&owner))?
        };
        let playback = m.extra.cast_animation.as_ref().filter(|p| p.alive())?;
        if playback.event.spell_id != spell || playback.event.cast_id != cast_id {
            return None;
        }
        mobs::render::player_palm(m, alpha, camera, self.res.mob_data(), m.main_arm_left())
    }
}
