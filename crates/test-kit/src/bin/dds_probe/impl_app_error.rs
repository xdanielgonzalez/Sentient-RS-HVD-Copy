use super::prelude::*;
use std::fmt;

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Usage(msg) => write!(f, "usage: {msg}"),
            AppError::Dds(e) => write!(f, "dds: {e}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<DdsError> for AppError {
    fn from(e: DdsError) -> Self {
        AppError::Dds(e)
    }
}
