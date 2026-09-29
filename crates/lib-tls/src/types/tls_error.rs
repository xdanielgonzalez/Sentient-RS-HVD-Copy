use lib_security::SecurityError;

/// A failure from the TLS glue. Fail-closed by construction: **every** variant means the connection
/// is untrusted and must not be treated as secure — there is no "degraded but usable" state, and
/// deliberately no variant a caller could read as a soft warning.
///
/// `#[non_exhaustive]` for the same reason `SecurityError` is: a new failure mode must not silently
/// fall into an existing `match` arm at a call site.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsError {
    /// **The state of this whole crate.** The operation is a skeleton from ADR 0013 WP-3 and has no
    /// implementation behind it. It is returned instead of a plausible-looking success so that a
    /// premature call site fails loudly at the first handshake rather than authorizing a peer that
    /// was never verified.
    ///
    /// Grepping for this variant enumerates exactly what WP-3 has left to build.
    Unimplemented,
    /// The handshake failed — bad certificate, an SPKI that is not pinned, a protocol mismatch, or
    /// a received alert. The connection is dropped; ADR 0013 §6 routes this through the existing
    /// `accept_source` closure returning `None`, so no new error path enters `lib-net`.
    Handshake,
    /// A record failed to decrypt or authenticate, or arrived out of sequence.
    Protocol,
    /// A caller-supplied buffer was too small. Bounded, no allocation — the caller retries with a
    /// larger buffer; this is rustls's `InsufficientSizeError` surfaced through our own type.
    BufferTooSmall,
    /// The peer authenticated but is not authorized for what it attempted. Distinct from
    /// [`TlsError::Handshake`] because it is an *authorization* failure over an established
    /// identity — the decision ADR 0013 §6/WP-7 keys on the resolved `AnchorId`.
    NotAuthorized,
    /// The `lib-security` seam refused, and this is the reason it gave. Carried through rather than
    /// flattened so a custody or approved-algorithm refusal is still legible at the TLS layer.
    Security(SecurityError),
}
