//! Impls for [`TlsRefusal`]: the RTT-renderable reason string.
//!
//! Deliberately NOT a `defmt::Format` derive — `defmt` is its own optional feature on this crate,
//! so a derive would tie `tls` to `defmt`. A plain `&'static str` renders through defmt's `{=str}`
//! when `defmt` is on and costs nothing when it is off.

use crate::prelude::*;

impl TlsRefusal {
    /// A short, operator-readable reason, suitable for a defmt `{=str}` argument.
    ///
    /// Every string here describes a REFUSAL. If a future variant's string reads like a success,
    /// that is a bug in the variant, not in this method.
    ///
    /// The sole caller is the defmt-gated refusal log in `main.rs`'s `accept_source` closure, so a
    /// `tls`-without-`defmt` build has no caller at all — this crate has no other logging sink.
    /// The `allow` is scoped to exactly that combination rather than blanket-applied.
    #[cfg_attr(not(feature = "defmt"), allow(dead_code))]
    pub(crate) fn reason(&self) -> &'static str {
        match self {
            TlsRefusal::NoSecureProvider => {
                "no SecureProvider on this chip (ADR 0013 WP-3/WP-4/WP-5) — no plaintext fallback"
            }
        }
    }
}
