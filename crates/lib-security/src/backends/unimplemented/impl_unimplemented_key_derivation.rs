use super::prelude::*;

impl KeyDerivation for UnimplementedKeyDerivation {
    fn extract(&mut self, _salt: Option<KeyHandle>, _ikm: Option<KeyHandle>) -> Result<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }

    fn expand_label(
        &mut self,
        _secret: KeyHandle,
        _label: &[u8],
        _context: &[u8],
        _len: usize,
    ) -> Result<KeyHandle> {
        Err(SecurityError::NotImplemented)
    }

    fn expand_to_bytes(
        &mut self,
        _secret: KeyHandle,
        _label: &[u8],
        _context: &[u8],
        _out: &mut [u8],
    ) -> Result<usize> {
        // `_out` is deliberately left untouched. Zero-filling it would manufacture an all-zero
        // "derived key" for a caller that ignored the `Err`.
        Err(SecurityError::NotImplemented)
    }
}
