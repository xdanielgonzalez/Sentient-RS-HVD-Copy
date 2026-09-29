//! Pico Seplos BMS pack-emulator — HIL bench, phase 1. Targets **RP2350A** (Raspberry Pi Pico 2,
//! Cortex-M33, thumbv8m.main-none-eabihf). UART0 (GP0=TX, GP1=RX, 19200 8N1) slave: reads one
//! request frame at a time (delimited by `\r`, the Seplos EOI byte), hands it to
//! `pico_seplos_emu_core::seplos_responder::SeplosResponder`, writes the reply back. Telemetry
//! (`0x42`) only, DIP address 0, always-success (RTN=0x00), canned `BatteryState::sample_14_cell()`
//! data — see `hil/README.md` for the full phase-1 scope and wiring.
//!
//! Includes an **onboard-LED heartbeat** (GPIO25 on non-W Pico 2) toggled at ~2 Hz by a spawned async
//! task. **Note:** Pico 2 W (CYW43 variant) has the LED on the wireless chip, so the GP25 blink is a
//! no-op on that board — the CYW43 LED driver is not implemented here.
//!
//! UART1 (GP4=TX, GP5=RX) is RESERVED, unused in phase 1 — a phase-2 second responder node (e.g.
//! an ESP32 leg) plugs in there without touching this file's UART0 wiring or the responder logic
//! (see `transport::UartChannel` and `hil/README.md`'s Phase-2 section).
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::{Duration, Instant, Timer};
use panic_halt as _;
use pico_seplos_emu_core::seplos_responder::{BatteryState, DriftSpec, SeplosResponder, SimReply};

mod transport;

use transport::{EmbassyUart0, UartChannel};

/// Max on-wire request length this loop will accumulate before giving up on finding the `\r`
/// delimiter — mirrors `pico_seplos_emu_core::seplos_responder::MAX_FRAME` (256), matching the
/// request side's own bound (a request frame is always much shorter than a reply, but sharing one
/// bound keeps this loop simple and avoids introducing a second magic number).
const MAX_REQUEST: usize = 256;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut uart = EmbassyUart0::new(p.UART0, p.PIN_0, p.PIN_1);

    // Spawn the onboard LED heartbeat task (GP25 = the onboard LED pin on Pico 2 non-W).
    let led = Output::new(p.PIN_25, Level::Low);
    spawner.spawn(led_heartbeat(led).expect("Failed to create LED heartbeat task token"));

    // Capture boot time for telemetry drift (living data).
    let boot = Instant::now();
    let base_state = BatteryState::sample_14_cell();
    let drift_spec = DriftSpec::default();
    let mut responder = SeplosResponder::new(base_state.clone());
    let mut request = [0u8; MAX_REQUEST];

    loop {
        // Accumulate bytes until the Seplos EOI delimiter (`\r`) or the buffer fills. On
        // overrun, drop this frame and resync on the next `\r` — never index out of bounds, never
        // panic (matches the repo's no-panic-in-logic posture).
        let mut len = 0usize;
        loop {
            let mut byte = [0u8; 1];
            uart.read_byte(&mut byte);
            if len < request.len() {
                request[len] = byte[0];
                len += 1;
            }
            if byte[0] == b'\r' {
                break;
            }
        }

        // Apply time-modulated drift to the battery state before responding.
        let elapsed_ms = boot.elapsed().as_millis() as u64;
        let drifted_state = drift_spec.apply(&base_state, elapsed_ms);
        responder.state = drifted_state;

        match responder.respond(&request[..len]) {
            SimReply::Bytes(reply) => uart.write_all(reply.as_slice()),
            // An unrecognized/malformed request — stay silent, matching a real bus (never
            // fabricate a reply). See `pico_seplos_emu_core::seplos_responder`'s module doc.
            SimReply::Silence => {}
        }
    }
}

/// Heartbeat task: toggle the onboard LED at ~2 Hz (250ms on, 250ms off).
#[embassy_executor::task]
async fn led_heartbeat(mut led: Output<'static>) {
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(250)).await;
        led.set_low();
        Timer::after(Duration::from_millis(250)).await;
    }
}
