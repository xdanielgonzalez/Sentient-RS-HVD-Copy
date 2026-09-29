/// Whether cryptography runs inside a FIPS-validated module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoValidation {
    /// FIPS 140-3 validated module; approved algorithms only; KAT self-tests on start.
    /// (`feature = "fips"`.)
    Fips140_3,
    /// Standard, non-validated cryptography.
    Standard,
}
