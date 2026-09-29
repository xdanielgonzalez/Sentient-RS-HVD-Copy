//! Onboard NeoPixel status LED bring-up + task (feature `neopixel`, default OFF) — a HIL debugging
//! aid, not a mission function. Data pin = GPIO21 on the Adafruit Feather RP2350 (3.3V, no enable
//! pin), driven via embassy-rp's WS2812 PIO driver on PIO0/DMA_CH2 (both unused elsewhere in this
//! crate).
//!
//! Status colors (brightness capped, see [`DIM`]): dim white = booting/pre-net; dim red = link
//! down; dim amber = DHCP pending; dim green = DHCP acquired/broker up; a dim-blue blink layers on
//! top each time [`crate::POLL_HEARTBEAT`] advances (Seplos telemetry succeeding).
//!
//! Fail-open by construction: `PioWs2812::new`/`.write()` return no `Result` (one-way, DMA-pushed,
//! no ACK/NACK), so a wedged or absent LED can only leave the pixel dark or stale.

use core::sync::atomic::Ordering;

use embassy_net::Stack;
use embassy_rp::peripherals::{DMA_CH2, PIN_21, PIO0};
use embassy_rp::pio::Pio;
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_rp::Peri;
use embassy_time::{Duration, Instant, Ticker};
use smart_leds::RGB8;

use crate::types::Rp2350NeoPixel;

/// Per-channel cap for every status color below — `0x20` is the ceiling the task note allows;
/// `0x18` sits comfortably under it while staying clearly distinguishable room-to-room.
const DIM: u8 = 0x18;

/// Status colors. All `const` (not computed per-tick) — cheap and legible at a glance in this file.
const BOOTING: RGB8 = RGB8 { r: DIM, g: DIM, b: DIM }; // dim white
const LINK_DOWN: RGB8 = RGB8 { r: DIM, g: 0, b: 0 }; // dim red
const LINK_UP_NO_LEASE: RGB8 = RGB8 { r: DIM, g: DIM / 2, b: 0 }; // dim amber
const READY: RGB8 = RGB8 { r: 0, g: DIM, b: 0 }; // dim green
const TELEMETRY_BLINK: RGB8 = RGB8 { r: 0, g: 0, b: DIM }; // dim blue

/// Status task cadence — fast enough that link/DHCP transitions and telemetry blinks both read as
/// prompt, slow enough to leave the executor's budget alone (this is a diagnostics leg, not the
/// mission).
const STATUS_PERIOD_MS: u64 = 200;

/// How long each Seplos-telemetry blink holds before falling back to the base status color — long
/// enough to actually see at a glance, short enough to read as a "blink" rather than a color change.
const BLINK_HOLD_MS: u64 = 150;

/// Bring up the onboard NeoPixel over PIO0/DMA_CH2 and show the "booting / pre-net" color once.
///
/// Called early in `main`, before the W5500/network leg exists — see this module's doc comment for
/// why the network-aware states live in [`neopixel_task`] instead. Building the driver cannot fail
/// (see this module's doc comment); the returned handle is later handed to [`neopixel_task`] once a
/// `Stack` exists to drive the link/DHCP-aware colors.
pub(crate) async fn build_neopixel(
    pio0: Peri<'static, PIO0>,
    dma_ch: Peri<'static, DMA_CH2>,
    pin: Peri<'static, PIN_21>,
) -> Rp2350NeoPixel {
    let Pio {
        mut common, sm0, ..
    } = Pio::new(pio0, crate::Irqs);
    let program = PioWs2812Program::new(&mut common);
    let mut np = PioWs2812::new(&mut common, sm0, dma_ch, crate::Irqs, pin, &program);
    np.write(&[BOOTING]).await;
    np
}

/// The status task: owns the NeoPixel driver + the `Stack` handle for link/DHCP polling. Never
/// returns. See this module's doc comment for the color/blink semantics.
#[embassy_executor::task]
pub(crate) async fn neopixel_task(mut np: Rp2350NeoPixel, stack: Stack<'static>) {
    let mut ticker = Ticker::every(Duration::from_millis(STATUS_PERIOD_MS));
    let mut last_heartbeat = crate::POLL_HEARTBEAT.load(Ordering::Relaxed);
    let mut blink_until: Option<Instant> = None;

    loop {
        ticker.next().await;

        let heartbeat = crate::POLL_HEARTBEAT.load(Ordering::Relaxed);
        if heartbeat != last_heartbeat {
            last_heartbeat = heartbeat;
            blink_until = Some(Instant::now() + Duration::from_millis(BLINK_HOLD_MS));
        }

        let blinking = blink_until.is_some_and(|until| Instant::now() < until);
        let color = if blinking {
            TELEMETRY_BLINK
        } else if !stack.is_link_up() {
            LINK_DOWN
        } else if stack.config_v4().is_none() {
            LINK_UP_NO_LEASE
        } else {
            READY
        };

        np.write(&[color]).await;
    }
}
