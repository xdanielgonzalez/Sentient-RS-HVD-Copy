use super::impl_core_render::render;
use super::prelude::*;
use test_kit::start_dds_probe;

/// `publish` mode: build a real participant on `iface`, create a `Heartbeat` writer for
/// `device_id`, publish `count` XCDR2-rendered samples (the exact bytes `lib_tms::cdr` emits —
/// mirrors `crates/lib-dds/tests/dds_loopback.rs`'s `render` helper) `interval` apart.
pub(crate) async fn run_publish(
    iface: String,
    device_id: String,
    count: u32,
    interval: Duration,
) -> Result<(), AppError> {
    let participant = start_dds_probe(iface)?;

    let mut id = Identity::new();
    id.push_str(&device_id)
        .map_err(|_| AppError::Usage(format!("--device-id {device_id:?} too long")))?;

    let writer = participant.create_writer(
        TmsTopicId::Heartbeat,
        "Heartbeat",
        "Heartbeat",
        qos_profile(TmsTopicId::Heartbeat),
        &id,
    )?;

    println!("dds-probe: publishing {count} Heartbeat sample(s) for device {device_id}");
    for seq in 0..count {
        let hb = lib_tms::Heartbeat {
            device_id: id.clone(),
            sequence_number: seq,
        };
        let payload = render(&hb);
        writer.write(&id, &payload)?;
        println!("  wrote seq={seq} ({} bytes)", payload.len());
        tokio::time::sleep(interval).await;
    }
    Ok(())
}
