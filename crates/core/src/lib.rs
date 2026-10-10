//! pursuer_core: every rule of Pursuer.
//!
//! Builder requirement: this crate performs no input or output. It takes
//! values and returns values, so `cargo test` reaches every rule without a
//! browser. `tests/boundary.rs` enforces that, along with the dependency
//! allowlist and the reference ban on floats, hash maps and clocks.
#![allow(clippy::needless_range_loop, clippy::collapsible_match)]

pub mod car;
pub mod city;
pub mod content;
pub mod fx;
pub mod game;
pub mod geom;
pub mod house;
pub mod mesh;
pub mod monster;
pub mod quat;
pub mod render;
pub mod rng;
pub mod rubble;
pub mod terrain;
pub mod walker;
