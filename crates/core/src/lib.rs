//! Pure simulation layer: blocks, generation, meshing, physics.
//!
//! No GPU or window dependencies are allowed here; this crate must stay
//! buildable headlessly so the whole simulation is unit-testable.

pub mod atlas;
pub mod blocks;
pub mod cracks;
pub mod digest;
pub mod generation;
pub mod mesher;
pub mod noise;
pub mod particles;
pub mod player;
pub mod raycast;
pub mod rng;
pub mod world;
