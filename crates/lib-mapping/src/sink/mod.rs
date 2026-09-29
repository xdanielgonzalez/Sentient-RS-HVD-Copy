//! `no_std` MQTT publish contract.
//!
//! The [`Publisher`] sink + topic builder + [`publish_tms`] live here (no `alloc`, no `std`) so the
//! **same contract** is implemented by the RPi's embedded rumqttd broker (std) and a Zephyr MCU MQTT
//! client (no_std) — the device-vs-broker difference is behind the trait, not in it. Topics are
//! built into a bounded `heapless::String`; payloads serialize via the [`crate::codec`] no_std JSON.

mod prelude;
mod types;

mod impl_publish_error;

pub use types::*;

use prelude::*;

/// Default topic prefix.
pub const DEFAULT_PREFIX: &str = "tms";
/// Max topic length (bounded, no alloc).
pub const MAX_TOPIC: usize = 128;
/// Max serialized payload size — the single render/publish buffer cap on both targets.
///
/// **Sized by measurement, 2026-07-18:** a FULL 16-cell `PowerHardwareUpdate` (the canonical
/// `MAX_CELLS` bound the Seplos packs actually reach) renders **1747 B**, and a 2-port
/// `DeviceInfo` measures ~1693 B — both silently exceeded the previous 1664 cap
/// (`render_*` → `BufferTooSmall` → publish dropped, the quiet-drop class
/// `tests/render_stack_depth.rs` pins). 2048 covers the 16-cell payload with ~270 B headroom
/// (a 32-char `Identity` adds ~22 B over the 10-char test id; growth beyond that requires a
/// re-measurement, not a guess — the size truth-table test prints every step).
///
/// **Why growing is affordable now (history):** the cap was trimmed 8192 → 2048 → 1664 during T3,
/// when up to 3 co-resident `[0u8; MAX_PAYLOAD]` stack buffers sat on top of the serde
/// materializers' multi-KB frames (`EnergyStorageState`'s ~1 KiB stack `Vec`,
/// `to_active_diagnostics`' 14.9 KB frame) and overflowed the poll-loop thread. Both multipliers
/// are gone: `impl_app::run` owns ONE reused buffer for all publishes in a cycle, and the
/// chain serializer (PR #316; `docs/wire-serialization.md` "Measured outcome") removed the
/// materializer frames entirely (render path ≤ 208 B). The +384 B here lands on a thread that
/// shed ~20 KB of frame mass.
///
/// The zephyr broker pump's `PUMP_BUF` derives from this constant automatically
/// (`BrokerCore::min_out_len(MAX_PAYLOAD)`, a `.bss` static — +384 B SRAM, budgeted). The
/// acceptance scenario "A TMS payload exceeding MAX_PAYLOAD fails to serialize"
/// (`test-acceptance/tests/features/mqtt_spec.feature`) still exercises the overflow path: its
/// maxed `ActiveDiagnosticState` fixture (64 DTCs) serializes to roughly 24 KiB, ~12x this cap.
pub const MAX_PAYLOAD: usize = 2048;

/// Build `{prefix}/{device_id}/{slug}` into `out`, where `slug` is `tms_topic` converted from
/// CamelCase to snake_case (e.g. `DcMeasurementUpdate` → `dc_measurement_update`). No allocation.
pub fn topic_into<const N: usize>(
    prefix: &str,
    device_id: &str,
    tms_topic: &str,
    out: &mut String<N>,
) -> Result<(), PublishError> {
    out.clear();
    let mut put = |s: &str| out.push_str(s).map_err(|_| PublishError::TopicTooLong);
    put(prefix)?;
    put("/")?;
    put(device_id)?;
    put("/")?;
    for (i, ch) in tms_topic.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_').map_err(|_| PublishError::TopicTooLong)?;
            }
            out.push(ch.to_ascii_lowercase())
                .map_err(|_| PublishError::TopicTooLong)?;
        } else {
            out.push(ch).map_err(|_| PublishError::TopicTooLong)?;
        }
    }
    Ok(())
}

/// Serialize a TMS payload to JSON (no_std codec) and publish it to
/// `{prefix}/{device_id}/{snake_case(tms_topic)}`. No allocation.
///
/// Convenience wrapper around [`publish_tms_into`] that owns its own `[0u8; MAX_PAYLOAD]`
/// stack-local scratch buffer. Fine for callers with stack headroom to spare (e.g. the host/RPi
/// platform, or a one-shot test); a caller on a RAM-critical thread that publishes more than once
/// per call frame (e.g. `impl_app::run_cycle`, up to 3x/cycle — see [`publish_tms_into`]'s doc
/// comment for the T3 stack-overflow story this avoids) should call `publish_tms_into` directly
/// with one reused buffer instead.
///
/// **GATED: test-only oracle (WP8).** Production code uses the wire-serializer
/// ([`crate::render::render_tms`] + [`publish_rendered`]); this function survives only as a
/// cross-oracle verification path.
#[cfg(feature = "std")]
pub fn publish_tms<P, T>(
    pubr: &mut P,
    prefix: &str,
    device_id: &str,
    tms_topic: &str,
    payload: &T,
) -> Result<(), PublishError>
where
    P: Publisher,
    T: Serialize,
{
    let mut buf = [0u8; MAX_PAYLOAD];
    publish_tms_into(pubr, prefix, device_id, tms_topic, payload, &mut buf)
}

