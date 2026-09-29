use super::wolf::wolf;

/// [`crate::Rng`] backed by the wolfCrypt CSPRNG (FIPS DRBG when wolfSSL is built in FIPS mode).
pub struct WolfRng {
    pub(in crate::backends::wolfcrypt) inner: wolf::Rng,
}
