//! Step definition submodules for the cucumber spec-alignment suite.
//!
//! cucumber discovers `#[given]`/`#[when]`/`#[then]` step fns globally within the test binary
//! regardless of which module they live in, so this file is purely a declarer — it groups step
//! definitions by feature area so multiple agents/PRs can extend the suite in parallel without
//! touching the same file.
pub mod bus;
pub mod disk;
pub mod inverter;
pub mod legacy;
mod mapping;
pub mod mesh;
pub mod mqtt;
pub mod resilience;
pub mod rtps;
pub mod security;
pub mod simulation;
mod support;
pub mod tms;
pub mod tms_breadth;
