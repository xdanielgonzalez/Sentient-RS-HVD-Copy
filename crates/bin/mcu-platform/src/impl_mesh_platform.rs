use crate::prelude::*;

impl<Inner: Platform, S: Sender<Item = MeshUnit>> MeshPlatform<Inner, S> {
    /// Wrap `inner` with a mesh-radio forwarding leg over `mesh` (the channel sender half the board
    /// crate supplies). Starts with no cached reading — see [`MeshPlatform::last_reading`]'s doc
    /// comment.
    pub fn new(inner: Inner, mesh: S) -> Self {
        MeshPlatform {
            inner,
            mesh,
            last_reading: None,
        }
    }

    /// Condense `reading` (+ optional `fault`) into `scratch` and `try_send` the result onto the
    /// mesh channel. `scratch` is the caller's `payload_buf` (the shared reused TMS-serialize
    /// scratch every `Platform::emit_*` call already receives) — reused here, not a second buffer,
    /// once this platform's own TMS render/publish calls have already consumed its previous
    /// contents (see the repo's zero-copy/reused-scratch doctrine). Fail-open: an empty
    /// condensation or a full/closed channel is silently dropped, never surfaced as a `Health`
    /// failure (see this type's doc comment).
    fn forward_to_mesh(
        &mut self,
        reading: &CanonicalReading,
        fault: Option<&CanonicalFault>,
        scratch: &mut [u8],
    ) {
        let input = CondenseInput { reading, fault };
        let n = condense(&input, MESH_PAYLOAD, scratch);
        if n == 0 {
            return;
        }
        if let Ok(payload) = heapless::Vec::from_slice(&scratch[..n]) {
            let _ = self.mesh.try_send(MeshUnit { payload });
        }
    }
}

impl<Inner: Platform, S: Sender<Item = MeshUnit>> Platform for MeshPlatform<Inner, S> {
    type Transport = Inner::Transport;
    type Pub = Inner::Pub;

    fn transport(&mut self) -> &mut Self::Transport {
        self.inner.transport()
    }
    fn publisher(&mut self) -> &mut Self::Pub {
        self.inner.publisher()
    }
    fn now(&self) -> ClockMonotonic {
        self.inner.now()
    }
    async fn sleep(&mut self, ms: u32) {
        self.inner.sleep(ms).await;
    }

