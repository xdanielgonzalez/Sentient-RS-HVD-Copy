//! Bring-up helpers for the INA3221 power-monitor's own I2C0 bus (GP4/GP5, feature `power`, WP4):
//! peripheral -> owned async `I2c` -> driver -> poll task, mirroring `boot_seplos.rs`/`boot_w5500.rs`.
//!
//! Measures the adapter's own rail draw (W5500, Heltec) for diagnostics/UI only, on non-TMS
//! `diag/rails` — NOT the project's power source (Seplos BMS via `boot_seplos.rs`; ADR 0012 covers
//! why an earlier mapping onto `DcMeasurementUpdate` was removed).
//!
//! Fail-open like the W5500 leg ([`build_i2c0`] can't fail; [`rail_task`]'s `monitor.read_all` is
//! fail-open per-channel, `lib_io::ina3221`). I2C0 (GP4/GP5) differs from the OLED's I2C1 (GP2/GP3,
//! `boot_ui.rs`) so the legs share no bus; rail polling runs its own `Ticker`, independent of
//! `POLL_HEARTBEAT`.

use embassy_rp::i2c::{Config as I2cConfig, I2c};
use embassy_rp::peripherals::{I2C0, PIN_4, PIN_5};
use embassy_rp::Peri;

use bin_mcu_platform::Ina3221RailMonitor;
use embassy_time::{Duration, Ticker};
use lib_canonical::{DeviceRail, DeviceRailReading};
use lib_io::ina3221::Ina3221;
use lib_mapping::sink::MAX_PAYLOAD;
use spec_app::{Health, RailMonitor, RunConfig, MAX_DEVICE_RAILS};

use crate::types::{Rp2350I2c0Bus, Rp2350RailMonitor};
use crate::Rp2350Sender;

/// Bring up I2C0 = GP4 (SDA) / GP5 (SCL) in async/interrupt-driven mode — the bench's actual
/// INA3221 wiring (a different peripheral than the OLED FeatherWing's fixed I2C1, GP2/GP3 — see
/// this module's own doc comment). Directly owned by this leg alone; no shared-bus wrapper.
#[cfg(feature = "power")]
pub(crate) fn build_i2c0(
    i2c0: Peri<'static, I2C0>,
    sda: Peri<'static, PIN_4>,
    scl: Peri<'static, PIN_5>,
) -> Rp2350I2c0Bus {
    let mut config = I2cConfig::default();
    // HIL bring-up: the INA3221 STEMMA QT breakout is tapped via flying leads on the bench (not
    // plugged into a STEMMA QT connector, which would carry its own external pull-ups), so the bus
    // has NO external pull-ups. Fall back to the RP2350's internal pull-ups (~50-80 kOhm, weak) and
    // drop to 100 kHz Standard Mode — weak pull-ups cannot meet Fast Mode rise times, so 400 kHz
    // with them would fail intermittently or not at all. Revert both if the sensor moves onto a
    // connector that supplies real external pull-ups.
    config.frequency = 100_000;
    config.sda_pullup = true;
    config.scl_pullup = true;

    // NB: keep this leg's bring-up NON-blocking. An earlier boot-time die-ID probe here (`write_read`
    // per address) would HANG the whole boot if the INA's finicky bench supply left I2C0 stuck —
    // stalling W5500 + OLED + net, since `main` awaits this inline. The INA is instead validated
    // non-blockingly by `rail_task`'s first-read log below.
    I2c::new_async(i2c0, scl, sda, crate::Irqs, config)
}

/// Wire a fresh [`Rp2350RailMonitor`] onto `i2c0` at the INA3221's default address (`0x40` — no
/// solder jumper set on the bench; no collision with the SSD1306's `0x3C`, see the WP4 task note's
/// "addressing" observation). Channel 0 = Ethernet (W5500 rail), channel 1 = Radio (Heltec V4
/// rail), channel 2 = Mcu (the RP2350 Feather's own supply — now wired on the bench). Takes `i2c0`
/// BY VALUE — this leg is the sole owner, there is no bus to lend a reference onto.
#[cfg(feature = "power")]
pub(crate) fn build_rail_monitor(i2c0: Rp2350I2c0Bus) -> Rp2350RailMonitor {
    Ina3221RailMonitor(Ina3221::new_default(
        i2c0,
        [
            Some(DeviceRail::Ethernet),
            Some(DeviceRail::Radio),
            Some(DeviceRail::Mcu),
        ],
    ))
}

/// Poll cadence — matches the Seplos leg's own 2s cycle, but on its OWN `Ticker`, independent of
/// the parser loop / `POLL_HEARTBEAT` (see this module's crate-level doc comment).
#[cfg(feature = "power")]
const RAIL_POLL_INTERVAL_MS: u64 = 2_000;

