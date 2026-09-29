use super::prelude::*;
use super::types::SessionSlot;

impl<R: Rng> SoftwareSessionStore<R> {
    /// Construct an empty store. `rng` is drawn from ONLY at
    /// [`establish`](SessionCipher::establish) time, to mint a fresh 256-bit session key —
    /// [`seal`](SessionCipher::seal)/[`open`](SessionCipher::open) need no entropy (the nonce is a
    /// deterministic per-key counter, never randomly drawn).
    pub fn new(rng: R) -> Self {
        SoftwareSessionStore {
            rng,
            keys: HVec::new(),
        }
    }

    fn get_mut(&mut self, handle: KeyHandle) -> Result<&mut SessionSlot> {
        self.keys
            .get_mut(handle.0 as usize)
            .ok_or(SecurityError::KeyNotFound)
    }
}

impl<R: Rng> core::fmt::Debug for SoftwareSessionStore<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `aes_gcm::Aes256Gcm` does not implement `Debug` at all (it holds an expanded AES key
        // schedule + running GHASH state) — print no fields rather than deriving something that
        // would need to leak them.
        f.debug_struct("SoftwareSessionStore")
            .finish_non_exhaustive()
    }
}

impl<R: Rng> SessionCipher for SoftwareSessionStore<R> {
    fn custody(&self) -> KeyCustody {
        // Mandatory honesty accessor: software session keys are never hardware-backed.
        KeyCustody::SoftwareMemory
    }

    fn establish(&mut self) -> Result<KeyHandle> {
        // The key is born HERE, from this crate's `Rng` seam — never accepted as an argument, never
        // returned as bytes.
        let mut key_bytes = [0u8; 32];
        self.rng.fill(&mut key_bytes)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
        // Scrub the transient stack copy of the raw key bytes ourselves — this crate's own
        // `zeroize` dependency, not `aes-gcm`'s (which only zeroizes ITS OWN internal derivation
        // buffer with the "zeroize" feature; see this crate's Cargo.toml comment on that feature).
        key_bytes.zeroize();

        // Arena capacity, not key material, is the failure mode here — mirrors
        // `SoftwareKeyStore::push`.
        let handle = KeyHandle(u32::try_from(self.keys.len()).map_err(|_| SecurityError::Backend)?);
        self.keys
            .push(SessionSlot {
                cipher,
                nonce_counter: 0,
            })
            .map_err(|_| SecurityError::Backend)?;
        Ok(handle)
    }

    fn seal(&mut self, handle: KeyHandle, aad: &[u8], buf: &mut [u8], len: usize) -> Result<usize> {
        // Fail closed BEFORE touching `buf`: refuse an output that cannot possibly fit the framing.
        let sealed_len = len
            .checked_add(NONCE_LEN + TAG_LEN)
            .ok_or(SecurityError::BufferTooSmall)?;
        if buf.len() < sealed_len {
            return Err(SecurityError::BufferTooSmall);
        }

        let slot = self.get_mut(handle)?;

        // Draw the next nonce from THIS key's monotonic counter — the caller never supplies or
        // sees a nonce value it could reuse. Encode it big-endian into the low 8 bytes of the
        // 12-byte nonce (the high 4 bytes are always zero); advancing before any fallible step
        // below means a retried `seal` on error never reuses a nonce.
        let counter = slot.nonce_counter;
        slot.nonce_counter = counter.checked_add(1).ok_or(SecurityError::Backend)?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes[4..].copy_from_slice(&counter.to_be_bytes());

        // Relocate the plaintext to make room for the nonce prefix, then encrypt it in place.
        buf.copy_within(0..len, NONCE_LEN);
        buf[..NONCE_LEN].copy_from_slice(&nonce_bytes);

        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = &mut buf[NONCE_LEN..NONCE_LEN + len];
        let tag = slot
            .cipher
            .encrypt_in_place_detached(nonce, aad, ciphertext)
            .map_err(|_| SecurityError::Backend)?;
        buf[NONCE_LEN + len..sealed_len].copy_from_slice(&tag);

        Ok(sealed_len)
    }

    fn open(&mut self, handle: KeyHandle, aad: &[u8], buf: &mut [u8], len: usize) -> Result<usize> {
        // A `len` too short to even hold the framing (or longer than the buffer actually backing
        // it) describes malformed INPUT, not an undersized output buffer.
        if len < NONCE_LEN + TAG_LEN || len > buf.len() {
            return Err(SecurityError::Malformed);
        }
        let plaintext_len = len - NONCE_LEN - TAG_LEN;

        let slot = self.get_mut(handle)?;

        let mut nonce_bytes = [0u8; NONCE_LEN];
        nonce_bytes.copy_from_slice(&buf[..NONCE_LEN]);
        let mut tag_bytes = [0u8; TAG_LEN];
        tag_bytes.copy_from_slice(&buf[NONCE_LEN + plaintext_len..len]);

        let nonce = Nonce::from_slice(&nonce_bytes);
        let tag = Tag::from_slice(&tag_bytes);
        let ciphertext = &mut buf[NONCE_LEN..NONCE_LEN + plaintext_len];

        match slot
            .cipher
            .decrypt_in_place_detached(nonce, aad, ciphertext, tag)
        {
            Ok(()) => {
                // Move the now-decrypted plaintext down to buf[..plaintext_len].
                buf.copy_within(NONCE_LEN..NONCE_LEN + plaintext_len, 0);
                Ok(plaintext_len)
            }
            Err(_) => {
                // `aes-gcm`'s `decrypt_in_place_detached` computes and constant-time-compares the
                // GHASH tag over the (untouched) ciphertext BEFORE running the CTR-XOR decrypt step
                // — confirmed by reading the crate source — so `buf` is already left as ciphertext,
                // never partially decrypted, on a tag mismatch. This store zeroes the whole framed
                // region anyway, as defense-in-depth: a caller that forgets to check this `Result`
                // must never be able to mistake ANY leftover bytes here (ciphertext or otherwise)
                // for authenticated plaintext.
                buf[..len].fill(0);
                Err(SecurityError::VerificationFailed)
            }
        }
    }
}
