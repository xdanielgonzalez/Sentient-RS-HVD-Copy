//! Bring-up + radio-link helpers for the Meshtastic mesh-radio leg of `main` (WP3): UART1 Heltec
//! transport ([`build_mesh_transport`]) + radio task ([`mesh_radio_task`]), draining `MESH_CHANNEL`
//! (outgoing) and reading `FromRadio` bytes over one [`lib_io::transport::SerialTransport`].
//! Compiled only under feature `mesh` (default OFF).
//!
//! Heltec V4 Serial Module needs `moduleConfig.serial.mode = PROTO` with `rxd`/`txd` on two spare
//! GPIOs matching [`MESH_BAUD`] — the USB-C console UART pins (43/44) cannot be repurposed for this
//! in PROTO mode (meshtastic/firmware #7749, open as of fw 2.7.6).
//!
//! Read/write are sequential (`EmbassyUart`'s `SerialTransport` takes `&mut self` for both):
//! [`mesh_radio_task`] `select`s the channel receive against a single ~500ms-bounded
//! `transport.read()` call.

use embassy_futures::select::{select, Either};
use embassy_rp::peripherals::{PIN_24, PIN_25, UART1};
use embassy_rp::uart::{BufferedUart, Config, DataBits, Parity, StopBits};
use embassy_rp::Peri;
use lib_io::transport::SerialTransport;
use lib_net::mesh::{MeshFrame, ToRadioFrame, MAX_RADIO_FRAME, PORTNUM_PRIVATE_APP};

use crate::types::{BufferedUartAdapter, FromRadioParser, Rp2350MeshDe};
use crate::{Irqs, Rp2350MeshReceiver, Rp2350MeshTransport};

/// Heltec Serial Module UART baud — matches `moduleConfig.serial.baud`'s documented default
/// (`#define BAUD 38400` in the firmware's `SerialModule.cpp`; PROTO mode honours whatever value
/// is configured, so this is a choice, not a hardware constant — change it here AND in the
/// Heltec's `moduleConfig.serial.baud` together if a different rate is ever needed).
const MESH_BAUD: u32 = 38400;

/// UART1 TX/RX ring-buffer size. A full on-air frame (stream header + `ToRadio` protobuf wrapping
/// a <= `bin_mcu_platform::MESH_PAYLOAD` (233-byte) condensed JSON payload) comes in well under
/// 256 bytes — matches `boot_seplos.rs`'s own `UART_BUF_LEN` sizing rationale (headroom over the
/// expected frame, not the 512-byte worst-case `MAX_RADIO_FRAME` ceiling a corrupt/oversized
/// declared length could claim).
pub(crate) const MESH_UART_BUF_LEN: usize = 256;

/// Bring up UART1 = GP24 (TX) / GP25 (RX), [`MESH_BAUD`] 8N1 — the Heltec Serial Module leg (WP3,
/// delight-features, 2026-07-25: reserved pins per the Feather board port; see `main.rs`'s
/// "RESERVED, do not reassign" note). No RS-485 DE pin: [`Rp2350MeshDe`] is a zero-sized no-op
/// stand-in for `EmbassyUart`'s `D` generic, so this leg never sacrifices a spare GPIO to a
/// direction pin nothing drives.
pub(crate) fn build_mesh_transport(
    uart1: Peri<'static, UART1>,
    tx_pin: Peri<'static, PIN_24>,
    rx_pin: Peri<'static, PIN_25>,
) -> Rp2350MeshTransport {
    let mut config = Config::default();
    config.baudrate = MESH_BAUD;
    config.data_bits = DataBits::DataBits8;
    config.stop_bits = StopBits::STOP1;
    config.parity = Parity::ParityNone;
    let tx_buf: &'static mut [u8; MESH_UART_BUF_LEN] =
        crate::MESH_UART_TX_BUF.init([0u8; MESH_UART_BUF_LEN]);
    let rx_buf: &'static mut [u8; MESH_UART_BUF_LEN] =
        crate::MESH_UART_RX_BUF.init([0u8; MESH_UART_BUF_LEN]);
    let uart = BufferedUart::new(
        uart1,
        tx_pin,
        rx_pin,
        Irqs,
        tx_buf.as_mut_slice(),
        rx_buf.as_mut_slice(),
        config,
    );
    let uart = BufferedUartAdapter::new(uart);

    lib_io::embassy_uart::EmbassyUart {
        uart,
        dir_enable: Rp2350MeshDe,
        delay: embassy_time::Delay,
    }
}

/// Per-`transport.read()` scratch — bytes are fed straight into [`FromRadioParser::feed`], so this
/// only needs to be large enough for one `read()` burst, not a whole frame (the parser owns the
/// cross-read accumulation buffer, sized to [`MAX_RADIO_FRAME`]).
const READ_CHUNK: usize = 64;

/// The mesh-radio task: drains `crate::MESH_CHANNEL` (outgoing condensed telemetry, produced by
/// `bin_mcu_platform::MeshPlatform::emit_reading`/`emit_fault`) onto the Heltec over `transport`,
/// framing each with Meshtastic's on-air `MeshFrame` + stream header
/// ([`ToRadioFrame::to_radio_frame`]) — and concurrently resyncs on any `FromRadio` bytes the radio
/// sends back (this module's doc comment explains the single-transport select-not-split shape).
/// Never returns.
///
/// **Fail-open**: a write failure (radio absent/unresponsive) is silently dropped — the frame is
/// gone, but the loop keeps running and the NEXT condensed reading gets its own attempt. Same
/// posture as the W5500 leg (a secondary/delight leg must never wedge the primary telemetry path,
/// which this task doesn't even touch — it only ever reads from `MESH_CHANNEL`, never from
/// `MQTT_CHANNEL`).
#[embassy_executor::task]
pub(crate) async fn mesh_radio_task(
    mut transport: Rp2350MeshTransport,
    rx: Rp2350MeshReceiver,
) -> ! {
    let mut send_buf = [0u8; MAX_RADIO_FRAME];
    let mut read_buf = [0u8; READ_CHUNK];
    let mut parser = FromRadioParser::new();
    #[cfg(feature = "defmt")]
    let mut frames_seen: u32 = 0;

    loop {
        match select(rx.receive(), transport.read(&mut read_buf)).await {
            Either::First(unit) => {
                let frame = MeshFrame {
                    channel: 0,
                    portnum: PORTNUM_PRIVATE_APP,
                    payload: unit.payload.as_slice(),
                };
                let n = frame.to_radio_frame(&mut send_buf);
                if n > 0 {
                    let sent = transport.write_all(&send_buf[..n]).await.is_ok();
                    #[cfg(feature = "defmt")]
                    defmt::info!(
                        "mesh: TX {=usize}B ToRadio frame -> Heltec (ok={=bool})",
                        n,
                        sent
                    );
                }
            }
            Either::Second(Ok(n)) => {
                #[cfg(feature = "defmt")]
                if n > 0 {
                    defmt::info!("mesh: RX {=usize}B <- Heltec (Serial Module alive)", n);
                }
                parser.feed(&read_buf[..n], |_frame_bytes| {
                    #[cfg(feature = "defmt")]
                    {
                        frames_seen = frames_seen.wrapping_add(1);
                        defmt::debug!(
                            "mesh: FromRadio frame #{=u32} ({=usize}B)",
                            frames_seen,
                            _frame_bytes.len()
                        );
                    }
                    #[cfg(not(feature = "defmt"))]
                    let _ = _frame_bytes;
                });
            }
            Either::Second(Err(_)) => {
                // Read timeout/transient UART error -- nothing arrived this cycle; loop again.
            }
        }
    }
}
