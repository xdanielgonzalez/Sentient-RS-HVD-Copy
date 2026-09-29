# `lib-mqtt` — the device's local MQTT surface

The dissemination end of the pipeline on the RPi target: an embedded MQTT server that hosts the
device's TMS data, published onto in-process.

## Role in the pipeline

```
TMS payload ──▶ Publisher (BrokerPublisher) ──▶ embedded rumqttd broker (127.0.0.1:1883)
                                                        │  (external mosquitto bridge subscribes)
                                                        ▼
                                                 private cloud / Greengrass
```

`bin-rpi` embeds an **rumqttd MQTT server** and publishes mapped TMS payloads onto it via the
in-process broker `Link`. An external **mosquitto** bridge connects to that broker, subscribes to
the TMS topics, and relays them to the private-cloud endpoint (and Greengrass). **mosquitto reads
*from* the device; Rust never pushes to the cloud** — see [`../deployment.md`](../../deployment/README.md) §1
and §5.

## `no_std` status

`#![forbid(unsafe_code)]`, **std-only** (this is an RPi-side, `tokio` + `rumqttd` crate — no `no_std`
build). The `no_std` publish contract itself lives in `lib-mapping::sink` so a Zephyr MQTT
client can implement the same `Publisher` trait without pulling this crate in.

## Key public types

| Item | What it is |
|---|---|
| `broker::LocalBroker` | The embedded rumqttd server; `start(BrokerConfig)` brings it up and keeps the broker thread/listener alive for the process lifetime. |
| `broker::BrokerPublisher` | A `Publisher` backed by the broker's `LinkTx` (in-process publish). |
| `broker::BrokerConfig` | Broker bind/config (host/port). |
| `publisher::MockPublisher` | Test mock sink. |
| `publisher::NetError` | Publisher/link error type. |
| re-exports | `publish_tms`, `topic_into`, `PublishError`, `Publisher`, `DEFAULT_PREFIX` (re-exported from `lib_mapping::sink` for RPi-side ergonomics). |

The broker's CONNECT authorizer routes exclusively through `lib_security`'s `TokenVerifier`
seam (never a hardcoded credential) — an unbound backend fails closed
(`lib_security::SecurityError::NotImplemented` denies the connection); the concrete
`SoftwareTokenVerifier` backend (`lib_security::backends::software`) is what's wired in today.

## Dependencies and dependents

- **Depends on:** `lib-parse`, `lib-mapping` (the `Publisher` contract + TMS types),
  `lib-security` (`TokenVerifier`/`TokenPolicy` for broker CONNECT auth),
  `rumqttd`, `tokio`, `serde`, `thiserror`, `log`.
- **Depended on by:** `impl-app` (optional, behind the `host` feature), `bin-rpi`.

## Testing

- `tests/broker_roundtrip.rs` — publish → subscribe round-trip against a real embedded broker
  (includes the env-gated `SENTIENT_TEST_ENDPOINT` cloud-reachability check, see
  [`../deployment.md`](../../deployment/README.md) §9).
- `tests/broker_health.rs` — broker/publisher health reporting.
- `tests/publish.rs` — the publish path and topic/JSON shape (`serde_json` dev-dep parses the
  emitted JSON back).
- `tests/mqtt_e2e.rs` — drives the real `run_cycle` poll loop (via `impl-app` +
  `lib-io`'s `mock` transport) through the real broker `Publisher`.
