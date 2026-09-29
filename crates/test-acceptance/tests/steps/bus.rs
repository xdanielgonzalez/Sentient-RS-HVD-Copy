//! Steps for the multi-port feature area (#238 fan-out, #242 multi-port): proves
//! `bin_rpi_platform::HostPlatform::build_port` composes N platforms against ONE shared embedded
//! broker — the exact shape `bin-rpi::runtime::supervise` uses. Config-level normalization
//! (`[[bus]]` vs the legacy `[serial]`) is unit-tested in `bin-rpi` directly — see
//! `features/multi_port.feature`'s header comment for why it isn't reachable from here.
//!
//! Test code only; freely uses unwrap/expect/panic (the workspace lint gate denies these in
//! production logic, not tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bin_rpi_platform::{
    CyclePulse, DirectionMode, GreengrassTap, HostPlatform, HostTap, PreflightInputs, SinkSet,
};
use cucumber::{given, then, when};
use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;
use lib_mqtt::broker::{BrokerConfig, LocalBroker};

use crate::SpecWorld;

/// Scenario scratch for the multi-port feature area. `cucumber` resets `SpecWorld` per scenario.
#[derive(Default)]
pub struct BusState {
    broker: Option<LocalBroker>,
    build_results: Vec<Result<(), String>>,
}

impl std::fmt::Debug for BusState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusState")
            .field("broker_present", &self.broker.is_some())
            .field("build_results", &self.build_results)
            .finish()
    }
}

/// Reserve a free ephemeral TCP port, mirroring `test_kit::mqtt::start_broker`'s sibling helper —
/// keeps concurrent `cargo test` runs from fighting over a fixed port.
fn reserve_ephemeral_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read back the OS-assigned local address")
        .port()
}

#[given("a fresh embedded broker")]
fn given_broker(world: &mut SpecWorld) {
    let port = reserve_ephemeral_port();
    let cfg = BrokerConfig {
        id: "acceptance-multi-port".into(),
        host: "127.0.0.1".into(),
        port,
        auth: None,
    };
    world.bus.broker = Some(LocalBroker::start(cfg).expect("start broker"));
}

#[when(expr = "{int} host platforms are built against it, one per bus")]
fn when_build_platforms(world: &mut SpecWorld, n: i64) {
    let broker = world
        .bus
        .broker
        .as_mut()
        .expect("broker present (Given step ran)");
    world.bus.build_results.clear();
    for i in 0..n {
        let publisher = broker.publisher();
        let result = HostPlatform::build_port(
            publisher,
            SinkSet::default(),
            format!("bus-{i}"),
            "/dev/tty-sentient-does-not-exist-anywhere",
            SerialKind::Rs232,
            SerialParams::default(),
            DirectionMode::default(),
            100,
            CyclePulse::default(),
            PreflightInputs {
                device_id_present: true,
                dip_addresses_present: true,
                cloud: None,
            },
            None,
            Duration::from_secs(10),
        )
        .map(|_platform| ())
        .map_err(|e| e.to_string());
        world.bus.build_results.push(result);
    }
}

/// D13: a unit with both legs runs TWO identities concurrently, one per bus — a battery/Seplos bus
/// (`METEOR-001`, `device_type = "battery"`) and an inverter/Voltronic bus (`METEOR-001-inv`,
/// `device_type = "inverter"`), each with its own `GreengrassTap` (so its legacy publish lands on
/// `telemetry/{that type}/{that id}`), sharing the one broker. The identity DERIVATION itself
/// (`Config::identities`, decision 0007) is unit-tested in `bin-rpi` — see this feature's header.
#[when("a battery bus and an inverter bus are built against it")]
fn when_build_two_identity_buses(world: &mut SpecWorld) {
    let broker = world
        .bus
        .broker
        .as_mut()
        .expect("broker present (Given step ran)");
    world.bus.build_results.clear();
    for (device_id, device_type) in [("METEOR-001", "battery"), ("METEOR-001-inv", "inverter")] {
        let publisher = broker.publisher();
        let gg_publisher = broker.publisher();
        let (_loc_tx, loc_rx) = tokio::sync::watch::channel(None);
        let gg = GreengrassTap::new(gg_publisher, device_id, device_type, loc_rx);
        let sinks = SinkSet {
            taps: vec![HostTap::Greengrass(gg)],
        };
        let result = HostPlatform::build_port(
            publisher,
            sinks,
            format!("bus-{device_type}"),
            "/dev/tty-sentient-does-not-exist-anywhere",
            SerialKind::Rs232,
            SerialParams::default(),
            DirectionMode::default(),
            100,
            CyclePulse::default(),
            PreflightInputs {
                device_id_present: true,
                dip_addresses_present: true,
                cloud: None,
            },
            None,
            Duration::from_secs(10),
        )
        .map(|_platform| ())
        .map_err(|e| e.to_string());
        world.bus.build_results.push(result);
    }
}

#[then("each platform builds successfully")]
fn then_all_ok(world: &mut SpecWorld) {
    assert!(
        !world.bus.build_results.is_empty(),
        "no platforms were built — did the When step run?"
    );
    for (i, r) in world.bus.build_results.iter().enumerate() {
        assert!(r.is_ok(), "bus {i} failed to build: {r:?}");
    }
}

#[then("the broker is still running")]
fn then_broker_running(world: &mut SpecWorld) {
    assert!(
        world
            .bus
            .broker
            .as_ref()
            .expect("broker present")
            .is_running(),
        "broker must stay up through multi-platform construction"
    );
}
