//! `ns`'s host-testable **lib** target — exists so the `MockDisplay` golden tests in `ns::ui` can
//! run via `cargo test -p ns --lib --features ui` (WP5 part 1). `ns` was previously bin-only
//! (`test = false`), so these `#[cfg(test)]` modules were never compiled or run.
//!
//! `#![cfg_attr(not(test), no_std)]` stays `no_std` for the real firmware build (including
//! cross-compiling for `thumbv8m.main-none-eabihf`), dropping `no_std` only under `cargo test` so
//! the host harness gets `std`. `ns::ui` itself never uses `std` either way.
//!
//! `ns::ui` lives in exactly ONE place: here. `main.rs` reaches it via `use ns::ui::{...};`, never
//! `mod ui;` — declaring the module in both crate roots would compile `ns::ui` twice as two distinct
//! types, breaking `Screen`/`UiSnapshot`/`ButtonEvent` identity across the lib/bin boundary.
//!
//! Gated behind the `ui` feature (default OFF); with it off this lib compiles to an empty crate.
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "ui")]
pub mod ui;
