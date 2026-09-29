use crate::profile::KeyCustody;

/// [`crate::KeyStore`] over wolfCrypt software keys.
///
/// **Software custody, stated honestly.** [`custody`](WolfKeyStore::custody) returns
/// [`KeyCustody::SoftwareMemory`]: wolfCrypt software keys live in normal-world process memory and
/// are exfiltrable by a compromised host — they are NOT hardware-backed. Hardware custody requires
/// the `trustzone` tier (a secure element / TrustZone / TPM), which this backend does not provide.
///
/// Key-material operations ([`generate`](crate::KeyStore::generate),
/// [`import_private`](crate::KeyStore::import_private), [`export_public`](crate::KeyStore::export_public),
/// [`sign`](crate::KeyStore::sign)) are **not yet bound** and return [`crate::SecurityError::NotImplemented`]
/// (fail closed) rather than faking success — wiring them requires a key arena keyed by
/// [`crate::KeyHandle`] plus confirmed wolfCrypt signing-key types (see the `wolf` module markers).
pub struct WolfKeyStore {
    pub(in crate::backends::wolfcrypt) custody: KeyCustody,
}
