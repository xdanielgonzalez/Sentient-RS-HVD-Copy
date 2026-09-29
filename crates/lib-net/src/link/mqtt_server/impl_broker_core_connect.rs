use super::prelude::*;

impl<const MAX_CLIENTS: usize> BrokerCore<MAX_CLIENTS> {
    pub(super) fn handle_connect(&mut self, slot: usize, body: &[u8]) -> Result<(), BrokerError> {
        let mut pos = 0usize;
        let proto_name = get_str_field(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        let level = get_u8(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        // Connect Flags (byte) + Keep Alive (u16) — parsed for framing correctness even though
        // this minimal broker doesn't act on individual flag bits (no Will/auth/persistent
        // sessions in scope).
        let _connect_flags = get_u8(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        let _keep_alive = get_u16(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        let client_id_len = get_u16(body, &mut pos).map_err(|_| {
            self.mark_closing(slot);
            BrokerError::Malformed
        })?;
        // Client Identifier bytes: parsed for correctness (advances past it), but this broker
        // tracks clients by caller-assigned `slot`, not by MQTT Client Identifier.
        if body.get(pos..pos + client_id_len as usize).is_none() {
            self.mark_closing(slot);
            return Err(BrokerError::Malformed);
        }

        if proto_name != "MQTT" || level != 4 {
            // MQTT 3.1.1 section 3.2.2.2: return code 1 = "unacceptable protocol version". Stage
            // the refusal CONNACK BEFORE dooming the slot, so mark_closing (which doesn't touch
            // `control`) leaves it there for the caller to drain.
            self.stage_connack(slot, 1);
            self.mark_closing(slot);
            return Err(BrokerError::UnsupportedProtocol);
        }

        self.stage_connack(slot, 0);
        if let Some(c) = self.clients.get_mut(slot) {
            c.connected = true;
        }
        Ok(())
    }

    fn stage_connack(&mut self, slot: usize, return_code: u8) {
        let mut buf = [0u8; 4];
        buf[0] = PKT_CONNACK << 4; // fixed header: CONNACK, flags 0000.
        buf[1] = 0x02; // Remaining Length: always 2 (ack flags + return code).
        buf[2] = 0x00; // Session Present always 0 (no persistent sessions in scope).
        buf[3] = return_code;
        self.stage_or_drop_control(slot, &buf);
    }
}
