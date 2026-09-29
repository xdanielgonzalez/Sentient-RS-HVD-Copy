use crate::profile::KeyCustody;

/// [`crate::SessionCipher`] over wolfCrypt software session keys.
///
/// **Software custody, stated honestly.** [`custody`](WolfSessionStore::custody) returns
/// [`KeyCustody::SoftwareMemory`]: like [`super::WolfKeyStore`], wolfCrypt software keys live in
/// normal-world process memory and are NOT hardware-backed. Hardware custody requires the
/// `trustzone` tier, which this backend does not provide.
///
/// Every other operation ([`establish`](crate::SessionCipher::establish),
/// [`seal`](crate::SessionCipher::seal), [`open`](crate::SessionCipher::open)) is **not yet bound**
/// and returns [`crate::SecurityError::NotImplemented`] (fail closed) rather than faking success —
/// wiring them requires a session-key arena keyed by [`crate::KeyHandle`] plus confirmed wolfCrypt
/// AES-GCM types (see the `wolf` module markers), exactly like [`super::WolfKeyStore`]'s key-material
/// operations.
pub struct WolfSessionStore {
    pub(in crate::backends::wolfcrypt) custody: KeyCustody,
}
