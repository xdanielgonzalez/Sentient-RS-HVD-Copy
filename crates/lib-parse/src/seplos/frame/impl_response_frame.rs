use super::prelude::*;

impl ResponseFrame {
    /// Parse a raw on-wire response (with or without trailing EOI), verifying SOI, the frame
    /// checksum, and the LENGTH LCHKSUM/LENID before returning fields. Returns a typed
    /// [`SeplosError`] on any malformation; never panics.
    ///
    /// # Example
    ///
    /// ```rust
    /// use lib_parse::seplos::frame::ResponseFrame;
    /// use lib_parse::seplos::SeplosError;
    ///
    /// // A live-captured `0x42` telemetry response (DIP address 0).
    /// let raw: &[u8] = b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F36\
    ///     0F360F35060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF8000000\
    ///     79004DDEC8\r";
    ///
    /// let frame = ResponseFrame::parse(raw)?;
    /// assert_eq!(frame.adr, 0);
    /// assert_eq!(frame.rtn, 0);            // 0x00 = success
    /// assert_eq!(frame.lenid, 0x08E);      // 142 INFO hex chars…
    /// assert_eq!(frame.info.len(), 71);    // …decoded to 71 raw bytes
    ///
    /// // One flipped INFO byte fails the frame checksum with a typed error — no panic.
    /// let mut corrupt = raw.to_vec();
    /// corrupt[20] ^= 0x01;
    /// assert!(matches!(
    ///     ResponseFrame::parse(&corrupt),
    ///     Err(SeplosError::BadChecksum { .. })
    /// ));
    /// # Ok::<(), SeplosError>(())
    /// ```
    pub fn parse(raw: &[u8]) -> Result<Self, SeplosError> {
        // Step 1: Check SOI.
        if raw.first() != Some(&super::SOI) {
            return Err(SeplosError::BadSoi);
        }
        let rest = &raw[1..];

        // Step 2: Strip trailing EOI.
        let body = if rest.last() == Some(&super::EOI) {
            &rest[..rest.len().saturating_sub(1)]
        } else {
            rest
        };

        // Step 3: Check minimum length (12 header + 4 checksum).
        if body.len() < 16 {
            return Err(SeplosError::Truncated);
        }

        // Step 3b: Check that INFO (body.len() - 16) is even.
        let info_len = body.len().saturating_sub(16);
        if info_len % 2 != 0 {
            return Err(SeplosError::NonHex);
        }

        // Step 4: Split checksum from body.
        let (sum_region, chk_hex) = body.split_at(body.len() - 4);

        // Compute and verify checksum.
        let expected = super::super::checksum::frame_checksum(sum_region);
        let got = hex_u16_from_pair(chk_hex)?;
        if expected != got {
            return Err(SeplosError::BadChecksum { expected, got });
        }

        // Step 5: Decode ADR (body[2..4]), RTN (body[6..8]), LENGTH (body[8..12]).
        let adr = hex_byte_from_pair(&body[2..4])?;
        let rtn = hex_byte_from_pair(&body[6..8])?;
        let length = hex_u16_from_pair(&body[8..12])?;
        let lenid = length & 0x0FFF;

        // Step 5b: Validate the LENGTH field. The high nibble is the LCHKSUM over the 3 LENID
        // nibbles; the LENID is the INFO hex-char count. A frame that passed the outer checksum can
        // still carry a corrupt/mis-declared LENGTH, so both are checked (they exercise the
        // otherwise-dead BadLengthChecksum / LengthMismatch variants).
        let lchksum = ((length >> 12) & 0xF) as u8;
        if lchksum != super::super::checksum::length_checksum(lenid) {
            return Err(SeplosError::BadLengthChecksum);
        }
        if lenid as usize != info_len {
            return Err(SeplosError::LengthMismatch {
                lenid,
                actual: info_len,
            });
        }

        // Step 6: Check RTN == 0x00.
        if rtn != 0x00 {
            return Err(SeplosError::RtnError(rtn));
        }

        // Step 7: Decode INFO from ASCII-hex pairs.
        let info_hex = &body[12..body.len() - 4];
        let mut info: heapless::Vec<u8, 256> = heapless::Vec::new();
        for chunk in info_hex.chunks(2) {
            if chunk.len() != 2 {
                return Err(SeplosError::NonHex);
            }
            let byte = hex_byte_from_pair(chunk)?;
            if info.push(byte).is_err() {
                return Err(SeplosError::TooManyElements);
            }
        }

        // Step 8: Return parsed frame.
        Ok(ResponseFrame {
            adr,
            rtn,
            lenid,
            info,
        })
    }
}

/// Decode a single nibble from an ASCII hex character.
fn hex_nibble(b: u8) -> Result<u8, SeplosError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        _ => Err(SeplosError::NonHex),
    }
}

/// Decode a single byte from two ASCII hex characters (high nibble first).
fn hex_byte_from_pair(pair: &[u8]) -> Result<u8, SeplosError> {
    if pair.len() != 2 {
        return Err(SeplosError::NonHex);
    }
    let high = hex_nibble(pair[0])?;
    let low = hex_nibble(pair[1])?;
    Ok((high << 4) | low)
}

/// Decode a u16 from four ASCII hex characters (high byte first).
fn hex_u16_from_pair(hex4: &[u8]) -> Result<u16, SeplosError> {
    if hex4.len() != 4 {
        return Err(SeplosError::NonHex);
    }
    let high = hex_byte_from_pair(&hex4[0..2])?;
    let low = hex_byte_from_pair(&hex4[2..4])?;
    Ok(((high as u16) << 8) | (low as u16))
}
