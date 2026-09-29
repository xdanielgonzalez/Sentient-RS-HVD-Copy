mod buffered_uart_adapter;
mod embassy_clock;
mod parser_buffer_pool;
mod pool_reservation;
mod pooled_socket;
mod rp2350_de;
// WP3 (delight-features): the Heltec mesh-radio UART leg's types. Gated behind the `mesh` feature
// so the default (non-mesh) image never pulls in `lib-net`'s `mesh` feature or `bin-mcu-platform`'s
// `MeshUnit`.
#[cfg(feature = "mesh")]
mod rp2350_mesh_de;
#[cfg(feature = "mesh")]
mod rp2350_mesh_parser;
#[cfg(feature = "mesh")]
mod rp2350_mesh_parser_state;
#[cfg(feature = "mesh")]
mod rp2350_mesh_receiver;
#[cfg(feature = "mesh")]
mod rp2350_mesh_sender;
#[cfg(feature = "mesh")]
mod rp2350_mesh_transport;
mod rp2350_net_runner;
mod rp2350_platform;
mod rp2350_publisher;
mod rp2350_receiver;
mod rp2350_sender;
mod rp2350_spi;
mod rp2350_spi_device;
mod rp2350_uart;
// WP4/WP5 (delight-features): two SINGLY-OWNED I2C buses — INA3221 on I2C0 (GP4/GP5, `power`),
// OLED on I2C1 (GP2/GP3, `ui`). No longer a shared bus + per-consumer device handle (see
// `boot_i2c.rs`'s module doc for why that arrangement was removed rather than kept).
#[cfg(feature = "ui")]
mod rp2350_i2c_bus;
#[cfg(feature = "power")]
mod rp2350_i2c0_bus;
// WP4: the board's concrete INA3221 rail-monitor instantiation (adapter self-telemetry, NOT the
// project's power source). `power` only.
#[cfg(feature = "power")]
mod rp2350_rail_monitor;
// WP5: the board's concrete SSD1306 driver instantiation. `ui` only.
#[cfg(feature = "ui")]
mod rp2350_display;
// HIL debugging aid: the board's concrete PIO WS2812 NeoPixel driver instantiation. `neopixel` only.
#[cfg(feature = "neopixel")]
mod rp2350_neopixel;
mod rp2350_w5500_interrupt;
mod rp2350_w5500_parts;
mod rp2350_w5500_reset;
mod rp2350_wiznet_runner;
mod socket_buffer_pool;

pub use buffered_uart_adapter::*;
pub use embassy_clock::*;
pub use parser_buffer_pool::*;
pub use pool_reservation::*;
pub use pooled_socket::*;
pub(crate) use rp2350_de::Rp2350De;
pub(crate) use rp2350_net_runner::Rp2350NetRunner;
pub(crate) use rp2350_platform::Rp2350Platform;
// `Rp2350Publisher`, `Rp2350Spi`, `Rp2350W5500Interrupt`, `Rp2350W5500Reset` are internal building
// blocks only referenced by their sibling `types/` files (`rp2350_platform.rs`,
// `rp2350_spi_device.rs`, `rp2350_w5500_parts.rs`, `rp2350_wiznet_runner.rs` import them directly
// via `use super::rp2350_*::...`) — not re-exported here since nothing outside `types/` names them.
pub(crate) use rp2350_receiver::Rp2350Receiver;
pub(crate) use rp2350_sender::Rp2350Sender;
// WP3 (delight-features) mesh-radio types — see the `mod` declarations above for the feature gate
// rationale. `FromRadioState` is only ever named via `FromRadioParser::state`'s type within
// `impl_rp2350_mesh_parser.rs`; re-exported alongside `FromRadioParser` for that use.
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_de::Rp2350MeshDe;
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_parser::FromRadioParser;
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_parser_state::FromRadioState;
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_receiver::Rp2350MeshReceiver;
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_sender::Rp2350MeshSender;
#[cfg(feature = "mesh")]
pub(crate) use rp2350_mesh_transport::Rp2350MeshTransport;
pub(crate) use rp2350_spi_device::Rp2350SpiDevice;
pub(crate) use rp2350_uart::Rp2350Uart;
// WP4/WP5 (delight-features) — see the `mod` declarations above for the feature-gate rationale.
#[cfg(feature = "ui")]
pub(crate) use rp2350_display::Rp2350Display;
#[cfg(feature = "ui")]
pub(crate) use rp2350_i2c_bus::Rp2350I2c1Bus;
#[cfg(feature = "power")]
pub(crate) use rp2350_i2c0_bus::Rp2350I2c0Bus;
#[cfg(feature = "power")]
pub(crate) use rp2350_rail_monitor::Rp2350RailMonitor;
#[cfg(feature = "neopixel")]
pub(crate) use rp2350_neopixel::Rp2350NeoPixel;
pub(crate) use rp2350_w5500_parts::Rp2350W5500Parts;
pub(crate) use rp2350_wiznet_runner::Rp2350WiznetRunner;
pub use socket_buffer_pool::*;

// ADR 0013 (transport security): the TLS interposition point's refusal reason (feature "tls",
// default OFF) — see `tls_refusal.rs`'s doc comment for why the type has no success variant.
#[cfg(feature = "tls")]
mod tls_refusal;
#[cfg(feature = "tls")]
pub(crate) use tls_refusal::TlsRefusal;

// The veneer `SessionCipher` seam (feature "veneers", default OFF) — see
// `veneer_session_cipher.rs`'s doc comment.
#[cfg(feature = "veneers")]
mod veneer_session_cipher;
#[cfg(feature = "veneers")]
pub use veneer_session_cipher::*;
