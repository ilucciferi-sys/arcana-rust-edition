use super::{Actor, Game, catalog};
use arcana_core::storage::PlayerStore;

impl Game<'_> {
    pub(super) fn arcana_ensure_actor(&mut self, uuid: u128) -> bool {
        if self.arcana.load_errors.contains_key(&uuid) {
            return false;
        }
        if self.arcana.actors.contains_key(&uuid) {
            return true;
        }
        match PlayerStore::new(&self.world_root).load(uuid, catalog()) {
            Ok(saved) => {
                self.arcana.actors.insert(
                    uuid,
                    Actor {
                        state: saved.unwrap_or_default(),
                        dimension: self.dim.id().into(),
                        ..Default::default()
                    },
                );
                true
            }
            Err(error) => {
                log::error!("{error}");
                self.arcana.load_errors.insert(uuid, error);
                self.arcana_send_snapshot(
                    &Actor::default(),
                    Some("Не удалось загрузить прогресс Arcana. Проверьте журнал.".into()),
                );
                false
            }
        }
    }
    pub(super) fn arcana_save_actor(&mut self, uuid: u128) -> bool {
        let Some(actor) = self.arcana.actors.get(&uuid) else {
            return true;
        };
        let state = actor.caster.saved_state(&actor.state);
        match PlayerStore::new(&self.world_root).save(uuid, &state, catalog()) {
            Ok(()) => {
                if let Some(actor) = self.arcana.actors.get_mut(&uuid) {
                    actor.dirty = false;
                }
                true
            }
            Err(error) => {
                log::error!("Arcana save failed: {error}");
                false
            }
        }
    }
    pub(in crate::game) fn arcana_save_all(&mut self) -> horizon_client::Result<()> {
        let ids: Vec<_> = self
            .arcana
            .actors
            .iter()
            .filter(|(_, a)| a.dirty)
            .map(|(id, _)| *id)
            .collect();
        for uuid in ids {
            if !self.arcana_save_actor(uuid) {
                return Err("Не удалось сохранить прогресс Arcana".into());
            }
        }
        for terrain in self.arcana.school.terrain.values_mut() {
            terrain.save()?;
        }
        Ok(())
    }
    pub(in crate::game) fn arcana_save_departing(&mut self, uuid: u128) {
        if !self.arcana_save_actor(uuid) {
            log::error!("Arcana: leaving player could not be saved");
        }
    }
    pub(in crate::game) fn arcana_player_left(&mut self, uuid: u128, conn: u32, dimension: &str) {
        self.arcana.load_errors.remove(&uuid);
        let Some(mut actor) = self.arcana.actors.remove(&uuid) else {
            return;
        };
        if let Some(arcana_core::ritual::Event::Cancelled(rite)) = actor.ritual.interrupt() {
            self.arcana.ritual_outbox.push(super::rituals::Visual {
                owner: conn,
                rite,
                finished: false,
                cancelled: true,
            });
        }
        let tick = (self.world.game_time.max(0) as u64).max(actor.tick.unwrap_or(0));
        if let Some(cast) = actor.caster.cancel(&mut actor.state, tick, true) {
            self.arcana.sequence += 1;
            self.arcana.outbox.push(super::Wire {
                owner: conn,
                uuid: Some(super::owner_words(uuid)),
                dimension: dimension.into(),
                event: arcana_core::network::ServerEvent {
                    protocol: arcana_core::network::PROTOCOL,
                    session: self.arcana.session,
                    event_id: self.arcana.sequence,
                    cast,
                    solar: None,
                },
            });
        }
        actor.bolts.clear();
        for active in actor.school.effects.drain(..) {
            self.arcana.school.outbox.push(super::school::Visual {
                owner: conn,
                id: active.id,
                dimension: dimension.into(),
                effect: active.effect,
                expired: true,
                body: None,
            });
        }
        actor.school.lease = None;
        // Successful saves already contain refunded, interrupted preparation.
        // Keep failed saves in memory for retry, but never resume a stale cast.
        if actor.dirty {
            self.arcana.actors.insert(uuid, actor);
        }
        self.arcana.load_errors.remove(&uuid);
    }
}
