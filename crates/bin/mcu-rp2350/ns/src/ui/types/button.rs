//! [`Button`] — one of the three FeatherWing buttons (A/B/C), the frozen shape a later,
//! hardware-wiring wave's debounced GPIO edge handler produces. **No GPIO/pin code lives here**
//! — this WP5-part-1 task is pure render; the actual `Input` + `Pull::Up` +
//! `wait_for_falling_edge()` + 20-50 ms debounce loop is the wiring wave's job (WP5 task note,
//! step 4). See [`super::button_event::ButtonEvent`] for the debounced press event built from
//! this button.

/// One of the three FeatherWing buttons (A/B/C). Physical pins (GPIO9/GPIO6/GPIO5, per WP0's
/// pinout ground truth) are the wiring layer's concern, not this type's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    A,
    B,
    C,
}
