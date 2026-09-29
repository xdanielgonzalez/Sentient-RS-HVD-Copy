use super::prelude::*;

/// The AAD bound into every `seal`/`open` call. Empty for now: at this pipeline position the
/// stage sees only the opaque [`CryptoUnit`] (data + a key context), not the topic/route it is
/// ultimately headed to or came from, so there is nothing beyond the key selection itself
/// (already carried by the `KeyHandle`) to honestly bind. Binding topic/route AAD requires wiring
/// that context into the boundary message or the stage's construction — deferred to whichever
/// consumer wires this stage into the real pipeline position (TODO(#353)). Empty AAD is a
/// documented, honest choice, not a placeholder pretending to be a binding one.
const AAD: &[u8] = &[];

impl<C> CryptoStage<C> {
    /// Wrap a session cipher into a stage. The cipher owns all key material/handles; this stage
    /// only ever forwards a [`CryptoUnit::context`] as an opaque [`KeyHandle`] — it never sees key
    /// bytes.
    pub fn new(cipher: C) -> Self {
        CryptoStage { cipher }
    }
}

impl<C> CryptoStage<C>
where
    C: SessionCipher,
{
    /// The custody tier of the underlying cipher's keys (secure-world vs. software memory) —
    /// forwarded so a caller can report/assert on it without reaching into the cipher directly.
    pub fn custody(&self) -> KeyCustody {
        self.cipher.custody()
    }
}

impl<C> Stage for CryptoStage<C>
where
    C: SessionCipher,
{
    type In = CryptoUnit;
    // Same boundary-message shape in both directions: direction (seal vs. open) is carried by
    // `CryptoOp` and by which way the pipeline wires this stage, not by transforming the unit's
    // shape here. `op` is passed through unchanged on the output — a sealed unit still reads
    // `op: Seal`, an opened one still reads `op: Open`; this stage does not flip it.
    type Out = CryptoUnit;
    type Error = CryptoStageError;

    async fn process(&mut self, mut input: Self::In) -> Result<Self::Out, Self::Error> {
        let handle = KeyHandle(input.context);
        let len = input.data.len();

        match input.op {
            CryptoOp::Seal => {
                // Seal GROWS the payload (nonce + auth-tag framing beyond the plaintext). Resize
                // the boundary message's `heapless::Vec` out to its full fixed capacity first so
                // `seal` has headroom to write into past `len` — `data.len()` alone is only the
                // *input* length, not the room available for the sealed output.
                input
                    .data
                    .resize(CRYPTO_MAX, 0)
                    .map_err(|_| CryptoStageError::CapacityExceeded)?;
                let sealed_len = self
                    .cipher
                    .seal(handle, AAD, &mut input.data, len)
                    .map_err(CryptoStageError::Cipher)?;
                input.data.truncate(sealed_len);
            }
            CryptoOp::Open => {
                // Open only ever shrinks (strips nonce/tag framing back to plaintext) — no extra
                // headroom needed beyond the sealed record already in `input.data`.
                let opened_len = self
                    .cipher
                    .open(handle, AAD, &mut input.data, len)
                    .map_err(CryptoStageError::Cipher)?;
                input.data.truncate(opened_len);
            }
        }

        Ok(input)
    }
}
