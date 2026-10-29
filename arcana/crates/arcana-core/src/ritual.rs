//! Elemental awakening rules from Rituals/RitualManager. The host supplies
//! loaded block/fluid facts; clients never select the resulting school.
use crate::{Catalog, DVec3, School, state::MagicState};
use serde::{Deserialize, Serialize};

pub const DEFAULT_DURATION: u32 = 120;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fluid {
    None,
    WaterSource,
    OtherWater,
    Lava,
}
#[derive(Debug, Clone)]
pub struct Block {
    pub id: String,
    pub lit: bool,
    pub fluid: Fluid,
}
#[derive(Debug, Clone, Copy)]
pub enum Requirement {
    Block(&'static str),
    LitCampfire,
    SourceWater,
    Lava,
}
impl Requirement {
    pub fn accepts(self, block: &Block) -> bool {
        match self {
            Self::Block(id) => block.id == id,
            Self::LitCampfire => block.id == "minecraft:campfire" && block.lit,
            Self::SourceWater => block.fluid == Fluid::WaterSource,
            Self::Lava => block.fluid == Fluid::Lava,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub offset: [i32; 3],
    pub requirement: Requirement,
}
pub fn pattern(school: School) -> Option<Vec<Cell>> {
    let (center, near, far, corner, corner_distance) = match school {
        School::Fire => ("obsidian", "obsidian", "magma_block", "obsidian", 2),
        School::Earth => (
            "polished_deepslate",
            "deepslate",
            "copper_block",
            "iron_ore",
            2,
        ),
        School::Air => ("quartz_block", "smooth_quartz", "quartz_block", "glass", 1),
        School::Water => ("prismarine", "prismarine_bricks", "", "sea_lantern", 2),
        School::Necromancy => return None, // requires soul item and UndeadArmy, not an elemental rite
    };
    let block_id = |id| match id {
        "obsidian" => "minecraft:obsidian",
        "magma_block" => "minecraft:magma_block",
        "polished_deepslate" => "minecraft:polished_deepslate",
        "deepslate" => "minecraft:deepslate",
        "copper_block" => "minecraft:copper_block",
        "iron_ore" => "minecraft:iron_ore",
        "quartz_block" => "minecraft:quartz_block",
        "smooth_quartz" => "minecraft:smooth_quartz",
        "glass" => "minecraft:glass",
        "prismarine" => "minecraft:prismarine",
        "prismarine_bricks" => "minecraft:prismarine_bricks",
        "sea_lantern" => "minecraft:sea_lantern",
        _ => unreachable!(),
    };
    let mut cells = vec![Cell {
        offset: [0, 0, 0],
        requirement: Requirement::Block(block_id(center)),
    }];
    let mut add = |offset, requirement| {
        cells.push(Cell {
            offset,
            requirement,
        })
    };
    if school == School::Water {
        for x in -1..=1 {
            for z in -1..=1 {
                if x != 0 || z != 0 {
                    add([x, 0, z], Requirement::Block(block_id(near)));
                    add([x, 1, z], Requirement::SourceWater);
                }
            }
        }
    } else {
        for (distance, id) in [(1, near), (2, far)] {
            for [x, z] in [[distance, 0], [-distance, 0], [0, distance], [0, -distance]] {
                add([x, 0, z], Requirement::Block(block_id(id)));
            }
        }
    }
    for x in [-corner_distance, corner_distance] {
        for z in [-corner_distance, corner_distance] {
            add([x, 0, z], Requirement::Block(block_id(corner)));
            if school == School::Fire {
                add([x, 1, z], Requirement::LitCampfire);
            }
        }
    }
    if school == School::Fire {
        add([3, 0, 0], Requirement::Lava);
    }
    if school == School::Air {
        for [x, z] in [[2, 0], [-2, 0], [0, 2], [0, -2]] {
            add([x, 1, z], Requirement::Block("minecraft:end_rod"));
        }
    }
    Some(cells)
}
pub trait World {
    /// None means unloaded, never substitute air from an unloaded chunk.
    fn block(&self, at: [i32; 3]) -> Option<Block>;
    fn can_see_sky(&self, at: [i32; 3]) -> bool;
    fn occupied(&self, dimension: &str, center: [i32; 3]) -> bool;
}
pub fn matches(world: &impl World, school: School, center: [i32; 3]) -> bool {
    let Some(cells) = pattern(school) else {
        return false;
    };
    cells.iter().all(|cell| {
        let at = std::array::from_fn(|i| center[i] + cell.offset[i]);
        world
            .block(at)
            .is_some_and(|b| cell.requirement.accepts(&b))
    }) && match school {
        School::Earth => center[1] <= 64,
        School::Air => center[1] >= 120 && world.can_see_sky([center[0], center[1] + 1, center[2]]),
        _ => true,
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Activation {
    pub school: School,
    pub center: [i32; 3],
    pub dimension: String,
    pub progress: u32,
    pub duration: u32,
}
impl Activation {
    pub fn valid(&self) -> bool {
        self.school != School::Necromancy
            && (40..=1200).contains(&self.duration)
            && self.progress <= self.duration
            && self.dimension.len() <= 128
            && self.center.iter().all(|v| v.unsigned_abs() <= 30_000_000)
    }
}
pub struct Context {
    pub position: DVec3,
    pub dimension: String,
    pub alive: bool,
    pub spectator: bool,
    pub sleeping: bool,
    pub passenger: bool,
    pub empty_hands: bool,
    pub sneaking: bool,
    pub casting: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    AlreadyBound,
    InvalidPattern,
    Occupied,
}
#[derive(Debug, Clone)]
pub enum Event {
    Started(Activation),
    Progress(Activation),
    Levitation(Activation),
    Completed(Activation),
    Cancelled(Activation),
    Rejected(Rejection),
}
pub struct Tracker {
    active: Option<Activation>,
    sneak_ticks: u32,
    duration: u32,
}
impl Default for Tracker {
    fn default() -> Self {
        Self {
            active: None,
            sneak_ticks: 0,
            duration: DEFAULT_DURATION,
        }
    }
}
impl Tracker {
    pub fn with_duration(duration: u32) -> Result<Self, String> {
        if !(40..=1200).contains(&duration) {
            return Err("ritual duration must be 40..=1200".into());
        }
        Ok(Self {
            duration,
            ..Default::default()
        })
    }
    pub fn active(&self) -> Option<&Activation> {
        self.active.as_ref()
    }
    pub fn interrupt(&mut self) -> Option<Event> {
        self.sneak_ticks = 0;
        self.active.take().map(Event::Cancelled)
    }
    /// Exactly once per authoritative tick. Releasing sneak after start is allowed.
    pub fn tick(
        &mut self,
        world: &impl World,
        context: &Context,
        state: &mut MagicState,
        catalog: &Catalog,
    ) -> Option<Event> {
        if let Some(mut active) = self.active.take() {
            let target =
                DVec3::from_array(active.center.map(f64::from)) + DVec3::new(0.5, 1.5, 0.5);
            if !context.alive
                || context.spectator
                || context.passenger
                || !context.position.is_finite()
                || context.dimension != active.dimension
                || context.position.distance_squared(target) > 9.0
                || state.school().is_some()
                || (active.progress.is_multiple_of(10)
                    && !matches(world, active.school, active.center))
            {
                return Some(Event::Cancelled(active));
            }
            active.progress += 1;
            if active.progress >= active.duration {
                return Some(
                    if matches(world, active.school, active.center)
                        && state.awaken(active.school, catalog)
                    {
                        Event::Completed(active)
                    } else {
                        Event::Cancelled(active)
                    },
                );
            }
            let event = if active.school == School::Air && active.progress == active.duration - 25 {
                Some(Event::Levitation(active.clone()))
            } else if active.progress.is_multiple_of(10) {
                Some(Event::Progress(active.clone()))
            } else {
                None
            };
            self.active = Some(active);
            return event;
        }
        self.sneak_ticks = if context.sneaking {
            self.sneak_ticks.saturating_add(1)
        } else {
            0
        };
        if self.sneak_ticks != 20
            || !context.alive
            || context.spectator
            || context.sleeping
            || context.passenger
            || !context.empty_hands
            || context.casting
            || !context.position.is_finite()
        {
            return None;
        }
        let center = [
            context.position.x.floor() as i32,
            context.position.y.floor() as i32 - 1,
            context.position.z.floor() as i32,
        ];
        let block = world.block(center)?;
        let school = match block.id.as_str() {
            "minecraft:obsidian" => School::Fire,
            "minecraft:prismarine" => School::Water,
            "minecraft:polished_deepslate" => School::Earth,
            "minecraft:quartz_block" => School::Air,
            _ => return None,
        };
        if state.school().is_some() {
            return Some(Event::Rejected(Rejection::AlreadyBound));
        }
        if !matches(world, school, center) {
            return Some(Event::Rejected(Rejection::InvalidPattern));
        }
        if world.occupied(&context.dimension, center) {
            return Some(Event::Rejected(Rejection::Occupied));
        }
        let active = Activation {
            school,
            center,
            dimension: context.dimension.clone(),
            progress: 0,
            duration: self.duration,
        };
        self.active = Some(active.clone());
        Some(Event::Started(active))
    }
}
