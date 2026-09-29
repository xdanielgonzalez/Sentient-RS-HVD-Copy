//! `test-e2e` — cross-crate / full-pipeline end-to-end tests for sentient-rs (#99).
//!
//! This crate has no library surface of its own; it exists solely to host `tests/*.rs`, moved
//! here from their previous per-crate homes (`lib-mqtt`, `lib-link`, `lib-io`) so
//! cross-crate e2e coverage lives in one findable place, depending on every crate under test plus
//! `test-kit`. Crate-local unit/component tests stay where they are (see #99's acceptance
//! criteria) — only tests that exercise multiple crates end-to-end moved here.
//!
//! std/host-only: never a dependency of the no_std MCU build.
