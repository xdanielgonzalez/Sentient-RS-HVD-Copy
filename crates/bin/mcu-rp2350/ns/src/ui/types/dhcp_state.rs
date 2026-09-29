//! [`DhcpState`] — the Network screen's link-state helper enum, one field's type within
//! [`super::ui_snapshot::UiSnapshot`].

/// The Network screen's link-state helper enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpState {
    /// No lease yet requested (link down or DHCP not started).
    Down,
    /// A DHCP request is in flight.
    Requesting,
    /// A lease is held; `UiSnapshot::ip_addr` is meaningful.
    Bound,
}
