use std::fmt;

use super::prelude::*;

impl fmt::Display for ScanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanStatus::Live => write!(f, "live"),
            ScanStatus::Silent => write!(f, "silent"),
            ScanStatus::Error(detail) => write!(f, "error ({detail})"),
        }
    }
}
