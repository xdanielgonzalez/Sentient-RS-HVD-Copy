use crate::prelude::*;

impl FromRadioParser {
    /// A parser hunting for the first magic byte, with an empty accumulator.
    pub(crate) fn new() -> Self {
        FromRadioParser {
            state: FromRadioState::Magic0,
            buf: [0u8; lib_net::mesh::MAX_RADIO_FRAME],
            len: 0,
            declared_len: 0,
        }
    }

    /// Feed newly-read bytes through the resync state machine, calling `on_frame` once per
    /// complete `FromRadio` message recognized (payload bytes only — the `0x94 0xC3` + length
    /// header is already stripped). Never panics regardless of `bytes`' content: malformed/corrupt
    /// input just resets the search for the next magic, per this type's doc comment.
    pub(crate) fn feed(&mut self, bytes: &[u8], mut on_frame: impl FnMut(&[u8])) {
        for &b in bytes {
            match self.state {
                FromRadioState::Magic0 => {
                    if b == 0x94 {
                        self.state = FromRadioState::Magic1;
                    }
                }
                FromRadioState::Magic1 => {
                    self.state = if b == 0xC3 {
                        FromRadioState::LenHi
                    } else if b == 0x94 {
                        FromRadioState::Magic1
                    } else {
                        FromRadioState::Magic0
                    };
                }
                FromRadioState::LenHi => {
                    self.state = FromRadioState::LenLo(b);
                }
                FromRadioState::LenLo(hi) => {
                    let declared = (usize::from(hi) << 8) | usize::from(b);
                    if declared > self.buf.len() {
                        // Corruption per the firmware's own `MAX_TO_FROM_RADIO_SIZE` check --
                        // resume hunting for the magic rather than trusting this length.
                        self.state = FromRadioState::Magic0;
                    } else if declared == 0 {
                        on_frame(&[]);
                        self.state = FromRadioState::Magic0;
                    } else {
                        self.declared_len = declared;
                        self.len = 0;
                        self.state = FromRadioState::Payload;
                    }
                }
                FromRadioState::Payload => {
                    self.buf[self.len] = b;
                    self.len += 1;
                    if self.len == self.declared_len {
                        on_frame(&self.buf[..self.len]);
                        self.state = FromRadioState::Magic0;
                        self.len = 0;
                    }
                }
            }
        }
    }
}

impl Default for FromRadioParser {
    fn default() -> Self {
        Self::new()
    }
}
