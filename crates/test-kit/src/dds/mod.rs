//! `lib-dds` test-support helper (WP5 #323) — the DDS analogue of [`crate::mqtt::start_broker`].
//!
//! Unlike the MQTT broker (which this process embeds and owns end-to-end), a DDS participant talks
//! RTPS/UDP over a real network interface — there is no in-process "local broker" to start. This
//! module exists so a test (or the `dds-probe` bin's `subscribe`/`publish` modes — see
//! `src/bin/dds_probe/`) gets a ready-to-use, real [`DdsParticipant`] the same one line
//! `start_broker()` gives for MQTT, instead of re-deriving [`DdsConfig::domain0`] +
//! `lib_dds::dustdds::build_participant` boilerplate every call site (the pattern
//! `crates/lib-dds/tests/dds_loopback.rs` already hand-rolls once — this is that call, reusable).
//!
//! The HEAVY interop asserting (USER_DATA, late-joiner TRANSIENT_LOCAL replay, Reply KEEP_LAST
//! depth, CycloneDDS decode) lives in the containerized python probe
//! (`deploy/dds-probe/subscribe_assert.py`), NOT here — this is plumbing, not an oracle.
//!
//! Gated at the `mod dds;` declaration in `lib.rs` (`#[cfg(feature = "dds-probe")]`) — not repeated
//! here as an inner attribute (clippy `duplicated_attributes`).

use lib_dds::dustdds::build_participant;
use lib_dds::{DdsConfig, DdsError, DdsParticipant};

/// Build a real [`DdsParticipant`] on domain 0 (MIL-STD-3071) over `iface`'s custom UDP/IPv6
/// transport ([`DdsConfig::domain0`] — loopback unicast advertisement, same-host default).
///
/// Fallible (unlike `start_broker`'s infallible-with-retry shape) because there is no ephemeral
/// port to retry on: a DDS participant binds a real interface, and a bind/discovery failure there
/// is a genuine environment problem the caller should see, not paper over.
pub fn start_dds_probe(iface: impl Into<String>) -> Result<Box<dyn DdsParticipant>, DdsError> {
    let config = DdsConfig::domain0(iface);
    build_participant(&config)
}
