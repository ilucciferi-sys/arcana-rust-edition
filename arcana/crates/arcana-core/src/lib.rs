//! Arcana's first migration slice. All 76 definitions are preserved; only the
//! solar spear has a world executor at this stage. Unsupported spells fail closed.
#![forbid(unsafe_code)]
pub mod casting;
pub mod catalog;
pub mod network;
pub mod solar;
pub mod state;

pub use catalog::{Catalog, School, Spell};
pub use glam::DVec3;
