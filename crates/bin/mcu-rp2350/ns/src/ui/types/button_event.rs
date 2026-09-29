//! [`ButtonEvent`] — the frozen shape a later, hardware-wiring wave's debounced GPIO edge handler
//! produces. See [`super::button::Button`]'s own doc for the "no GPIO/pin code lives here"
//! disclaimer that applies equally to this type.

use super::button::Button;

/// A single debounced button press, ready for the UI's screen-cycling logic. One event per press —
/// the wiring layer owns debounce timing (20-50 ms per the WP5 task note) before it emits this;
/// this type carries no repeat/hold state.
///
/// Suggested mapping (decided by the wiring layer, NOT enforced here): button A = [`Screen::prev`],
/// button C = [`Screen::next`], button B = select/no-op. Only button B has an onboard pull-up
/// (added for the ESP8266) — A and C need the MCU's internal pull-ups enabled or they silently
/// never fire; that trap is the wiring wave's to avoid, not this type's to encode.
///
/// [`Screen::prev`]: super::screen::Screen::prev
/// [`Screen::next`]: super::screen::Screen::next
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonEvent {
    /// Which button fired.
    pub button: Button,
}
