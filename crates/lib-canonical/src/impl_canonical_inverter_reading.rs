use crate::CanonicalInverterReading;

impl CanonicalInverterReading {
    /// Load is drawing power (`status_bits` bit 4).
    pub const fn load_on(&self) -> bool {
        (self.status_bits >> 4) & 1 == 1
    }
    /// The battery is charging (`status_bits` bit 2).
    pub const fn charging(&self) -> bool {
        (self.status_bits >> 2) & 1 == 1
    }
    /// Charging from solar / PV (`status_bits` bit 1).
    pub const fn solar_charging(&self) -> bool {
        (self.status_bits >> 1) & 1 == 1
    }
    /// Charging from AC / grid (`status_bits` bit 0).
    pub const fn ac_charging(&self) -> bool {
        self.status_bits & 1 == 1
    }
}
