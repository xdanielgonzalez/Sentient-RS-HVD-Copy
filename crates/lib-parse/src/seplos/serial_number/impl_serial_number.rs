use super::prelude::*;

impl SerialNumber {
    /// Decode the serial number from a verified `0xA4` response frame's INFO payload. Returns
    /// [`SeplosError::NonAscii`] on any non-ASCII byte.
    pub fn decode(frame: &super::super::frame::ResponseFrame) -> Result<Self, SeplosError> {
        // Check for non-ASCII bytes
        for &byte in &frame.info {
            if byte > 0x7F {
                return Err(SeplosError::NonAscii);
            }
        }

        // Convert to UTF-8 string
        let s = core::str::from_utf8(&frame.info).map_err(|_| SeplosError::NonAscii)?;

        // Trim surrounding whitespace and strip leading '<'
        let s = s.trim();
        let s = s.strip_prefix('<').unwrap_or(s);

        // Build the bounded string
        let mut result = heapless::String::<MAX_SERIAL_LEN>::new();
        result
            .push_str(s)
            .map_err(|_| SeplosError::TooManyElements)?;

        Ok(SerialNumber(result))
    }
}
