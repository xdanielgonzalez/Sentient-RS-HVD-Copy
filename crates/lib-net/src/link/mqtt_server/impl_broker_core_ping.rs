use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    pub(super) fn handle_pingreq(&mut self, slot: usize) -> Result<(), BrokerError> {
        if !self.is_connected(slot) {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }
        self.stage_or_drop_control(slot, &[PKT_PINGRESP << 4, 0x00]); // PINGRESP: fixed 2 bytes.
        Ok(())
    }
}