/// The INA3221 rail-monitor poll task: reads both wired rails every [`RAIL_POLL_INTERVAL_MS`],
/// publishes them as a small non-TMS diagnostic JSON payload via
/// [`bin_mcu_platform::publish_rail_draw`] (the SAME render/publish logic
/// `McuPlatform::emit_rail_draw` wraps — see that fn's doc comment), and — under the `ui` feature —
/// mirrors the two rail currents into [`crate::UI_SNAPSHOT_WATCH`] for the About/device screen.
///
/// `tx` is a SECOND sender handle onto the SAME `MQTT_CHANNEL` `run_task`'s own `ChannelPublisher`
/// uses — `embassy_sync::channel::Channel` is a general (not single-producer) channel, so two
/// independent `ChannelPublisher`s safely coexist (unlike `heapless::spsc`, which this codebase
/// never uses for a channel with more than one producer — see the repo's own concurrency doctrine).
///
/// **Fail-open**: `monitor.init()`'s outcome is deliberately ignored — an absent/unresponsive
/// INA3221 still gets `read_all` attempted every cycle forever (which itself degrades to 0 readings
/// written, never a panic or a block — see `lib_io::ina3221`'s own doc comment); this
/// secondary/delight leg must never affect the primary Seplos/MQTT telemetry mission.
///
/// `monitor.read_all(..)` resolves through the [`RailMonitor`] trait (imported into scope here),
/// not the driver's own inherent method directly — closing the seam gap an earlier wave left (see
/// `Rp2350RailMonitor`'s own doc comment for why the impl lives in `bin-mcu-platform`, not here or
/// in `lib-io`).
#[cfg(feature = "power")]
#[embassy_executor::task]
pub(crate) async fn rail_task(
    mut monitor: Rp2350RailMonitor,
    tx: Rp2350Sender,
    // Unused: the diagnostic payload carries no device-id/prefix/timestamp field (unlike the TMS
    // publishes the Seplos leg does) — kept as a parameter so `main.rs`'s spawn call site (which
    // already has a `RunConfig` in hand for the Seplos leg) doesn't need a second code path.
    _cfg: RunConfig,
    payload_buf: &'static mut [u8; MAX_PAYLOAD],
) -> ! {
    let _ = monitor.init().await;

    let mut publisher = bin_mcu_platform::ChannelPublisher::new(tx);
    let mut health = Health::default();

    let blank = DeviceRailReading {
        rail: DeviceRail::Spare,
        shunt_uv: 0,
        bus_mv: 0,
    };
    let mut readings = [blank; MAX_DEVICE_RAILS];

    #[cfg(feature = "defmt")]
    defmt::info!("ina3221: rail_task entered (init done)");

    #[cfg(feature = "defmt")]
    let mut probed = false;

    let mut ticker = Ticker::every(Duration::from_millis(RAIL_POLL_INTERVAL_MS));
    loop {
        ticker.next().await;

        let n = monitor.read_all(&mut readings).await;

        // HIL bring-up diagnostic (defmt-only): report the FIRST poll's real outcome once. Unlike
        // the I2C0 address scan (unreliable at weak internal pull-ups — it false-ACKs), this is a
        // genuine INA3221 protocol read: n>0 with plausible bus_mv ⇒ the chip is wired and talking.
        #[cfg(feature = "defmt")]
        if !probed {
            probed = true;
            if n == 0 {
                defmt::warn!("ina3221: first read returned 0 rails — chip absent/unresponsive on GP4/GP5");
            } else {
                defmt::info!("ina3221: first read OK — {=usize} rail(s)", n);
                for r in &readings[..n] {
                    defmt::info!(
                        "ina3221: rail shunt_uv={=i32} bus_mv={=u16}",
                        r.shunt_uv,
                        r.bus_mv
                    );
                }
            }
        }

        if n == 0 {
            continue; // nothing read this cycle (chip absent/unresponsive) — try again next tick
        }

        bin_mcu_platform::publish_rail_draw(
            &mut publisher,
            &mut health,
            &readings[..n],
            &mut payload_buf[..],
        );

        #[cfg(feature = "ui")]
        mirror_into_ui_snapshot(&readings[..n]);
    }
}

/// Mirror the two INA3221 rail currents into [`crate::UI_SNAPSHOT_WATCH`] (WP5's About/device
/// screen — `rail_eth_ma`/`rail_radio_ma`) via `send_modify`, which merges into whatever the watch
/// currently holds (seeding a default first) rather than overwriting fields OTHER producers own —
/// see `boot_ui.rs`'s module doc for which `UiSnapshot` fields this wave actually has a producer
/// for.
///
/// Amperage is derived the SAME way `bin_mcu_platform::publish_rail_draw`'s own diagnostic-JSON
/// renderer does (Ohm's law over the Adafruit INA3221 breakout's fixed 0.05 Ω shunts —
/// PROVISIONAL, unverified against the physical board) rather than duplicating that constant via a
/// shared dependency: milliamps = microvolts / (shunt milliohms). `UiSnapshot::rail_eth_ma`'s own
/// doc comment already documents this "pre-mapped to mA" convention.
#[cfg(all(feature = "power", feature = "ui"))]
fn mirror_into_ui_snapshot(readings: &[DeviceRailReading]) {
    // 0.05 ohm shunt -> milliamps = microvolts / 50.
    const SHUNT_MILLIOHMS: i32 = 50;

    crate::UI_SNAPSHOT_WATCH.sender().send_modify(|opt| {
        let snap = opt.get_or_insert_with(ns::ui::UiSnapshot::default);
        for r in readings {
            let ma = (r.shunt_uv / SHUNT_MILLIOHMS) as i16;
            match r.rail {
                DeviceRail::Ethernet => snap.rail_eth_ma = ma,
                DeviceRail::Radio => snap.rail_radio_ma = ma,
                // The MCU rail has no `UiSnapshot` field yet (only eth/radio are shown on the
                // About screen); it still reaches the `diag/rails` MQTT payload. Add a UI field if
                // the device screen should display it.
                DeviceRail::Mcu => {}
                DeviceRail::Spare => {}
            }
        }
    });
}
