use super::*;
use arcana_core::ritual::{self, Activation, Context, Event, Rejection};

pub(super) const CHANNEL: &str = "arcana:ritual_v1";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Visual {
    pub owner: u32,
    pub rite: Activation,
    pub finished: bool,
    pub cancelled: bool,
}
pub(super) struct Viewed {
    pub message: Visual,
    pub received: Instant,
}
fn school_name(school: School) -> &'static str {
    match school {
        School::Fire => "Огонь",
        School::Water => "Вода",
        School::Earth => "Земля",
        School::Air => "Воздух",
        School::Necromancy => "Некромантия",
    }
}
impl Game<'_> {
    pub(super) fn arcana_tick_ritual(&mut self, actor: &mut Actor, alive: bool) -> bool {
        let context = Context {
            position: self
                .player
                .as_ref()
                .map_or(DVec3::NAN, |p| core(p.position)),
            dimension: self.dim.id().into(),
            alive,
            spectator: self.spectator(),
            sleeping: self.sleeping.is_some(),
            passenger: self.riding.is_some(),
            empty_hands: self.stack_in(self.selected).is_none() && self.offhand.is_none(),
            sneaking: self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft),
            casting: !actor.caster.idle(),
        };
        let Some(event) =
            actor
                .ritual
                .tick(&RitualWorld(self), &context, &mut actor.state, catalog())
        else {
            return false;
        };
        let completed = matches!(event, Event::Completed(_));
        let mut notice = None;
        let visual = match event {
            Event::Rejected(reason) => {
                notice = Some(
                    match reason {
                        Rejection::AlreadyBound => "Стихия уже выбрана. Сменить её нельзя.",
                        Rejection::InvalidPattern => "Проверьте конструкцию и условия ритуала.",
                        Rejection::Occupied => "Ритуальное место уже занято.",
                    }
                    .to_owned(),
                );
                None
            }
            Event::Started(rite) => {
                notice = Some("Оставайтесь рядом с центром.".into());
                log::info!("Arcana ritual started: {:?}", rite.school);
                Some((rite, false, false))
            }
            Event::Completed(rite) => {
                notice = Some(format!("Пробуждена стихия: {}!", school_name(rite.school)));
                log::info!("Arcana ritual completed: {:?}, level=1", rite.school);
                Some((rite, true, false))
            }
            Event::Cancelled(rite) => {
                notice = Some("Пробуждение прервано.".into());
                log::info!("Arcana ritual cancelled: {:?}", rite.school);
                Some((rite, false, true))
            }
            Event::Levitation(rite) => {
                let mut effect = horizon_client::effects::EffectInstance::new("levitation", 20, 0);
                effect.visible = false;
                self.add_player_effect(effect);
                Some((rite, false, false))
            }
            Event::Progress(rite) => Some((rite, false, false)),
        };
        if let Some(text) = notice {
            actor.notice = Some((text, self.world.game_time.max(0) as u64 + 100));
        }
        if let Some((rite, finished, cancelled)) = visual {
            self.arcana.ritual_outbox.push(Visual {
                owner: self.lan_acting.unwrap_or(0),
                rite,
                finished,
                cancelled,
            });
        }
        self.arcana_send_snapshot(actor, None);
        completed
    }
    pub(in crate::game) fn arcana_receive_ritual(&mut self, bytes: &[u8]) {
        if bytes.len() > MAX_PACKET {
            return;
        }
        let Ok(message) = serde_json::from_slice::<Visual>(bytes) else {
            return;
        };
        if !message.rite.valid() {
            return;
        }
        if message.cancelled {
            self.arcana.ritual_views.remove(&message.owner);
        } else if self.arcana.ritual_views.len() < 256
            || self.arcana.ritual_views.contains_key(&message.owner)
        {
            self.arcana.ritual_views.insert(
                message.owner,
                Viewed {
                    message,
                    received: Instant::now(),
                },
            );
        }
    }
    pub(super) fn arcana_ritual_hud(
        &self,
        hud: &mut horizon_client::hud::HudBatch,
        snapshot: &MagicSnapshot,
    ) {
        use horizon_client::hud::rgba;
        let atlas = &self.gui.atlas;
        let start = hud.vertices.len();
        hud.fill(atlas, 7.0, 7.0, 300.0, 60.0, rgba(10, 15, 23, 205));
        let title = snapshot.ritual.as_ref().map_or_else(
            || "Стихия ещё не пробуждена".into(),
            |r| {
                format!(
                    "Пробуждение: {} · {}%",
                    school_name(r.school),
                    r.progress * 100 / r.duration
                )
            },
        );
        hud.text(atlas, &title, 14.0, 12.0, rgba(190, 220, 245, 255), true);
        if let Some(rite) = &snapshot.ritual {
            hud.fill(atlas, 14.0, 28.0, 278.0, 3.0, rgba(50, 65, 80, 255));
            hud.fill(
                atlas,
                14.0,
                28.0,
                278.0 * rite.progress as f32 / rite.duration as f32,
                3.0,
                rgba(120, 205, 245, 255),
            );
        } else {
            hud.text(
                atlas,
                "Пустые руки · Shift на центре ритуала",
                14.0,
                28.0,
                rgba(195, 207, 221, 255),
                true,
            );
        }
        hud.text(
            atlas,
            snapshot
                .notice
                .as_deref()
                .unwrap_or("Постройте ритуал выбранной стихии"),
            14.0,
            43.0,
            rgba(255, 200, 140, 255),
            true,
        );
        hud.scale_since(start, 7.0, 7.0, 0.65, 0.65);
    }
}
struct RitualWorld<'a, 'r>(&'a Game<'r>);
impl ritual::World for RitualWorld<'_, '_> {
    fn block(&self, at: [i32; 3]) -> Option<ritual::Block> {
        let [x, y, z] = at;
        if !self.0.world.chunks.contains_key(&(x >> 4, z >> 4)) {
            return None;
        }
        let state = self.0.world.block(x, y, z);
        let fact = self.0.res.facts().get(state);
        use horizon_client::blocks::Fluid;
        let fluid = match fact.fluid {
            Fluid::Water if fact.fluid_level == 8 && !fact.fluid_falling => {
                ritual::Fluid::WaterSource
            }
            Fluid::Water => ritual::Fluid::OtherWater,
            Fluid::Lava => ritual::Fluid::Lava,
            _ => ritual::Fluid::None,
        };
        Some(ritual::Block {
            id: fact.name.to_string(),
            fluid,
            lit: self
                .0
                .pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get("lit"))
                .is_some_and(|v| v == "true"),
        })
    }
    fn can_see_sky(&self, [x, y, z]: [i32; 3]) -> bool {
        self.0.dim == crate::game::Dim::Overworld
            && self.0.world.chunks.contains_key(&(x >> 4, z >> 4))
            && self.0.world.light(x, y, z) >> 4 == 15
    }
    fn occupied(&self, dimension: &str, center: [i32; 3]) -> bool {
        self.0
            .arcana
            .actors
            .values()
            .filter_map(|a| a.ritual.active())
            .any(|r| {
                r.dimension == dimension
                    && r.center
                        .iter()
                        .zip(center)
                        .map(|(a, b)| i64::from(*a - b).pow(2))
                        .sum::<i64>()
                        < 36
            })
    }
}
