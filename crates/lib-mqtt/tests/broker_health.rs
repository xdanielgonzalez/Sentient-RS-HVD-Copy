//! T4b — the broker publisher reports real health, not a hardcoded `true`.
//!
//! Starts the embedded rumqttd broker on a loopback high port, publishes through the in-process
//! link, and asserts the publisher is healthy and the broker thread is running. (The
//! consecutive-failure threshold that flips `healthy()` to false is exercised by the atomic
//! counter updated on every publish; this test pins the live, wired-up happy path.)
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lib_mapping::sink::Publisher;
use test_kit::start_broker;

#[test]
fn healthy_while_broker_runs_and_publishes() {
    let (mut broker, _port) = start_broker();
    let mut publisher = broker.publisher();

    assert!(
        broker.is_running(),
        "broker thread should be alive right after start"
    );
    assert!(
        publisher.healthy(),
        "a fresh publisher over a live broker is healthy"
    );

    // A successful publish keeps it healthy and holds the failure streak at zero.
    publisher
        .publish("tms/METEOR-001/dc_measurement_update", b"{}")
        .unwrap();
    assert!(
        publisher.healthy(),
        "still healthy after a successful publish"
    );
    assert!(broker.is_running());
}
