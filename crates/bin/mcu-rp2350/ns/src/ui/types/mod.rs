//! Type definitions for the `ui` module (OLED four-screen UI, WP5 part 1 — pure render layer). Per
//! the repo's Go-style data/behavior split, the type definitions live here under `types/`; their
//! `impl` blocks live in the sibling `impl_*.rs` files at the `ui` module root (`crate::ui`).

mod button;
mod button_event;
mod dhcp_state;
mod screen;
mod ui_snapshot;

pub use button::Button;
pub use button_event::ButtonEvent;
pub use dhcp_state::DhcpState;
pub use screen::{Screen, ScreenView};
pub use ui_snapshot::UiSnapshot;
