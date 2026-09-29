use crate::prelude::*;

impl<S: Sender<Item = MqttUnit>> ChannelPublisher<S> {
    /// Wrap `sender` (the concrete channel half `bin/mcu-rp2350` supplies — an `embassy_sync` channel
    /// `Sender` under the `channel-embassy` feature, or a mock in tests). Starts `healthy()` —
    /// optimistic until a `try_send` actually fails, matching `ZephyrMqttPublisher`'s posture.
    pub fn new(sender: S) -> Self {
        ChannelPublisher {
            sender,
            last_ok: true,
        }
    }
}

impl<S: Sender<Item = MqttUnit>> Publisher for ChannelPublisher<S> {
    /// Build an [`MqttUnit`] (QoS 1) from `topic`/`payload` and `try_send` it onto the channel.
    /// Fails closed — never a silent drop — on either an oversize topic/payload (rejected before
    /// any send is attempted; does not itself affect `healthy()`, see this type's doc comment) or a
    /// full/closed channel (`try_send` failure; DOES flip `healthy()` to `false`).
    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), PublishError> {
        let mut unit_topic = heapless::String::<MQTT_TOPIC>::new();
        unit_topic
            .push_str(topic)
            .map_err(|_| PublishError::TopicTooLong)?;
        let unit_payload = heapless::Vec::<u8, MQTT_PAYLOAD>::from_slice(payload)
            .map_err(|_| PublishError::Serialize)?;
        let unit = MqttUnit {
            topic: unit_topic,
            payload: unit_payload,
            qos: 1,
        };
        match self.sender.try_send(unit) {
            Ok(()) => {
                self.last_ok = true;
                Ok(())
            }
            Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => {
                self.last_ok = false;
                Err(PublishError::Sink)
            }
        }
    }

    fn healthy(&self) -> bool {
        self.last_ok
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Hand-rolled single-slot mock `Sender` — no executor needed since these tests only exercise
    /// `try_send` (the `async fn send` half is never reached by `ChannelPublisher::publish`).
    struct MockSender {
        slot: Option<MqttUnit>,
        closed: bool,
    }

    impl MockSender {
        fn new() -> Self {
            MockSender {
                slot: None,
                closed: false,
            }
        }
    }

    impl Sender for MockSender {
        type Item = MqttUnit;

        fn try_send(&mut self, item: MqttUnit) -> Result<(), TrySendError<MqttUnit>> {
            if self.closed {
                return Err(TrySendError::Closed(item));
            }
            if self.slot.is_some() {
                return Err(TrySendError::Full(item));
            }
            self.slot = Some(item);
            Ok(())
        }

        async fn send(&mut self, item: MqttUnit) -> Result<(), lib_net::channel::Closed> {
            self.try_send(item).map_err(|_| lib_net::channel::Closed)
        }
    }

    #[test]
    fn publish_builds_mqtt_unit_with_qos_1_and_sends() {
        let mut publisher = ChannelPublisher::new(MockSender::new());
        publisher
            .publish("tms/METEOR-001/dc_measurement_update", br#"{"x":1}"#)
            .unwrap();

        let sent = publisher.sender.slot.as_ref().expect("slot filled");
        assert_eq!(sent.topic.as_str(), "tms/METEOR-001/dc_measurement_update");
        assert_eq!(sent.payload.as_slice(), br#"{"x":1}"#);
        assert_eq!(sent.qos, 1);
        assert!(publisher.healthy());
    }

    #[test]
    fn full_channel_maps_to_publish_error_sink_and_flips_unhealthy() {
        let mut publisher = ChannelPublisher::new(MockSender::new());
        // Fill the mock's single slot.
        publisher.publish("t", b"a").unwrap();
        assert!(publisher.healthy());

        // A second publish finds the channel full.
        let err = publisher.publish("t", b"b").unwrap_err();
        assert_eq!(err, PublishError::Sink);
        assert!(!publisher.healthy());
    }

    #[test]
    fn closed_channel_maps_to_publish_error_sink_and_flips_unhealthy() {
        let mut publisher = ChannelPublisher::new(MockSender::new());
        publisher.sender.closed = true;

        let err = publisher.publish("t", b"a").unwrap_err();
        assert_eq!(err, PublishError::Sink);
        assert!(!publisher.healthy());
    }

    #[test]
    fn oversize_topic_fails_closed_before_any_send() {
        let mut publisher = ChannelPublisher::new(MockSender::new());
        // `MqttUnit::topic` is `heapless::String<128>` (lib_net's `MQTT_TOPIC`) — 200 bytes
        // overflows it. Built on the stack (no alloc) to stay no_std-safe under `#[cfg(test)]`.
        let raw = [b'x'; 200];
        let huge_topic = core::str::from_utf8(&raw).unwrap();

        let err = publisher.publish(huge_topic, b"a").unwrap_err();
        assert_eq!(err, PublishError::TopicTooLong);
        // Never reached try_send — the channel is untouched.
        assert!(publisher.sender.slot.is_none());
    }

    #[test]
    fn oversize_payload_fails_closed_before_any_send() {
        let mut publisher = ChannelPublisher::new(MockSender::new());
        // `MqttUnit::payload` is `heapless::Vec<u8, 8192>` (lib_net's `MQTT_PAYLOAD`) — 9000 bytes
        // overflows it. A stack array, not a heap `Vec` — stays no_std-safe.
        let huge_payload = [0u8; 9000];

        let err = publisher.publish("t", &huge_payload).unwrap_err();
        assert_eq!(err, PublishError::Serialize);
        assert!(publisher.sender.slot.is_none());
    }
}
