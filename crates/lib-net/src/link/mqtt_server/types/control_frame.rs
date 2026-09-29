use super::super::CONTROL_CAP;

/// A staged CONTROL packet (CONNACK/SUBACK/PINGRESP) — never a PUBLISH; those are rendered directly
/// into the caller's `out` buffer at drain time, never staged as bytes.
pub(in crate::link::mqtt_server) struct ControlFrame {
    pub(in crate::link::mqtt_server) bytes: heapless::Vec<u8, CONTROL_CAP>,
}
