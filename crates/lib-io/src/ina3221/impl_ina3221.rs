use super::prelude::*;

impl<I2C> Ina3221<I2C>
where
    I2C: I2c,
{
    /// Construct a driver at an explicit 7-bit I2C `address` (see [`super::I2C_ADDR_DEFAULT`] and
    /// its `_ALT_*` siblings for the 4 addresses the `A0` pin selects). `rails` maps physical
    /// channel `0..=2` to the [`DeviceRail`] it is wired to (`None` for an unpopulated channel).
    pub fn new(i2c: I2C, address: u8, rails: [Option<DeviceRail>; 3]) -> Self {
        Ina3221 {
            i2c,
            address,
            rails,
        }
    }

    /// Construct a driver at the default address (`A0` tied to `GND`, [`super::I2C_ADDR_DEFAULT`]
    /// = `0x40`) — the bench's wiring (no solder-jumper set).
    pub fn new_default(i2c: I2C, rails: [Option<DeviceRail>; 3]) -> Self {
        Self::new(i2c, I2C_ADDR_DEFAULT, rails)
    }

    /// Reclaim the underlying I2C bus/device, consuming the driver.
    pub fn into_inner(self) -> I2C {
        self.i2c
    }

    /// Force the Configuration register to its documented power-on-reset value
    /// ([`super::CONFIG_RESET`] = `0x7127`: all 3 channels enabled, continuous shunt-and-bus
    /// conversion, 1.1ms conversion time, no averaging) — idempotent, safe to call every boot
    /// regardless of the device's actual prior state.
    pub async fn init(&mut self) -> Result<(), Ina3221Error<I2C::Error>> {
        let [msb, lsb] = CONFIG_RESET.to_be_bytes();
        self.i2c
            .write(self.address, &[REG_CONFIG, msb, lsb])
            .await
            .map_err(Ina3221Error::Bus)
    }

    /// Read the Die ID register (`0xFF`) — compare the result against
    /// [`super::DIE_ID_EXPECTED`] (`0x3220`) as a cheap "is this really an INA3221" bus probe,
    /// e.g. during a fail-open boot-time capability check.
    pub async fn identify(&mut self) -> Result<u16, Ina3221Error<I2C::Error>> {
        let mut buf = [0u8; 2];
        self.i2c
            .write_read(self.address, &[REG_DIE_ID], &mut buf)
            .await
            .map_err(Ina3221Error::Bus)?;
        Ok(u16::from_be_bytes(buf))
    }

    /// Read one physical channel's Shunt-Voltage and Bus-Voltage registers, returning
    /// `(shunt_uv, bus_mv)` already decoded into the raw device units [`DeviceRailReading`]
    /// carries. `index` must be `0..=2`; internal-only (called by [`Ina3221::read_all`] over its
    /// own bounded `rails` array), so it is never exposed to an out-of-range index.
    async fn read_channel(&mut self, index: usize) -> Result<(i32, u16), Ina3221Error<I2C::Error>> {
        let mut buf = [0u8; 2];

        self.i2c
            .write_read(self.address, &[SHUNT_REGS[index]], &mut buf)
            .await
            .map_err(Ina3221Error::Bus)?;
        let shunt_uv = decode_shunt_uv(u16::from_be_bytes(buf));

        self.i2c
            .write_read(self.address, &[BUS_REGS[index]], &mut buf)
            .await
            .map_err(Ina3221Error::Bus)?;
        let bus_mv = decode_bus_mv(u16::from_be_bytes(buf));

        Ok((shunt_uv, bus_mv))
    }

    /// Read every wired channel in this cycle, writing into `out` and returning how many entries
    /// were written (`<= out.len()`, and `<= 3` — the INA3221's physical channel count).
    ///
    /// Written to the **exact same shape** as `spec_app::RailMonitor::read_all` (name, args,
    /// return type) — see this module's top-level doc comment for why the trait itself isn't
    /// implemented here (the board crate's `Ina3221RailMonitor` newtype wrapper implements it
    /// instead). An unwired channel (`rails[i] == None`) is skipped and never written, and —
    /// because this signature has no `Result` to propagate a bus error through — a channel whose
    /// I2C exchange fails is treated exactly the same way: simply not written, rather than
    /// aborting the whole cycle. This is a deliberate fail-open choice, consistent with the W5500
    /// boot leg's fail-open doctrine.
    pub async fn read_all(&mut self, out: &mut [DeviceRailReading]) -> usize {
        let mut n = 0;

        for index in 0..self.rails.len() {
            if n >= out.len() {
                break;
            }
            let Some(rail) = self.rails[index] else {
                continue;
            };
            if let Ok((shunt_uv, bus_mv)) = self.read_channel(index).await {
                out[n] = DeviceRailReading {
                    rail,
                    shunt_uv,
                    bus_mv,
                };
                n += 1;
            }
        }

        n
    }
}
