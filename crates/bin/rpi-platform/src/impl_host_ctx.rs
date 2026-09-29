use super::prelude::*;

impl SerialCap for HostCtx {
    fn serial_present(&self) -> bool {
        self.serial_present
    }
    fn serial_label(&self) -> &str {
        &self.serial_label
    }
}

impl ConfigCap for HostCtx {
    fn config_check(&self) -> Result<(), &'static str> {
        self.config_valid
    }
}