/// Serialize a TMS payload to JSON (no_std codec) into caller-supplied `scratch`, and publish it to
/// `{prefix}/{device_id}/{snake_case(tms_topic)}`. No allocation, and — unlike [`publish_tms`] —
/// no stack-local payload buffer of its own: `scratch` is entirely the caller's storage choice
/// (stack-local, `static`, or otherwise), sized `>= MAX_PAYLOAD` to serialize any TMS payload this
/// crate produces (smaller buffers simply reject bigger payloads via `PublishError::Serialize`,
/// same as `to_json_slice`'s normal too-small-buffer behavior).
///
/// This is the T3 RAM-budget fix: `impl_app::run_cycle` (Zephyr `qemu_cortex_m3` e2e) calls
/// `publish_counted`/`publish_tms` up to 3 times per cycle (dc_measurement, power_hardware, and on
/// fault-due cycles active_diagnostic_state) within the same `async fn` activation — each call to
/// the old stack-owning `publish_tms` declared its own fresh `[0u8; MAX_PAYLOAD]` (1664 B) frame,
/// which combined with `EnergyStorageState`'s own stack-resident `Vec<StorageCellState, 50>` and
/// the rest of the poll-loop call chain overflowed the main thread's 9216 B stack under combined
/// broker+live-telemetry load (confirmed via psp-vs-linker-map correlation — see the T3 stack-diet
/// report). Routing every publish in a cycle through ONE caller-owned buffer (see
/// `impl_app::run`'s `payload_buf` local) removes that multiplication entirely: the buffer is
/// materialized once, in `run`'s outermost frame (which persists for the process lifetime, not
/// re-entered per cycle), rather than fresh on every nested `publish_tms` call.
///
/// **GATED: test-only oracle (WP8).** Production code uses the wire-serializer
/// ([`crate::render::render_tms`] + [`publish_rendered`]); this function survives only as a
/// cross-oracle verification path.
#[cfg(feature = "std")]
pub fn publish_tms_into<P, T>(
    pubr: &mut P,
    prefix: &str,
    device_id: &str,
    tms_topic: &str,
    payload: &T,
    scratch: &mut [u8],
) -> Result<(), PublishError>
where
    P: Publisher,
    T: Serialize,
{
    let mut topic = String::<MAX_TOPIC>::new();
    topic_into(prefix, device_id, tms_topic, &mut topic)?;
    let n = crate::codec::to_json_slice(payload, scratch).map_err(|_| PublishError::Serialize)?;
    pubr.publish(&topic, &scratch[..n])
}

/// Render a TMS payload via [`crate::wire`]'s flat serializer (a `render_tms`/`render_fault`-style
/// closure: `FnOnce(&mut [u8]) -> Option<usize>`) into caller-supplied `scratch`, and publish it to
/// `{prefix}/{device_id}/{snake_case(tms_topic)}`. No allocation, no serde.
///
/// **One wire format**: this is what unifies the RPi host publish path
/// (`impl_app::host::HostPlatform::emit_reading`/`emit_fault`, via [`crate::render::render_tms`]/
/// [`crate::render::render_fault`]) with the device-hosted-broker path
/// (`bin_zephyr_platform::render_pending_topic`, calling the same `render_tms`/`render_fault`) — both
/// ultimately bottom out in [`crate::wire`]'s serializer, so host and device emit byte-identical
/// JSON for the same canonical input (mirrors [`publish_tms_into`]'s caller-owned-`scratch`
/// discipline, just fed by a render closure instead of a `T: Serialize` value).
pub fn publish_rendered<P>(
    pubr: &mut P,
    prefix: &str,
    device_id: &str,
    tms_topic: &str,
    scratch: &mut [u8],
    render: impl FnOnce(&mut [u8]) -> Option<usize>,
) -> Result<(), PublishError>
where
    P: Publisher,
{
    let mut topic = String::<MAX_TOPIC>::new();
    topic_into(prefix, device_id, tms_topic, &mut topic)?;
    let n = render(scratch).ok_or(PublishError::Serialize)?;
    pubr.publish(&topic, &scratch[..n])
}
