use super::prelude::*;

impl From<IoError> for ExchangeError {
    fn from(e: IoError) -> Self {
        ExchangeError::Io(e)
    }
}

impl From<ParseError> for ExchangeError {
    fn from(e: ParseError) -> Self {
        ExchangeError::Parse(e)
    }
}
