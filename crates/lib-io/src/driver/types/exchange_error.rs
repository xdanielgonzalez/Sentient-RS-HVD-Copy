use crate::transport::IoError;
use lib_parse::vendor::ParseError;

/// Error from one exchange: transport failure or parse failure.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    Io(IoError),
    Parse(ParseError),
}
