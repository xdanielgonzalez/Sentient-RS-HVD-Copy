use crate::prelude::*;

impl lib_profile::ProfileClock for EmbassyClock {
    fn now_micros(&self) -> u64 {
        self.boot.elapsed().as_micros() as u64
    }
}
