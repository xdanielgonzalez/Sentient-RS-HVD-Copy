use super::prelude::*;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Usage(msg) => write!(f, "usage error: {msg}"),
            AppError::Io { what, source } => write!(f, "I/O error ({what}): {source}"),
            AppError::Fixture(msg) => write!(f, "fixture generation error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}
