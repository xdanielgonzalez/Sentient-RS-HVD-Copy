//! Bring-up helpers for the Seplos/UART leg of `main` (EPIC #350) — the NS_PARSER MPU region, the
//! UART0 Seplos transport + RS-485 DE pin, the `McuPlatform` construction, the optional `veneers`
//! Seal-stage wiring, and the parser/poll-loop task spawn (core 0 default vs `core1-parser`). Each
//! fn is one step of the sequence `main` calls in order — see `main.rs`'s `async fn main` for the
//! call order and `boot_w5500.rs` for the W5500/broker leg.

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{PIN_0, PIN_1, PIN_29, UART0};
use embassy_rp::uart::{BufferedUart, Config, DataBits, Parity, StopBits};
use embassy_rp::Peri;
use embassy_time::Delay;

use crate::types::{BufferedUartAdapter, EmbassyClock};
use crate::{Irqs, Rp2350De, Rp2350Platform, Rp2350Receiver, Rp2350Sender, Rp2350Uart};

#[cfg(feature = "veneers")]
use crate::types::VeneerSessionCipher;
#[cfg(feature = "veneers")]
use lib_net::channel::CryptoStage;
#[cfg(feature = "veneers")]
use lib_security::{KeyHandle, SessionCipher};

/// Program the PMSAv8 `NS_PARSER` MPU region over the static parser buffer pool — always done,
/// unconditionally, before any task starts. See `mpu.rs`'s module doc for exactly what this does
/// and does not provide (fault containment, NOT enforced per-task privilege separation).
///
/// # Panics
/// Panics if `cortex_m::Peripherals::take()` observes a prior take — see the safety note below for
/// why that never happens in this binary's own boot path.
///
/// # Safety (of the `unsafe` block inside)
/// `init_ns_parser_region` must be called at most once, before any task that touches the parser
/// buffer pool has started — satisfied here because this fn runs synchronously, before
/// `main` spawns anything. `cortex_m::Peripherals::take()` is likewise called exactly once in this
/// binary (`embassy_rp::init` uses its OWN `Peripherals` singleton, a distinct type — verified
/// against embassy-rp 0.10.0's `src/lib.rs`, which never touches `cortex_m::Peripherals`), so the
/// `.expect()` below never observes a prior take.
pub(crate) fn init_mpu() {
    let cm_peripherals = cortex_m::Peripherals::take().expect("cortex-m Peripherals already taken");
    unsafe {
        crate::mpu::init_ns_parser_region(
            &cm_peripherals.MPU,
            crate::PARSER_BUFFER_POOL.base_addr(),
            crate::PARSER_BUFFER_POOL.pool_len(),
        );
    }
}

/// Bring up UART0 = GP0/GP1, 19200 8N1 — the Seplos wire config (matches hil/pico-seplos-emu and
/// `lib_io::transport::SerialParams::default()`'s doc comment) — plus the RS-485 driver-enable pin
/// on **GP29** (WP1, delight-features, 2026-07-25: verified Adafruit Feather RP2350 pinout —
/// deliberately NOT GP2, which on this board is I2C1's fixed SDA pin and was wrongly used for DE
/// on the earlier Pico 2 layout), and wraps them into the `lib_io::EmbassyUart` transport
/// `bin_mcu_platform::McuPlatform` wants.
///
/// `BufferedUart` (interrupt-driven, static ring buffers), NOT the DMA `Uart<Async>`: in
/// embassy-rp 0.10 only `BufferedUart` implements the `embedded_io_async::Read + Write` traits
/// `lib_io::EmbassyUart` requires — and it implements the 0.7 version of them, bridged to the 0.6
/// version lib-io wants by `BufferedUartAdapter` (see its doc comment). No DMA channels are needed
/// in buffered mode.
///
/// `lib_io::EmbassyUart<U, D, T>` is embassy-version-free (see that module's doc comment): its
/// `delay: T` field takes any `embedded_hal_async::delay::DelayNs` impl, so this crate's own
/// `embassy_time::Delay` (embassy-time 0.5.1) satisfies it without lib-io itself pinning any
/// embassy-time version.
pub(crate) fn build_seplos_transport(
    uart0: Peri<'static, UART0>,
    tx_pin: Peri<'static, PIN_0>,
    rx_pin: Peri<'static, PIN_1>,
    de_pin: Peri<'static, PIN_29>,
) -> lib_io::embassy_uart::EmbassyUart<Rp2350Uart, Rp2350De, Delay> {
    let mut config = Config::default();
    config.baudrate = 19200;
    config.data_bits = DataBits::DataBits8;
    config.stop_bits = StopBits::STOP1;
    config.parity = Parity::ParityNone;
    let tx_buf: &'static mut [u8; crate::UART_BUF_LEN] =
        crate::UART_TX_BUF.init([0u8; crate::UART_BUF_LEN]);
    let rx_buf: &'static mut [u8; crate::UART_BUF_LEN] =
        crate::UART_RX_BUF.init([0u8; crate::UART_BUF_LEN]);
    let uart = BufferedUart::new(
        uart0,
        tx_pin,
        rx_pin,
        Irqs,
        tx_buf.as_mut_slice(),
        rx_buf.as_mut_slice(),
        config,
    );
    let uart = BufferedUartAdapter::new(uart);

    let dir_enable = Output::new(de_pin, Level::Low);

    lib_io::embassy_uart::EmbassyUart {
        uart,
        dir_enable,
        delay: Delay,
    }
}

