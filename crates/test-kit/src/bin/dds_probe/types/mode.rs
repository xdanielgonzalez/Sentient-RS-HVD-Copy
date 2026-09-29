/// `dds-probe`'s three modes (mirrors `seplos-tcp-sim`'s `Mode`): `subscribe`/`publish` drive the
/// real `lib-dds` seam over `iface`; `dump-fixtures` renders canned XCDR2 bytes offline (no
/// networking) — the SAME canned values `deploy/dds-probe/dump_reference.py`'s `fixtures`/
/// `arbitration` modes use, for a Rust<->Python cross-check without a live participant.
pub(crate) enum Mode {
    Subscribe {
        iface: String,
        timeout_secs: u64,
    },
    Publish {
        iface: String,
        device_id: String,
        count: u32,
        interval_ms: u64,
    },
    DumpFixtures,
}
