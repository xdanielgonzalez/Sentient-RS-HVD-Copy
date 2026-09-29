/// Frozen `sv_profile` custody codes (`u32`, `story #355` ABI). `sv_profile` writes one of these
/// into each of its two out-params (`identity_custody_out`, `session_custody_out`) — see
/// `../../veneers/veneer.h`.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvCustody {
    /// Keys live in normal S-world process memory (zeroized on drop). The lowest assurance tier —
    /// what this demo scaffold's software backend always reports.
    SoftwareMemory = 0,
    /// Keys live in the secure world / another on-die hardware isolation domain, referenced by
    /// opaque handle only. Not produced by this scaffold (no `trustzone`-tier backend is wired).
    SecureWorld = 1,
    /// Keys live in a discrete secure element behind its own hardware boundary. Not produced by
    /// this scaffold (no `secure-element`-tier backend is wired).
    SecureElement = 2,
}