/// Wire `transport` + a fresh [`EmbassyClock`] + `MQTT_CHANNEL`'s sender half into a
/// [`Rp2350Platform`] (`McuPlatform` has no `new` — see `bin_mcu_platform::types::mcu_platform` —
/// so the struct literal is built directly). Hands back the channel's receiver half alongside it:
/// `rx` is `run_task`'s (the poll loop's) output either way — the raw channel receiver in the
/// default build, or the input to the optional `veneers` Seal stage (see
/// [`resolve_drain_rx`]) — so this is the one place that pairs sender and receiver from the same
/// static channel.
pub(crate) fn build_platform(
    transport: lib_io::embassy_uart::EmbassyUart<Rp2350Uart, Rp2350De, Delay>,
) -> (Rp2350Platform, Rp2350Receiver) {
    let clock = EmbassyClock::new();

    let tx: Rp2350Sender = crate::MQTT_CHANNEL.sender();
    let rx: Rp2350Receiver = crate::MQTT_CHANNEL.receiver();
    let publisher = bin_mcu_platform::ChannelPublisher::new(tx);

    let platform: Rp2350Platform = bin_mcu_platform::McuPlatform {
        transport,
        publisher,
        clock,
    };

    (platform, rx)
}

/// Stage 4 (#355) `veneers`: insert the optional Seal stage between the publisher's channel output
/// (`rx`, `MQTT_CHANNEL`'s receiver) and whichever receiver `drain_task`/`broker_task` should
/// actually hold — the raw one directly (default) or the sealed-channel one (feature `veneers`),
/// so the default path stays byte-identical (the returned receiver IS `rx` when the feature is
/// off).
#[cfg_attr(not(feature = "veneers"), allow(unused_variables))]
pub(crate) fn resolve_drain_rx(spawner: Spawner, rx: Rp2350Receiver) -> Rp2350Receiver {
    #[cfg(not(feature = "veneers"))]
    let drain_rx: Rp2350Receiver = rx;
    #[cfg(feature = "veneers")]
    let drain_rx: Rp2350Receiver = {
        let sealed_tx: Rp2350Sender = crate::SEALED_CHANNEL.sender();
        let sealed_rx: Rp2350Receiver = crate::SEALED_CHANNEL.receiver();

        let mut cipher = VeneerSessionCipher::new();
        // Fail closed at boot rather than run with no established session: a `veneers` build with
        // no working veneer/S-side has nothing honest to publish through the Seal stage.
        let handle: KeyHandle = cipher.establish().expect(
            "veneer session establish failed (S-side/veneer artifact not linked or refused)",
        );
        let stage = CryptoStage::new(cipher);

        spawner.spawn(
            crate::seal_task(rx, sealed_tx, stage, handle)
                .expect("failed to create seal_task token (pool exhausted)"),
        );
        sealed_rx
    };
    drain_rx
}

/// In embassy-executor 0.10 the `#[task]` fn returns `Result<SpawnToken, SpawnError>` (a token is
/// minted from a bounded per-task pool) and `Spawner::spawn` takes the token and returns `()`;
/// unwrap the token first, then spawn (matches hil/pico-seplos-emu's pattern).
///
/// Default (feature `core1-parser` OFF): `run_task` (the parser/poll leg) spawns onto core 0's own
/// spawner, same executor as everything else in this binary.
#[cfg(not(feature = "core1-parser"))]
pub(crate) fn spawn_parser(spawner: Spawner, platform: Rp2350Platform) {
    spawner.spawn(
        crate::run_task(platform, crate::run_config())
            .expect("failed to create run_task token (pool exhausted)"),
    );
}

/// Stage 4 (#355) `core1-parser`: `run_task` (the parser/poll leg) moves to its own executor on
/// CORE1 instead of core 0's spawner; `drain_task`/`broker_task` always stay on core 0. See
/// `core1.rs`'s module doc.
#[cfg(feature = "core1-parser")]
pub(crate) fn spawn_parser(
    core1: Peri<'static, embassy_rp::peripherals::CORE1>,
    platform: Rp2350Platform,
) {
    crate::core1::spawn_parser_core1(core1, platform, crate::run_config());
}
