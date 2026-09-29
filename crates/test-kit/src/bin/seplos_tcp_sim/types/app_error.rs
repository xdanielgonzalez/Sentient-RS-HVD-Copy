use std::io;

#[derive(Debug)]
pub(crate) enum AppError {
    Usage(String),
    Io { what: String, source: io::Error },
    Fixture(String),
}
