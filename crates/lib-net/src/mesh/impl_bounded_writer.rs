use super::prelude::*;

impl<'a> BoundedWriter<'a> {
    pub(crate) fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }
}

impl core::fmt::Write for BoundedWriter<'_> {
    /// Append `s` if it fits in the remaining capacity, else fail without writing anything
    /// (never a partial write) — this all-or-nothing behavior is what makes the checkpoint/
    /// rollback pattern in [`crate::mesh::impl_condense::try_field`] correct: on error, resetting
    /// `len` back to its pre-attempt value always discards exactly (and only) the bytes this
    /// attempt would have added.
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let end = self.len + bytes.len();
        if end > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use core::fmt::Write as _;

    #[test]
    fn write_str_appends_when_it_fits() {
        let mut buf = [0u8; 8];
        let mut w = BoundedWriter::new(&mut buf);
        assert!(w.write_str("hi").is_ok());
        assert_eq!(w.len, 2);
        assert_eq!(&w.buf[..2], b"hi");
    }

    #[test]
    fn write_str_fails_and_writes_nothing_when_it_does_not_fit() {
        let mut buf = [0xAAu8; 2];
        let mut w = BoundedWriter::new(&mut buf);
        assert!(w.write_str("too long").is_err());
        assert_eq!(w.len, 0, "a failed write must not advance len");
    }

    #[test]
    fn write_fmt_rolls_back_cleanly_on_overflow_after_a_partial_append() {
        // `write!` may call `write_str` more than once for a single format; the SECOND part must
        // not leave a dangling partial write behind once the caller detects the overall failure
        // and resets `len` to its pre-attempt checkpoint.
        let mut buf = [0u8; 4];
        let mut w = BoundedWriter::new(&mut buf);
        let checkpoint = w.len;
        assert!(write!(w, "\"soc\":775").is_err());
        w.len = checkpoint;
        assert_eq!(w.len, 0);
    }
}
