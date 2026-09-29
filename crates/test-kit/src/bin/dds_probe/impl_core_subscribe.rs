use super::prelude::*;
use test_kit::start_dds_probe;

/// `subscribe` mode: build a real participant on `iface`, subscribe `Heartbeat` (the only topic
/// wired through `DustParticipant` today — WP6 adds the rest), print each received
/// [`DdsSample`]'s topic + hex payload to stdout until `timeout` elapses. The HEAVY asserting
/// (USER_DATA, late-joiner replay, KEEP_LAST depth) is the python probe's job — this just proves
/// the Rust side can reach a live DDS reader through the real `lib-dds` seam.
pub(crate) async fn run_subscribe(iface: String, timeout: Duration) -> Result<(), AppError> {
    let participant = start_dds_probe(iface)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<DdsSample>(16);
    participant.subscribe(
        TmsTopicId::Heartbeat,
        "Heartbeat",
        "Heartbeat",
        qos_profile(TmsTopicId::Heartbeat),
        tx,
    )?;

    println!(
        "dds-probe: subscribed Heartbeat, waiting up to {}s ...",
        timeout.as_secs()
    );
    let deadline = tokio::time::Instant::now() + timeout;
    let mut received = 0u32;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(sample)) => {
                received += 1;
                println!(
                    "RECEIVED #{received}: topic={:?} payload=[{}]",
                    sample.topic,
                    sample
                        .payload
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
            Ok(None) => {
                println!("dds-probe: subscriber channel closed");
                break;
            }
            Err(_elapsed) => break,
        }
    }
    println!("dds-probe: done, {received} sample(s) received");
    Ok(())
}
