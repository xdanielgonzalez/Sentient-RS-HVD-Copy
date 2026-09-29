use super::prelude::*;

impl<const MAX_CLIENTS: usize> Default for BrokerCore<MAX_CLIENTS> {
    fn default() -> Self {
        Self::new()
    }
}
