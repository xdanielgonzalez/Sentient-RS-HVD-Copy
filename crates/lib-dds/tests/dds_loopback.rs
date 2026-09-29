//! Env-gated integration test: drive the REAL dust_dds stack through the bytes-in seam with our
//! actual `lib_tms` XCDR2 chain bytes for `Heartbeat`, and assert the sample arrives **oracle-equal**
//! (received wire bytes == the chain bytes we fed in — rung C's round-trip identity for optional-free
//! topics).
//!
//! Gated on `SENTIENT_TEST_DDS_LOOPBACK=1` (mirrors the repo's `SENTIENT_TEST_*` pattern) because it
//! needs a multicast-capable IPv6 interface (SPDP discovery); CI wiring is WP7. Interface via
//! `DDS_IFACE` (default `eth0`). Run:
//!   `SENTIENT_TEST_DDS_LOOPBACK=1 DDS_IFACE=eth0 cargo test -p lib-dds --test dds_loopback -- --nocapture`
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use lib_dds::dustdds::build_participant;
use lib_dds::{DdsConfig, DdsSample};
use lib_tms::cdr::{emit_object_cdr, CdrWriteSink, HasCdrChain};
use lib_tms::generated_qos::qos_profile;
use lib_tms::{Heartbeat, Identity, TmsTopicId};

/// Render a value as a complete RTPS XCDR2 SerializedPayload via its `lib_tms` chain — the same bytes
/// `lib_mapping::render_cdr` emits (replicated here so the test needs only `lib_tms`).
fn render<T: HasCdrChain>(v: &T) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    let mut sink = CdrWriteSink::new(&mut buf);
    emit_object_cdr::<T, T::CdrChain, _>(v, &mut sink);
    sink.finalize_encapsulation_padding();
    assert!(!sink.overflowed(), "render buffer overflow");
    let n = sink.len();
    buf[..n].to_vec()
}

fn ident(s: &str) -> Identity {
    let mut i = Identity::new();
    i.push_str(s).unwrap();
    i
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heartbeat_bytes_in_roundtrip_over_dust_dds() {
    if std::env::var("SENTIENT_TEST_DDS_LOOPBACK").is_err() {
        eprintln!(
            "SKIP heartbeat_bytes_in_roundtrip_over_dust_dds: set SENTIENT_TEST_DDS_LOOPBACK=1 \
             (+ optionally DDS_IFACE, default eth0) to run — needs a multicast IPv6 interface."
        );
        return;
    }
    let iface = std::env::var("DDS_IFACE").unwrap_or_else(|_| "eth0".to_string());

    // Advertise unicast at ::1 (same-host loopback) so intra-host DATA delivery is scope-free.
    let config = DdsConfig::domain0(iface);
    let participant = build_participant(&config).expect("build participant");

    let dev = ident("METEOR-001");
    let writer = participant
        .create_writer(
            TmsTopicId::Heartbeat,
            "Heartbeat",
            "Heartbeat",
            qos_profile(TmsTopicId::Heartbeat),
            &dev,
        )
        .expect("create Heartbeat writer");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<DdsSample>(16);
    participant
        .subscribe(
            TmsTopicId::Heartbeat,
            "Heartbeat",
            "Heartbeat",
            qos_profile(TmsTopicId::Heartbeat),
            tx,
        )
        .expect("subscribe Heartbeat");

    // Let SPDP/SEDP discovery match the intra-participant writer<->reader over the IPv6 transport.
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let hb = Heartbeat {
        device_id: dev.clone(),
        sequence_number: 42,
    };
    let expected = render(&hb);
    println!(
        "fed {} XCDR2 chain bytes for Heartbeat seq=42",
        expected.len()
    );

    // Publish a few times (first few may precede full matching; reliable+keep-last replays).
    for _ in 0..5 {
        writer.write(&dev, &expected).expect("write");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // Receive, asserting the wire bytes equal our chain bytes (oracle-equal / round-trip identity).
    let got = tokio::time::timeout(Duration::from_secs(8), rx.recv())
        .await
        .expect("timed out waiting for a Heartbeat sample")
        .expect("subscriber channel closed");

    assert_eq!(got.topic, TmsTopicId::Heartbeat);
    assert_eq!(
        got.payload, expected,
        "received wire bytes must equal the fed chain bytes (rung-C round-trip identity)\n\
         got:      {:02x?}\nexpected: {:02x?}",
        got.payload, expected
    );
    println!("PASS: bytes-in seam delivered Heartbeat oracle-equal over the dust_dds IPv6 stack");
}
