use core::cell::Cell;
use lib_security::KeyCustody;

/// NS-side [`lib_security::SessionCipher`] riding the frozen S/NS veneer ABI
/// (`sv_session_establish`/`sv_seal`/`sv_open`/`sv_profile`) — see `impl_veneer_session_cipher.rs`
/// for the `extern "C"` bindings, the fail-closed `SV_MAX_PAYLOAD` enforcement, and the custody
/// code mapping.
///
/// **Demo scaffold** (see the crate root doc comment): this proves the `SessionCipher` seam can
/// ride a TrustZone-M veneer boundary — signed-boot / load-time-integrity / IP-protection posture
/// against non-physical attackers only. NO FIPS, NO attestation, NO anti-tamper claim. RP2350 A4
/// stepping only.
///
/// Caches the one-time `sv_profile()` custody query (see `impl_veneer_session_cipher.rs`'s
/// `custody()` doc comment for the per-call-vs-cached decision) — `Cell<Option<KeyCustody>>` gives
/// interior mutability under `&self` (the `SessionCipher::custody` signature takes `&self`, not
/// `&mut self`) without needing a lock: this type is driven by exactly one task (the ns pipeline's
/// seal stage), never shared across cores or tasks.
pub struct VeneerSessionCipher {
    pub(crate) cached_custody: Cell<Option<KeyCustody>>,
}
