//! Async **INA3221** triple-channel, high-side shunt/bus-voltage monitor driver, gated behind the
//! `ina3221` feature.
//!
//! **This measures the adapter board's OWN rail draw (W5500, Heltec) — engineering
//! self-telemetry for diagnostics + the on-device UI. It is NOT the project's power source.** The
//! project's power telemetry (pack voltage/current/SOC) comes from the Seplos BMS and is mapped to
//! TMS in `lib_mapping::transform`. Readings from this driver never reach TMS at all — an earlier
//! wave wrongly mapped them onto the same `DcMeasurementUpdate.internal_measurement` field the
//! pack uses; that mapping was removed (see ADR 0012). This driver's output is published on a
//! diagnostic-only, non-TMS topic (`bin-mcu-platform`'s `diag/rails`).
//!
//! **Embassy-version-free by construction**, exactly like [`crate::embassy_uart`] (see that
//! module's doc comment): [`Ina3221`] is generic over `I2C: embedded_hal_async::i2c::I2c` only —
//! no `embassy-*` crate, no digital/GPIO trait, no delay trait. The board crate supplies whichever
//! concrete I2C peripheral/bus-sharing wrapper it likes (on the bench, an `embedded-hal-bus`
//! (`async`) `I2cDevice` sharing I2C1 with the OLED FeatherWing).
//!
//! # Why hand-rolled, not the `ina3221` crate
//!
//! The published `ina3221` crate is 0.4.5 (last released 2023-02-20) and depends on
//! `embedded-hal ^1.0.0-alpha.9` — a pre-1.0 alpha that does not compose with `embedded-hal 1.0`
//! or embassy. Writing this driver is ~6 registers and buys an async impl that shares the
//! display's bus, host-testability against a mock I2C, and no stale alpha dependency in a
//! defense-adjacent SBOM.
//!
//! # Datasheet ground truth
//!
//! Every register address, bit layout, LSB scale, and reset value below is taken from the
//! Texas Instruments **INA3221** datasheet, literature number **SBOS576C** (May 2012 – revised
//! September 2025), <https://www.ti.com/lit/ds/symlink/ina3221.pdf> — table/section numbers are
//! cited per constant/function. This part is **not** the INA219 (different register map, different
//! LSB scales); do not carry over values from the INA219 research note.
//!
//! # This crate does NOT implement `spec_app::RailMonitor` (avoids a dependency cycle)
//!
//! [`read_all`] is written to the **exact same shape** as `spec_app::RailMonitor::read_all`
//! (`async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize`) so that implementing
//! the trait downstream is a one-line forward. It is an inherent method here, not a trait impl,
//! because `spec-app` already depends on `lib-io` (`Platform` names
//! `lib_io::transport::SerialTransport`) — `lib-io -> spec-app` would close a cycle. The actual
//! `impl RailMonitor for Ina3221RailMonitor<I2C>` (a local newtype wrapper, needed because
//! `Ina3221<I2C>` itself is foreign there too — see that type's own doc comment) lives in
//! `bin-mcu-platform`, the board crate that already depends on both (the same place
//! `spec_app::Platform` is implemented — never in `lib-io` itself; see
//! `crates/lib-io/Cargo.toml`'s `ina3221` feature comment).
//!
//! # Example — the seam in miniature
//!
//! ```
//! use embedded_hal_async::i2c::{ErrorType, I2c, Operation};
//! use lib_canonical::{DeviceRail, DeviceRailReading};
//! use lib_io::ina3221::Ina3221;
//!
//! /// A minimal in-memory I2C stand-in: no hardware, no wire.
//! struct FakeBus;
//!
//! impl ErrorType for FakeBus {
//!     type Error = core::convert::Infallible;
//! }
//!
//! impl I2c for FakeBus {
//!     async fn transaction(
//!         &mut self,
//!         _address: u8,
//!         operations: &mut [Operation<'_>],
//!     ) -> Result<(), Self::Error> {
//!         // Every register reads back as 0 (a real device never asserted, but the DECODE path
//!         // is what this example proves, not real telemetry).
//!         for op in operations {
//!             if let Operation::Read(buf) = op {
//!                 buf.fill(0);
//!             }
//!         }
//!         Ok(())
//!     }
//! }
//!
//! # fn block_on<F: core::future::Future>(fut: F) -> F::Output {
//! #     let mut fut = core::pin::pin!(fut);
//! #     let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
//! #     loop {
//! #         if let core::task::Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
//! #             return out;
//! #         }
//! #     }
//! # }
//! let rails = [Some(DeviceRail::Ethernet), Some(DeviceRail::Radio), None];
//! let mut monitor = Ina3221::new_default(FakeBus, rails);
//!
//! let blank = DeviceRailReading { rail: DeviceRail::Spare, shunt_uv: 0, bus_mv: 0 };
//! let mut out = [blank; 3];
//! let n = block_on(monitor.read_all(&mut out));
//! assert_eq!(n, 2); // Spare (index 2) is skipped entirely, never written
//! assert_eq!(out[0].rail, DeviceRail::Ethernet);
//! ```

