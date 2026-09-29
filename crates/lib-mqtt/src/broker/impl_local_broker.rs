use super::prelude::*;

impl LocalBroker {
    /// Boot the embedded rumqttd broker with a TCP listener at `config.host:config.port` and an
    /// in-process link for publishing.
    ///
    /// Constructs a minimal rumqttd 0.19 Config with one v4 server listener bound to
    /// `config.host:config.port`. Creates the broker and in-process link, spawns the broker on a
    /// dedicated thread, and returns a `LocalBroker` holding the link sender and thread handle.
    pub fn start(config: BrokerConfig) -> Result<Self, NetError> {
        // Parse broker id as usize (default to 0 if unparseable, for compatibility)
        let broker_id = config.id.parse::<usize>().unwrap_or(0);

        // Build the socket address for the TCP listener
        let listen_addr = SocketAddr::new(
            config
                .host
                .parse()
                .map_err(|e| NetError::Link(format!("Invalid host: {}", e)))?,
            config.port,
        );

        // Create minimal ConnectionSettings with sensible defaults for a local hop. `auth` (the
        // static username/password map) is deliberately never populated — the only supported
        // credential path is `external_auth`, routed through the `lib-security` seam via
        // `seam_auth` (see `BrokerConfig::auth`); there is no hardcoded credential ladder here.
        let connection_settings = rumqttd::ConnectionSettings {
            connection_timeout_ms: 10000,  // 10 second timeout
            max_payload_size: 1024 * 1024, // 1 MB max payload
            max_inflight_count: 100,       // Up to 100 in-flight publishes
            auth: None,
            external_auth: config.auth.map(|a| a.0),
            dynamic_filters: false,
        };

        // Create the v4 server settings for the single TCP listener
        let mut v4_map = HashMap::new();
        v4_map.insert(
            "tms".to_string(),
            rumqttd::ServerSettings {
                name: "tms".to_string(),
                listen: listen_addr,
                tls: None,
                next_connection_delay_ms: 0,
                connections: connection_settings,
            },
        );

        // Create router config with reasonable defaults
        let router_config = rumqttd::RouterConfig {
            max_connections: 1000,
            max_outgoing_packet_count: 1000,
            max_segment_size: 65536,
            max_segment_count: 100,
            custom_segment: None,
            initialized_filters: None,
            shared_subscriptions_strategy: Default::default(),
        };

        // Build the complete broker config
        let broker_config = rumqttd::Config {
            id: broker_id,
            router: router_config,
            v4: Some(v4_map),
            v5: None,
            ws: None,
            cluster: None,
            console: None,
            bridge: None,
            prometheus: None,
            metrics: None,
        };

        // Create the broker
        let mut broker = rumqttd::Broker::new(broker_config);

        // Create in-process link for publishing. `_link_rx` (the LinkRx half) is intentionally
        // dropped here, not kept for a `BrokerSubscriber` — this session's spike
        // (`tests/link_rx_spike.rs`) confirmed LinkRx DOES deliver in-process publishes once
        // subscribed, and `impl_broker_subscriber.rs::try_recv` is fully implemented against it,
        // but wiring a live subscriber through `LocalBroker` needs a new field on this struct
        // (`types/local_broker.rs`) — outside the #240/#244/#246 TMS-breadth workstream's
        // owned-file boundary. `bin-rpi::tasks::reply_listener` uses the documented, equally-valid
        // loopback `rumqttc` TCP client fallback instead for now; a follow-up work stream can swap
        // it for the in-process path by keeping this `LinkRx` (mirroring `link_tx` below) once
        // that field lands.
        let (link_tx, _link_rx) = broker
            .link("sentient-local")
            .map_err(|e| NetError::Link(e.to_string()))?;

        // Spawn broker on a separate thread, keeping it alive
        let broker_thread = std::thread::spawn(move || {
            let _ = broker.start();
        });

        Ok(LocalBroker {
            broker_thread: Arc::new(broker_thread),
            link_tx: Arc::new(Mutex::new(link_tx)),
            failures: Arc::new(AtomicU32::new(0)),
        })
    }

    /// A [`Publisher`] that publishes onto this broker via its in-process `LinkTx`.
    ///
    /// Returns a `BrokerPublisher` sharing the broker's `LinkTx`, liveness handle, and
    /// failure counter.
    pub fn publisher(&mut self) -> BrokerPublisher {
        BrokerPublisher {
            link_tx: Arc::clone(&self.link_tx),
            broker_thread: Arc::clone(&self.broker_thread),
            failures: Arc::clone(&self.failures),
        }
    }

    /// True while the broker thread is still running.
    pub fn is_running(&self) -> bool {
        !self.broker_thread.is_finished()
    }
}
