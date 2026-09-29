//! Bring-up + render-loop helpers for the OLED FeatherWing UI leg of `main` (WP5): SSD1306 driver
//! over its own I2C1 bus (GP2/GP3, [`build_display`]), a render-on-change display task, and a
//! debounced button task driving `Screen::next`/`prev`. Compiled only under feature `ui` (default
//! OFF). Singly owned, not shared: the INA3221 leg (`boot_i2c.rs`, feature `power`) sits on the
//! different I2C0 bus (GP4/GP5).
//!
//! Screen data rides [`ns::ui::UiSnapshot`] over [`crate::UI_SNAPSHOT_WATCH`] (an
//! `embassy_sync::watch::Watch`, not a `Channel`, so a producer never stalls); the current
//! [`ns::ui::Screen`] rides [`crate::UI_CURRENT_SCREEN`], a plain `AtomicU8`.
//!
//! Known gap: Network/Meshtastic/Battery screen fields have no producer wired yet and stay at
//! `UiSnapshot::default()` until a later wave threads them through.

use embassy_futures::select::{select3, Either3};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{Config as I2cConfig, I2c};
use embassy_rp::peripherals::{I2C1, PIN_2, PIN_24, PIN_25, PIN_27, PIN_3, PIN_6};
use embassy_rp::Peri;
use embassy_time::{block_for, Duration, Instant, Ticker, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use ssd1306::prelude::{DisplayConfigAsync, DisplayRotation, DisplaySize128x32};
use ssd1306::{I2CDisplayInterface, Ssd1306Async};

use ns::ui::{Screen, ScreenView, UiSnapshot};

use crate::types::{Rp2350Display, Rp2350I2c1Bus};

/// Debounce window after a falling edge (WP5 task note: 20-50ms). 30ms is the middle of that band.
const BUTTON_DEBOUNCE_MS: u64 = 30;

/// Render-loop cadence (WP5 task note: a `Ticker` at 250-500ms). 250ms gives snappy button feedback
/// without the ~11ms/400kHz flush eating meaningfully into core0's executor budget.
const RENDER_PERIOD_MS: u64 = 250;

/// Bring up I2C1 = GP2 (SDA) / GP3 (SCL) in async/interrupt-driven mode — the Feather RP2350's
/// FIXED I2C1 pins (see the Feather pinout ground-truth note; also the STEMMA QT connector's bus) —
/// directly owned by this leg alone (no shared-bus wrapper; see this module's own doc comment for
/// why), then wrap it in an `ssd1306` async driver at its default I2C address (`0x3C` — no
/// collision with the INA3221's `0x40`, see the WP4 task note's "addressing" observation), buffered
/// graphics mode, 128x32 (Adafruit PID 2900). Does NOT call `.init()` — [`display_task`] does that
/// once, inside its own fail-open probe.
pub(crate) fn build_display(
    i2c1: Peri<'static, I2C1>,
    sda: Peri<'static, PIN_2>,
    scl: Peri<'static, PIN_3>,
    rst: Peri<'static, PIN_6>,
) -> Rp2350Display {
    // Hardware-reset the SSD1306 (RST on GP6 / Feather silk "D6") BEFORE init. The controller ACKs
    // I2C and every flush returns OK without this, but its display circuit / charge-pump never
    // starts — the glass stays dark despite "first frame flushed OK". Pulse RST low (>3µs per the
    // datasheet; 10ms here to be safe on a slow/flaky bench supply), release high, settle, then
    // `forget` the Output so Drop doesn't release the pin — RST must stay driven high for the run.
    let mut reset = Output::new(rst, Level::High);
    block_for(Duration::from_millis(10));
    reset.set_low();
    block_for(Duration::from_millis(10));
    reset.set_high();
    block_for(Duration::from_millis(10));
    core::mem::forget(reset);

    let mut config = I2cConfig::default();
    // HIL bring-up: the OLED is on flying leads tapped at the bare SDA/SCL pads, so the
    // FeatherWing's own 2.2 kOhm external pull-ups are NOT on the bus. Fall back to the RP2350's
    // internal pull-ups (~50-80 kOhm, weak) and drop to 100 kHz Standard Mode — weak pull-ups
    // cannot meet Fast Mode rise times, so 400 kHz with them would fail intermittently or not at
    // all. Revert both when the display is wired through the wing's own pads (external pull-ups).
    config.frequency = 100_000;
    config.sda_pullup = true;
    config.scl_pullup = true;

    // NB: no blocking I2C probe here — this runs inline in `main`'s boot, and a `write`/`write_read`
    // on a stuck I2C1 would hang the whole boot. `display_task` does the (retrying, fail-open) init
    // instead. The controller talks-or-not is reported there, and the all-pixels-ON self-test proves
    // whether the glass actually lights.
    let i2c: Rp2350I2c1Bus = I2c::new_async(i2c1, scl, sda, crate::Irqs, config);
    let interface = I2CDisplayInterface::new(i2c);
    Ssd1306Async::new(interface, DisplaySize128x32, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode()
}

/// Which `u8` [`crate::UI_CURRENT_SCREEN`] holds for each [`Screen`] variant — a plain enum<->u8
/// mapping (not a method on `Screen` itself: that type lives in the `ns` lib crate, outside this
/// wave's owned-file list, per its own "frozen contract for the wiring agent" doc comment).
fn screen_to_u8(s: Screen) -> u8 {
    match s {
        Screen::Network => 0,
        Screen::Meshtastic => 1,
        Screen::Battery => 2,
        Screen::About => 3,
    }
}

/// Inverse of [`screen_to_u8`]. Any value outside `0..=3` (never actually stored, but defensive
/// against a future encoding slip) falls back to `Screen::Network`.
fn screen_from_u8(v: u8) -> Screen {
    match v {
        1 => Screen::Meshtastic,
        2 => Screen::Battery,
        3 => Screen::About,
        _ => Screen::Network,
    }
}

#[cfg(feature = "defmt")]
static WARNED_ONCE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
#[cfg(feature = "defmt")]
static DREW_ONCE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);


/// The display render task: owns the SSD1306 driver + the boot instant (for `uptime_secs`).
///
/// **Fail-open**: if `.init()` never succeeds (display absent/mis-wired), the task returns early
/// rather than looping forever against a chip that isn't there — same posture as the W5500 leg,
/// just simpler (nothing else depends on this leg succeeding).
#[embassy_executor::task]
pub(crate) async fn display_task(mut display: Rp2350Display) {
    #[cfg(feature = "defmt")]
    defmt::info!("ui: display_task entered");
    // Retry init instead of giving up after one shot: the OLED can share a finicky bench supply
    // (the INA3221's Wago rail) that hasn't settled at boot, and a loose flying lead may seat a
    // moment later. ~20 attempts × 250ms ≈ a 5s window per boot before fail-open exit.
    const INIT_ATTEMPTS: u32 = 20;
    let mut ok = false;
    for attempt in 0..INIT_ATTEMPTS {
        if display.init().await.is_ok() {
            ok = true;
            #[cfg(feature = "defmt")]
            defmt::info!("ui: SSD1306 init OK on attempt {=u32}", attempt + 1);
            break;
        }
        #[cfg(feature = "defmt")]
        if attempt == 0 {
            defmt::warn!("ui: SSD1306 init failed — retrying up to {=u32}x", INIT_ATTEMPTS);
        }
        Timer::after(Duration::from_millis(250)).await;
    }
    if !ok {
        #[cfg(feature = "defmt")]
        defmt::warn!("ui: SSD1306 init failed after retries (absent/mis-wired) — display task exiting");
        return;
    }

    // Boot self-test: drive EVERY pixel on for ~1s. This separates "panel dark" (power/size/contrast
    // fault) from "panel lit but text mis-rendered" — the flush succeeding over I2C says nothing about
    // whether the glass actually lights. If this all-white flash is invisible, the controller talks
    // but the panel isn't displaying (wrong DisplaySize for the module, missing charge-pump/Vcc, or
    // contrast) — NOT an I2C problem.
    let _ = display.clear(BinaryColor::On);
    let all_on_flushed = display.flush().await.is_ok();
    #[cfg(feature = "defmt")]
    defmt::info!(
        "ui: boot self-test — all-pixels-ON flush {=bool} (watch the glass: white for ~1s)",
        all_on_flushed
    );
    Timer::after(Duration::from_millis(1000)).await;

    let boot = Instant::now();
    let mut last_drawn: Option<(UiSnapshot, Screen)> = None;
    let mut ticker = Ticker::every(Duration::from_millis(RENDER_PERIOD_MS));

    loop {
        ticker.next().await;

        let mut snap = crate::UI_SNAPSHOT_WATCH.try_get().unwrap_or_default();
        snap.uptime_secs = boot.elapsed().as_secs() as u32;
        snap.fw_version = env!("CARGO_PKG_VERSION");
        let screen =
            screen_from_u8(crate::UI_CURRENT_SCREEN.load(core::sync::atomic::Ordering::Relaxed));

        if last_drawn == Some((snap, screen)) {
            continue; // render-on-change: nothing to redraw this tick
        }

        let _ = display.clear(BinaryColor::Off);
        let rendered = screen.render(&mut display, &snap).is_ok();
        let flushed = if rendered { display.flush().await.is_ok() } else { false };
        if rendered && flushed {
            last_drawn = Some((snap, screen));
            #[cfg(feature = "defmt")]
            if !DREW_ONCE.swap(true, core::sync::atomic::Ordering::Relaxed) {
                defmt::info!("ui: first frame flushed OK");
            }
        } else {
            // HIL bring-up: a silent flush failure is indistinguishable from a blank screen, so
            // report it ONCE (a 512-byte flush every tick would otherwise spam RTT). The usual
            // cause is I2C bus weakness on a long burst — short init writes ACK, the full
            // framebuffer flush does not (e.g. relying on the RP2350's ~50-80k internal pull-ups
            // instead of real 2.2-4.7k externals).
            #[cfg(feature = "defmt")]
            if !WARNED_ONCE.swap(true, core::sync::atomic::Ordering::Relaxed) {
                defmt::warn!(
                    "ui: render={=bool} flush={=bool} — display init OK but frame not delivered",
                    rendered,
                    flushed
                );
            }
        }
        // A render or flush failure just retries next tick — never held against the primary
        // mission (same fail-open posture as the rest of this leg).
    }
}

/// The button task: owns all three `Input` pins and races their falling edges via `select3`
/// (mirrors `boot_mesh::mesh_radio_task`'s own `select`-not-split shape). Button A = `Screen::prev`,
/// button C = `Screen::next` (WP5 task note's suggested mapping); button B = select/no-op — still
/// debounced so a bouncing contact doesn't retrigger the OTHER two waits prematurely relative to
/// its own edge, but otherwise does nothing (the note leaves B's action to a later wave).
///
/// **Only button B has an onboard 100kΩ pull-up** (added for the ESP8266) — `Pull::Up` on ALL three
/// is what makes A and C fire at all (WP5 task note's single most likely bring-up bug); it is
/// harmless/redundant on B (its onboard pull-up and the RP2350's internal one simply parallel).
#[embassy_executor::task]
pub(crate) async fn button_task(
    pin_a: Peri<'static, PIN_24>,
    pin_b: Peri<'static, PIN_25>,
    pin_c: Peri<'static, PIN_27>,
) -> ! {
    let mut a = Input::new(pin_a, Pull::Up);
    let mut b = Input::new(pin_b, Pull::Up);
    let mut c = Input::new(pin_c, Pull::Up);

    loop {
        let fired = select3(
            a.wait_for_falling_edge(),
            b.wait_for_falling_edge(),
            c.wait_for_falling_edge(),
        )
        .await;

        match fired {
            Either3::First(()) => advance_screen(Screen::prev),
            Either3::Second(()) => {} // B = select/no-op this wave
            Either3::Third(()) => advance_screen(Screen::next),
        }

        Timer::after(Duration::from_millis(BUTTON_DEBOUNCE_MS)).await;
    }
}

/// Apply `f` to the current screen and store the result. Single writer (this is the only task that
/// ever stores into `UI_CURRENT_SCREEN`), so a plain load-then-store is race-free — no
/// `fetch_update`/CAS loop needed.
fn advance_screen(f: impl FnOnce(Screen) -> Screen) {
    let current =
        screen_from_u8(crate::UI_CURRENT_SCREEN.load(core::sync::atomic::Ordering::Relaxed));
    crate::UI_CURRENT_SCREEN.store(
        screen_to_u8(f(current)),
        core::sync::atomic::Ordering::Relaxed,
    );
}
