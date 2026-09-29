use lib_security::KeyCustody;

use crate::types::SvCustody;

impl SvCustody {
    /// This variant's frozen ABI code (the `u32` `sv_profile` writes into its out-params).
    pub const fn code(self) -> u32 {
        self as u32
    }
}

/// Map a [`KeyCustody`] tier to the frozen `sv_profile` out-param code. Total: every `KeyCustody`
/// variant has an exact [`SvCustody`] counterpart (the two enums were designed 1:1 for this ABI).
pub const fn key_custody_to_custody_code(custody: KeyCustody) -> u32 {
    match custody {
        KeyCustody::SoftwareMemory => SvCustody::SoftwareMemory.code(),
        KeyCustody::SecureWorld => SvCustody::SecureWorld.code(),
        KeyCustody::SecureElement => SvCustody::SecureElement.code(),
    }
}

/// The inverse direction: decode a raw `sv_profile` out-param value back into a [`KeyCustody`].
/// Not required by the frozen ABI spec (no veneer consumes this direction in this scaffold), but
/// provided for symmetry / a future NS-side profile reader. `None` for any code outside the three
/// frozen values — never guesses a tier for an unrecognized code.
pub const fn custody_code_to_key_custody(code: u32) -> Option<KeyCustody> {
    match code {
        c if c == SvCustody::SoftwareMemory.code() => Some(KeyCustody::SoftwareMemory),
        c if c == SvCustody::SecureWorld.code() => Some(KeyCustody::SecureWorld),
        c if c == SvCustody::SecureElement.code() => Some(KeyCustody::SecureElement),
        _ => None,
    }
}