    /// Delegates to `inner.emit_reading` first (the primary MQTT telemetry mission, unchanged),
    /// caches `reading` for a subsequent `emit_fault`, then best-effort forwards a condensed frame
    /// over the mesh channel — additive, never gating the primary path on the mesh outcome.
    fn emit_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.inner
            .emit_reading(cfg, reading, now, payload_buf, health);
        self.last_reading = Some(reading.clone());
        self.forward_to_mesh(reading, None, payload_buf);
    }

    /// Delegates to `inner.emit_fault` first, then — if a reading has already been cached (see
    /// [`MeshPlatform::last_reading`]'s doc comment) — best-effort forwards a condensed
    /// reading+fault frame over the mesh channel. A fault with nothing cached yet to attach it to
    /// is dropped (matches the Pi's `MeshTap` posture for the identical gap): `condense`'s `reading`
    /// field is mandatory, and there is nothing honest to send without one.
    fn emit_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalFault,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.inner.emit_fault(cfg, fault, now, payload_buf, health);
        if let Some(reading) = self.last_reading.clone() {
            self.forward_to_mesh(&reading, Some(fault), payload_buf);
        }
    }

    fn emit_inverter_reading(
        &mut self,
        cfg: &RunConfig,
        reading: &CanonicalInverterReading,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.inner
            .emit_inverter_reading(cfg, reading, now, payload_buf, health);
    }

    fn emit_inverter_fault(
        &mut self,
        cfg: &RunConfig,
        fault: &CanonicalInverterFault,
        mode: InverterMode,
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.inner
            .emit_inverter_fault(cfg, fault, mode, now, payload_buf, health);
    }

    fn emit_rail_draw(
        &mut self,
        cfg: &RunConfig,
        readings: &[DeviceRailReading],
        now: ClockMonotonic,
        payload_buf: &mut [u8],
        health: &mut Health,
    ) {
        self.inner
            .emit_rail_draw(cfg, readings, now, payload_buf, health);
    }

    fn on_cycle(&mut self, health: &Health) {
        self.inner.on_cycle(health);
    }

    async fn reconnect(&mut self) -> bool {
        self.inner.reconnect().await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lib_net::channel::TrySendError;

    /// Bare-minimum `Platform` stub: records what was delegated to it, so tests can assert the
    /// decorator forwards the primary path unconditionally regardless of the mesh outcome.
    struct StubInner {
        reading_calls: u32,
        fault_calls: u32,
    }

    impl Platform for StubInner {
        type Transport = StubTransport;
        type Pub = NullPublisher;

        fn transport(&mut self) -> &mut Self::Transport {
            unimplemented!("not exercised by these tests")
        }
        fn publisher(&mut self) -> &mut Self::Pub {
            unimplemented!("not exercised by these tests")
        }
        fn now(&self) -> ClockMonotonic {
            ClockMonotonic {
                seconds: 0,
                nanoseconds: 0,
            }
        }
        async fn sleep(&mut self, _ms: u32) {}
        fn emit_reading(
            &mut self,
            _cfg: &RunConfig,
            _reading: &CanonicalReading,
            _now: ClockMonotonic,
            _payload_buf: &mut [u8],
            _health: &mut Health,
        ) {
            self.reading_calls += 1;
        }
        fn emit_fault(
            &mut self,
            _cfg: &RunConfig,
            _fault: &CanonicalFault,
            _now: ClockMonotonic,
            _payload_buf: &mut [u8],
            _health: &mut Health,
        ) {
            self.fault_calls += 1;
        }
    }

    /// A `SerialTransport` never actually driven — `StubInner`'s associated type only, to satisfy
    /// `Platform::Transport: SerialTransport`; `StubInner::transport` is never called by these tests.
    struct StubTransport;
    impl lib_io::transport::SerialTransport for StubTransport {
        async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, lib_io::transport::IoError> {
            unimplemented!("not exercised by these tests")
        }
        async fn read_until(
            &mut self,
            _delimiter: u8,
            _buf: &mut [u8],
        ) -> Result<usize, lib_io::transport::IoError> {
            unimplemented!("not exercised by these tests")
        }
        async fn write_all(&mut self, _bytes: &[u8]) -> Result<(), lib_io::transport::IoError> {
            unimplemented!("not exercised by these tests")
        }
        async fn set_direction(
            &mut self,
            _dir: lib_io::transport::Direction,
        ) -> Result<(), lib_io::transport::IoError> {
            unimplemented!("not exercised by these tests")
        }
    }

    /// A `Publisher` that never publishes — `StubInner`'s associated type only, never invoked.
    struct NullPublisher;
    impl Publisher for NullPublisher {
        fn publish(&mut self, _topic: &str, _payload: &[u8]) -> Result<(), PublishError> {
            unimplemented!("not exercised by these tests")
        }
        fn healthy(&self) -> bool {
            true
        }
    }

    /// Hand-rolled single-slot mock `Sender` (mirrors `impl_channel_publisher.rs`'s test mock).
    struct MockSender {
        slot: Option<MeshUnit>,
    }
    impl Sender for MockSender {
        type Item = MeshUnit;
        fn try_send(&mut self, item: MeshUnit) -> Result<(), TrySendError<MeshUnit>> {
            if self.slot.is_some() {
                return Err(TrySendError::Full(item));
            }
            self.slot = Some(item);
            Ok(())
        }
        async fn send(&mut self, item: MeshUnit) -> Result<(), lib_net::channel::Closed> {
            self.try_send(item).map_err(|_| lib_net::channel::Closed)
        }
    }

    fn reading() -> CanonicalReading {
        CanonicalReading {
            device_address: 1,
            cell_voltages_mv: heapless::Vec::new(),
            temperatures_raw: heapless::Vec::new(),
            current_ca: -144,
            total_voltage_cv: 5451,
            remaining_capacity_cah: 13_953,
            typical_capacity_cah: 18_000,
            rated_capacity_cah: 18_000,
            soc_dpct: 775,
            soh_dpct: 1000,
            cycles: 2,
            port_voltage_cv: 5453,
        }
    }

    fn zero_now() -> ClockMonotonic {
        ClockMonotonic {
            seconds: 0,
            nanoseconds: 0,
        }
    }

    fn run_cfg() -> RunConfig {
        let mut device_id = lib_mapping::schema::Identity::new();
        let _ = device_id.push_str("TEST-001");
        let mut prefix = heapless::String::new();
        let _ = prefix.push_str("tms");
        RunConfig {
            device_id,
            prefix,
            dip_addresses: heapless::Vec::new(),
            interval_ms: 1000,
            fault_every: 10,
            max_backoff_ms: 1000,
        }
    }

    #[test]
    fn emit_reading_delegates_and_forwards_a_condensed_frame() {
        let inner = StubInner {
            reading_calls: 0,
            fault_calls: 0,
        };
        let mut platform = MeshPlatform::new(inner, MockSender { slot: None });
        let mut buf = [0u8; 256];
        let mut health = Health::default();

        platform.emit_reading(&run_cfg(), &reading(), zero_now(), &mut buf, &mut health);

        assert_eq!(
            platform.inner.reading_calls, 1,
            "primary path must still run"
        );
        assert!(platform.last_reading.is_some());
        let sent = platform
            .mesh
            .slot
            .as_ref()
            .expect("a frame was condensed and sent");
        let payload = core::str::from_utf8(&sent.payload).expect("valid utf8");
        assert!(payload.starts_with('{') && payload.ends_with('}'));
    }

    #[test]
    fn emit_fault_with_no_cached_reading_is_dropped_but_primary_path_still_runs() {
        let inner = StubInner {
            reading_calls: 0,
            fault_calls: 0,
        };
        let mut platform = MeshPlatform::new(inner, MockSender { slot: None });
        let mut buf = [0u8; 256];
        let mut health = Health::default();
        let fault = CanonicalFault {
            device_address: 1,
            charge_overcurrent_protection: true,
            ..CanonicalFault::default()
        };

        platform.emit_fault(&run_cfg(), &fault, zero_now(), &mut buf, &mut health);

        assert_eq!(platform.inner.fault_calls, 1, "primary path must still run");
        assert!(
            platform.mesh.slot.is_none(),
            "a fault with nothing cached to attach it to must not be sent"
        );
    }

    #[test]
    fn emit_fault_after_a_reading_forwards_a_condensed_fault_frame() {
        let inner = StubInner {
            reading_calls: 0,
            fault_calls: 0,
        };
        let mut platform = MeshPlatform::new(inner, MockSender { slot: None });
        let mut buf = [0u8; 256];
        let mut health = Health::default();

        platform.emit_reading(&run_cfg(), &reading(), zero_now(), &mut buf, &mut health);
        // Drain the reading's own frame so the fault's frame is unambiguous below.
        platform.mesh.slot = None;

        let fault = CanonicalFault {
            device_address: 1,
            charge_overcurrent_protection: true,
            ..CanonicalFault::default()
        };
        platform.emit_fault(&run_cfg(), &fault, zero_now(), &mut buf, &mut health);

        let sent = platform
            .mesh
            .slot
            .as_ref()
            .expect("fault frame should have been sent");
        let payload = core::str::from_utf8(&sent.payload).expect("valid utf8");
        assert!(
            payload.contains("\"f\":"),
            "condensed fault frame missing the fault bitmask field: {payload}"
        );
    }
}