mod impl_ina3221;
mod prelude;
mod types;

pub use types::*;

/// 7-bit I2C target address with `A0` tied to `GND` — the default/no-jumper address (TI INA3221
/// datasheet SBOS576C, §7.5.1.1, Table 7-1: `A0=GND -> 1000000b = 0x40`).
pub const I2C_ADDR_DEFAULT: u8 = 0x40;
/// 7-bit I2C target address with `A0` tied to `VS` (Table 7-1: `1000001b = 0x41`) — the Adafruit
/// INA3221 breakout's solder-jumper alternate address.
pub const I2C_ADDR_ALT_VS: u8 = 0x41;
/// 7-bit I2C target address with `A0` tied to `SDA` (Table 7-1: `1000010b = 0x42`).
pub const I2C_ADDR_ALT_SDA: u8 = 0x42;
/// 7-bit I2C target address with `A0` tied to `SCL` (Table 7-1: `1000011b = 0x43`).
pub const I2C_ADDR_ALT_SCL: u8 = 0x43;

/// Configuration register pointer address (Table 7-3 / Table 7-4).
pub const REG_CONFIG: u8 = 0x00;
/// Channel-1 Shunt-Voltage register pointer address (Table 7-4; §7.6.2.2).
pub const REG_CH1_SHUNT: u8 = 0x01;
/// Channel-1 Bus-Voltage register pointer address (Table 7-4; §7.6.2.3).
pub const REG_CH1_BUS: u8 = 0x02;
/// Channel-2 Shunt-Voltage register pointer address (Table 7-4; §7.6.2.4).
pub const REG_CH2_SHUNT: u8 = 0x03;
/// Channel-2 Bus-Voltage register pointer address (Table 7-4; §7.6.2.5).
pub const REG_CH2_BUS: u8 = 0x04;
/// Channel-3 Shunt-Voltage register pointer address (Table 7-4; §7.6.2.6).
pub const REG_CH3_SHUNT: u8 = 0x05;
/// Channel-3 Bus-Voltage register pointer address (Table 7-4; §7.6.2.7).
pub const REG_CH3_BUS: u8 = 0x06;
/// Die ID register pointer address (Table 7-4; §7.6.2.20) — factory-programmed `0x3220`,
/// distinguishing the part from any other device on the bus.
pub const REG_DIE_ID: u8 = 0xFF;

/// Configuration register power-on-reset value (§7.6.2.1, Table 7-3: `reset = 7127h`): `RST=0`,
/// all 3 channels enabled, averaging = 1 sample, both shunt- and bus-voltage conversion time =
/// 1.1ms, `MODE` = shunt-and-bus continuous. [`Ina3221::init`] writes this value explicitly
/// (rather than relying on the chip's actual POR state) so a warm boot that finds the device
/// left in single-shot or power-down mode by a previous run is put back into a known-good state.
pub const CONFIG_RESET: u16 = 0x7127;

/// Die ID register's factory-programmed content (§7.6.2.20, Table 7-44: `3220h`) — compare
/// against [`Ina3221::identify`]'s return value as a cheap "is this really an INA3221" bus probe.
pub const DIE_ID_EXPECTED: u16 = 0x3220;

/// Shunt-voltage register LSB, in **microvolts** (§6.5 Electrical Characteristics, "1 LSB step
/// size: Shunt voltage 40µV"; corroborated at §7.6.2.2, Table 7-7: "LSB (SD0): 40μV"). Applies to
/// all 3 channels' Shunt-Voltage registers alike.
pub const SHUNT_LSB_UV: i32 = 40;

