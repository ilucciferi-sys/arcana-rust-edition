//! Arcana migration core: catalog, casting, starter combat and elemental rites.
//! Unsupported spells fail closed before spending resources.
#![forbid(unsafe_code)]
pub mod animation;
pub mod casting;
pub mod catalog;
pub mod fire;
pub mod fire_school;
pub mod fire_terrain;
pub mod forms;
pub mod network;
pub mod ritual;
pub mod solar;
pub mod starter;
pub mod state;
pub mod storage;

pub use catalog::{Catalog, School, Spell};
pub use glam::DVec3;
