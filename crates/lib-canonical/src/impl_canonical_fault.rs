use crate::CanonicalFault;

impl CanonicalFault {
    /// True if any protection or warning flag is set.
    ///
    /// ```
    /// use lib_canonical::CanonicalFault;
    ///
    /// let clear = CanonicalFault::default();
    /// assert!(!clear.any());
    ///
    /// let tripped = CanonicalFault {
    ///     charge_overcurrent_protection: true,
    ///     ..clear
    /// };
    /// assert!(tripped.any());
    /// ```
    pub fn any(&self) -> bool {
        self.high_voltage_protection
            || self.low_voltage_protection
            || self.high_temperature_charge_protection
            || self.low_temperature_charge_protection
            || self.charge_overcurrent_protection
            || self.discharge_overcurrent_protection
            || self.short_circuit_protection
            || self.mosfet_fault
            || self.cell_balance_warning
            || self.low_capacity_warning
            || self.high_temperature_discharge_warning
            || self.low_temperature_discharge_warning
            || self.low_voltage_warning
            || self.high_voltage_warning
            || self.discharge_current_warning
            || self.charge_current_warning
    }
}