/// Bus-voltage register LSB, in **millivolts** (§6.5 Electrical Characteristics, "1 LSB step
/// size: Bus voltage 8mV"; corroborated at §7.6.2.3, Table 7-9: "LSB (BD0) = 8mV"). Applies to
/// all 3 channels' Bus-Voltage registers alike.
pub const BUS_LSB_MV: u16 = 8;

/// Shunt-Voltage register addresses, indexed `0..=2` == datasheet "Channel 1".."Channel 3".
pub(crate) const SHUNT_REGS: [u8; 3] = [REG_CH1_SHUNT, REG_CH2_SHUNT, REG_CH3_SHUNT];
/// Bus-Voltage register addresses, indexed `0..=2` == datasheet "Channel 1".."Channel 3".
pub(crate) const BUS_REGS: [u8; 3] = [REG_CH1_BUS, REG_CH2_BUS, REG_CH3_BUS];

/// Decode a raw 16-bit Shunt-Voltage register value into microvolts.
///
/// Per §7.6.2.2 (Table 7-7): bits 15-3 hold `SIGN` + `SD11-0` (a 13-bit two's-complement value,
/// negative meaning reversed current direction); bits 2-0 are reserved and always read `0`. TI's
/// worked example (§7.6.2.2) confirms treating the full 16-bit word as `i16` and arithmetic-
/// shifting right by 3 recovers the signed 13-bit magnitude directly: for `VSHUNT = -80mV`, the
/// documented wire value is `C180h`; `0xC180i16 >> 3 == -2000`, and `-2000 * 40µV == -80,000µV`,
/// matching. Full-scale (`0x7FF8 >> 3 == 4095`; `4095 * 40µV == 163.8mV`) matches the datasheet's
/// stated full-scale range exactly.
pub fn decode_shunt_uv(raw: u16) -> i32 {
    let value = (raw as i16) >> 3;
    i32::from(value) * SHUNT_LSB_UV
}

