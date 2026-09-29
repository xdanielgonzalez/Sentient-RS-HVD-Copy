//! `impl Default for UiSnapshot` — a well-defined baseline snapshot (link down, no readings yet)
//! for the wiring layer's initial `Watch` value and for this module's own golden tests.

use super::types::{DhcpState, UiSnapshot};

impl Default for UiSnapshot {
    fn default() -> Self {
        Self {
            dhcp_state: DhcpState::Down,
            ip_addr: None,
            broker_up: false,
            mesh_node_num: 0,
            mesh_frames_sent: 0,
            mesh_last_send_ok: false,
            battery_soc_dpct: 0,
            pack_voltage_cv: 0,
            pack_current_ca: 0,
            fw_version: "0.0.0",
            uptime_secs: 0,
            rail_eth_ma: 0,
            rail_radio_ma: 0,
        }
    }
}
