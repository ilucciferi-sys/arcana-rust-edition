//! Owned ArcaneFire cells. The adapter supplies placement/protection and block
//! facts; this journal never substitutes unowned vanilla fire.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type Position = [i32; 3];
pub const MAX_FIRES: usize = 4096;
pub const NEIGHBORS: [Position; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];
pub fn offset(p: Position, d: Position) -> Position {
    [p[0] + d[0], p[1] + d[1], p[2] + d[2]]
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Block {
    pub air: bool,
    pub fluid: bool,
    pub fire: bool,
    pub flammable: bool,
}
pub trait Terrain {
    /// None means unloaded/outside build limits/border: never load on demand.
    fn block(&self, at: Position) -> Option<Block>;
    fn allowed(&self, owner: u128, at: Position) -> bool;
    fn raining(&self, at: Position) -> bool;
    fn fire_tick(&self, at: Position) -> bool;
    fn owner_present(&self, owner: u128) -> bool;
    /// Must apply the same build, protected container and world-event policy.
    fn burn_flammable(&mut self, owner: u128, at: Position) -> bool;
    fn one_in_three(&mut self) -> bool;
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cell {
    pub at: Position,
    pub owner: u128,
    pub age: u8,
    pub next_tick: u64,
}
#[derive(Debug, Default, Clone)]
pub struct OwnedFire {
    cells: BTreeMap<Position, Cell>,
}
#[derive(Serialize, Deserialize)]
struct Save {
    schema: u8,
    cells: Vec<Cell>,
}
impl OwnedFire {
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
        self.cells.values()
    }
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    fn supported(world: &impl Terrain, at: Position) -> bool {
        NEIGHBORS.iter().any(|d| {
            world
                .block(offset(at, *d))
                .is_some_and(|b| !b.air && !b.fluid && !b.fire)
        })
    }
    pub fn ignite(&mut self, world: &impl Terrain, owner: u128, at: Position, tick: u64) -> bool {
        if self.cells.len() >= MAX_FIRES
            || self.cells.contains_key(&at)
            || !world.allowed(owner, at)
            || !world.block(at).is_some_and(|b| b.air && !b.fluid)
            || !Self::supported(world, at)
        {
            return false;
        }
        self.cells.insert(
            at,
            Cell {
                at,
                owner,
                age: 0,
                next_tick: tick.saturating_add(30),
            },
        );
        true
    }
    /// ArcaneFire ages independently of doFireTick; that rule controls spread.
    /// Unloaded cells pause rather than catching up with a burst of spread.
    pub fn tick(&mut self, world: &mut impl Terrain, tick: u64) -> bool {
        let mut changed = false;
        let mut remove = Vec::new();
        let mut spread = Vec::new();
        for cell in self.cells.values_mut() {
            let Some(block) = world.block(cell.at) else {
                continue;
            };
            if !block.air
                || block.fluid
                || world.raining(cell.at)
                || !Self::supported(world, cell.at)
            {
                remove.push(cell.at);
                continue;
            }
            if tick < cell.next_tick {
                continue;
            }
            if cell.age >= 12 {
                remove.push(cell.at);
                continue;
            }
            cell.age += 1;
            cell.next_tick = tick.saturating_add(25);
            changed = true;
            if world.fire_tick(cell.at) && world.owner_present(cell.owner) {
                for d in NEIGHBORS {
                    let at = offset(cell.at, d);
                    if world.block(at).is_some_and(|b| b.flammable)
                        && world.one_in_three()
                        && world.burn_flammable(cell.owner, at)
                    {
                        spread.push((cell.owner, at));
                    }
                }
            }
        }
        for at in remove {
            changed |= self.cells.remove(&at).is_some();
        }
        for (owner, at) in spread {
            changed |= self.ignite(world, owner, at, tick);
        }
        changed
    }
    pub fn extinguish(&mut self, at: Position) -> bool {
        self.cells.remove(&at).is_some()
    }
    pub fn protects(&self, owner: u128, feet: Position) -> bool {
        self.cells.values().any(|c| {
            c.owner == owner
                && (c.at[0] - feet[0]).abs() <= 1
                && (c.at[2] - feet[2]).abs() <= 1
                && (feet[1] - 1..=feet[1] + 2).contains(&c.at[1])
        })
    }
    pub fn touching(&self, bounds: crate::solar::Bounds) -> Vec<Position> {
        self.cells
            .values()
            .filter(|c| {
                let p =
                    crate::DVec3::new(f64::from(c.at[0]), f64::from(c.at[1]), f64::from(c.at[2]));
                !(p + crate::DVec3::ONE).cmple(bounds.min).any() && !p.cmpge(bounds.max).any()
            })
            .map(|c| c.at)
            .collect()
    }
    pub fn save_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&Save {
            schema: 1,
            cells: self.cells.values().cloned().collect(),
        })
    }
    pub fn load_json(text: &str) -> Result<Self, String> {
        let save: Save = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if save.schema != 1
            || save.cells.len() > MAX_FIRES
            || save
                .cells
                .iter()
                .any(|c| c.age > 12 || c.at.iter().any(|v| v.unsigned_abs() > 30_000_000))
        {
            return Err("invalid owned fire journal".into());
        }
        let count = save.cells.len();
        let cells = save
            .cells
            .into_iter()
            .map(|c| (c.at, c))
            .collect::<BTreeMap<_, _>>();
        if cells.len() != count {
            return Err("duplicate owned fire cell".into());
        }
        Ok(Self { cells })
    }
}