/// Decode a raw 16-bit Bus-Voltage register value into millivolts.
///
/// Same bit alignment as the shunt registers (§7.6.2.3, Table 7-9): bits 15-3 hold `SIGN` +
/// `BD11-0`, bits 2-0 reserved. The bus-voltage register cannot go negative in practice (there is
/// no reversed-polarity bus voltage), but the `SIGN` bit is architecturally present, so a
/// theoretical negative reading is clamped to `0` rather than wrapping — [`DeviceRailReading`]
/// deliberately stores `bus_mv` as `u16` (unsigned) per its own doc comment.
pub fn decode_bus_mv(raw: u16) -> u16 {
    let value = (raw as i16) >> 3;
    let clamped = value.max(0);
    // 4095 (13-bit max magnitude) * 8mV == 32760, well within u16 — no truncation.
    (i32::from(clamped) as u32 * u32::from(BUS_LSB_MV)) as u16
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use core::convert::Infallible;

    use embedded_hal_async::i2c::{ErrorType, I2c, Operation};
    use lib_canonical::{DeviceRail, DeviceRailReading};

    use super::*;

    // --- pure decode-function tests (no I2C needed at all) ---

    #[test]
    fn decode_shunt_matches_datasheet_worked_example() {
        // TI INA3221 datasheet SBOS576C §7.6.2.2: VSHUNT = -80mV encodes as 0xC180.
        assert_eq!(decode_shunt_uv(0xC180), -80_000);
    }

    #[test]
    fn decode_shunt_full_scale_positive() {
        // Table 7-7: full-scale range = 163.8mV, decimal register value 0x7FF8.
        assert_eq!(decode_shunt_uv(0x7FF8), 163_800);
    }

    #[test]
    fn decode_shunt_zero() {
        assert_eq!(decode_shunt_uv(0x0000), 0);
    }

    #[test]
    fn decode_shunt_small_negative_sign_handling() {
        // -40µV is exactly 1 LSB negative: magnitude 1, two's complement of a 13-bit field
        // shifted into bits 15-3 -> 0xFFF8 (all data bits set, SIGN set, reserved bits 0).
        assert_eq!(decode_shunt_uv(0xFFF8), -40);
    }

    #[test]
    fn decode_bus_matches_power_valid_upper_limit_reset() {
        // Table 7-37/7-38: Power-Valid Upper-Limit reset = 0x2710 = 10.000V; same bit alignment
        // and 8mV LSB as the per-channel Bus-Voltage registers this function decodes.
        assert_eq!(decode_bus_mv(0x2710), 10_000);
    }

    #[test]
    fn decode_bus_full_scale() {
        // Table 7-9: full-scale range = 32.76V, decimal register value 0x7FF8.
        assert_eq!(decode_bus_mv(0x7FF8), 32_760);
    }

    #[test]
    fn decode_bus_clamps_a_theoretical_negative_to_zero() {
        assert_eq!(decode_bus_mv(0xC180), 0);
    }

    #[test]
    fn scale_factor_round_trips_through_the_lsb() {
        // A 150mA load on a 0.05 Ω shunt (the bench's INA3221 shunt value, per WP4's task note)
        // drops 7.5mV = 7500µV; 7500µV / 40µV/LSB = 187.5 LSB, so the nearest representable
        // reading is 187 LSB * 40µV = 7480µV — round-trip through the encode a real device would
        // perform, decoded back by this crate's own function.
        let lsb_count: i16 = 187;
        let raw = (lsb_count << 3) as u16; // reserved bits 2-0 stay 0
        assert_eq!(decode_shunt_uv(raw), 187 * SHUNT_LSB_UV);
        assert_eq!(decode_shunt_uv(raw), 7_480);
    }

    // --- mock I2C: the seam's whole point is host-testability with zero hardware ---

    /// A canned-response I2C stand-in: `regs` holds the current value of every register the
    /// driver might address (indexed by register pointer, `0..=0xFF`), seeded by the test and
    /// mutated by `write`/`transaction` exactly like a real INA3221 would be. Every `(address,
    /// register)` pair issued is recorded for byte-exact assertions (mirrors `mock::MockSerial`'s
    /// write/direction recording, same rationale).
    struct MockI2c {
        regs: [u16; 256],
        target_address: u8,
        /// Every register pointer this mock was asked to READ from, in order.
        reads: heapless::Vec<u8, 32>,
    }

    impl MockI2c {
        fn new(target_address: u8) -> Self {
            MockI2c {
                regs: [0u16; 256],
                target_address,
                reads: heapless::Vec::new(),
            }
        }

        fn set_reg(&mut self, reg: u8, value: u16) {
            self.regs[usize::from(reg)] = value;
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct MockI2cError;

    impl embedded_hal_async::i2c::Error for MockI2cError {
        fn kind(&self) -> embedded_hal_async::i2c::ErrorKind {
            embedded_hal_async::i2c::ErrorKind::Other
        }
    }

    impl ErrorType for MockI2c {
        type Error = MockI2cError;
    }

    impl I2c for MockI2c {
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            if address != self.target_address {
                return Err(MockI2cError);
            }

            // A register-pointer write is always the first operation of any exchange this
            // driver performs (both a plain register write and a write-then-read).
            let Some(Operation::Write(first_write)) = operations.first() else {
                return Err(MockI2cError);
            };
            let reg = first_write[0];

            match operations {
                // write_read(reg): [Write([reg]), Read(buf)] -- fill `buf` MSB-first from `regs`.
                [Operation::Write(_), Operation::Read(buf)] => {
                    let _ = self.reads.push(reg);
                    let value = self.regs[usize::from(reg)];
                    buf.copy_from_slice(&value.to_be_bytes());
                    Ok(())
                }
                // write(reg, msb, lsb): [Write([reg, msb, lsb])] -- store the new value.
                [Operation::Write(bytes)] if bytes.len() == 3 => {
                    let value = u16::from_be_bytes([bytes[1], bytes[2]]);
                    self.regs[usize::from(reg)] = value;
                    Ok(())
                }
                _ => Err(MockI2cError),
            }
        }
    }

    fn blank() -> DeviceRailReading {
        DeviceRailReading {
            rail: DeviceRail::Spare,
            shunt_uv: 0,
            bus_mv: 0,
        }
    }

    #[tokio::test]
    async fn read_all_reads_only_wired_channels_and_skips_spare() {
        let mut bus = MockI2c::new(I2C_ADDR_DEFAULT);
        // Channel 1 (Ethernet): 7500µV shunt (187 LSB) / 5050mV bus.
        bus.set_reg(REG_CH1_SHUNT, (187i16 << 3) as u16);
        bus.set_reg(REG_CH1_BUS, (631i16 << 3) as u16); // 631 * 8mV = 5048mV
                                                        // Channel 2 (Radio): -4000µV shunt (reversed) / 3300mV bus.
        bus.set_reg(REG_CH2_SHUNT, ((-100i16) << 3) as u16); // -100 * 40uV = -4000uV
        bus.set_reg(REG_CH2_BUS, (412i16 << 3) as u16); // 412 * 8mV = 3296mV
                                                        // Channel 3 is Spare (unwired) — its registers are deliberately left non-zero to prove
                                                        // read_all never even issues an I2C exchange for it.
        bus.set_reg(REG_CH3_SHUNT, 0x7FF8);
        bus.set_reg(REG_CH3_BUS, 0x7FF8);

        let rails = [Some(DeviceRail::Ethernet), Some(DeviceRail::Radio), None];
        let mut monitor = Ina3221::new_default(bus, rails);

        let mut out = [blank(); 3];
        let n = monitor.read_all(&mut out).await;

        assert_eq!(n, 2);
        assert_eq!(out[0].rail, DeviceRail::Ethernet);
        assert_eq!(out[0].shunt_uv, 7_480);
        assert_eq!(out[0].bus_mv, 5_048);
        assert_eq!(out[1].rail, DeviceRail::Radio);
        assert_eq!(out[1].shunt_uv, -4_000);
        assert_eq!(out[1].bus_mv, 3_296);

        // Channel 3's registers were never read (no request for either of its two addresses).
        assert!(!monitor.into_inner().reads.contains(&REG_CH3_SHUNT));
    }

    #[tokio::test]
    async fn read_all_respects_a_short_output_slice() {
        let bus = MockI2c::new(I2C_ADDR_DEFAULT);
        let rails = [Some(DeviceRail::Ethernet), Some(DeviceRail::Radio), None];
        let mut monitor = Ina3221::new_default(bus, rails);

        let mut out = [blank(); 1]; // only room for 1, even though 2 rails are wired
        let n = monitor.read_all(&mut out).await;
        assert_eq!(n, 1);
        assert_eq!(out[0].rail, DeviceRail::Ethernet);
    }

    #[tokio::test]
    async fn read_all_treats_a_bus_error_on_one_channel_as_simply_not_written() {
        // The mock's `target_address` mismatch makes EVERY transaction fail — proving read_all
        // never panics or short-circuits on a bus error; it just returns fewer entries. This is
        // the fail-open behavior `read_all`'s frozen `usize`-only signature forces (no `Result`
        // channel exists to propagate a bus error through).
        let bus = MockI2c::new(I2C_ADDR_ALT_VS); // driver will address I2C_ADDR_DEFAULT instead
        let rails = [Some(DeviceRail::Ethernet), Some(DeviceRail::Radio), None];
        let mut monitor = Ina3221::new_default(bus, rails);

        let mut out = [blank(); 3];
        let n = monitor.read_all(&mut out).await;
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn init_writes_the_datasheet_reset_value() {
        let bus = MockI2c::new(I2C_ADDR_DEFAULT);
        let mut monitor = Ina3221::new_default(bus, [None, None, None]);

        monitor.init().await.expect("init writes ok");
        assert_eq!(
            monitor.into_inner().regs[usize::from(REG_CONFIG)],
            CONFIG_RESET
        );
    }

    #[tokio::test]
    async fn identify_reads_back_the_die_id() {
        let mut bus = MockI2c::new(I2C_ADDR_DEFAULT);
        bus.set_reg(REG_DIE_ID, DIE_ID_EXPECTED);
        let mut monitor = Ina3221::new_default(bus, [None, None, None]);

        let id = monitor.identify().await.expect("identify reads ok");
        assert_eq!(id, DIE_ID_EXPECTED);
    }

    #[tokio::test]
    async fn identify_surfaces_a_bus_error() {
        let bus = MockI2c::new(I2C_ADDR_ALT_VS); // address mismatch -> every transaction fails
        let mut monitor = Ina3221::new_default(bus, [None, None, None]);

        let err = monitor.identify().await.unwrap_err();
        assert_eq!(err, Ina3221Error::Bus(MockI2cError));
    }
}
