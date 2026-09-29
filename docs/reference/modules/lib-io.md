# `lib-io` — serial transport (RS-232 + RS-485)

The byte-transport abstraction that feeds the Seplos exchange, plus the generic poll driver that
runs it. One async trait, **generic over the reader** (`<R: SerialTransport>`, static dispatch, no
`Box<dyn>`), so the same driver monomorphizes onto a host `tokio-serial` port or an embassy MCU
UART.

## Role in the pipeline

```
serial hardware ──▶ SerialTransport ──▶ driver (read_telemetry / read_fault) ──▶ CanonicalReading
```

The trait seam here is what lets one codebase target both the RPi and the MCU: the transport is the
platform boundary, the driver above it is platform-free.

## `no_std` status

`#![cfg_attr(not(feature = "host"), no_std)]` + `#![forbid(unsafe_code)]`. The core (`transport`,
`driver`) is `no_std`; the `host` feature pulls in `std` + `tokio`. Features:

| Feature | Effect |
|---|---|
| `host` (default) | RPi Linux impls over `tokio-serial` (`dep:tokio`, `dep:tokio-serial`, `dep:log`, `lib-parse/std`). The deliverable path. |
| `mock` | Scripted in-memory `SerialTransport` for deterministic fault-injection tests (`no_std`, heapless). |
| `embassy` | Chip-agnostic UART over `embedded-io-async` + `embedded-hal` (host-checkable). |
| `stm32` / `nrf` / `rp2040` | Per-platform embassy HAL (`embassy-stm32` / `-nrf` / `-rp`) on top of `embassy`. The concrete chip feature (e.g. `stm32g4`) is selected downstream by the board/binary crate. |

## Key public types

| Item | What it is |
|---|---|
| `transport::SerialTransport` | The async byte-transport trait: `read`, `read_until`, `write_all`, `set_direction`. `async fn`-in-trait so it works in both `no_std` embassy and host tokio. |
| `transport::SerialParams` | Line params (baud, data bits, parity, stop bits); `Default` = Seplos 19200 8N1. |
| `transport::Parity` / `StopBits` | Line-parameter enums. |
| `transport::Direction` | RS-485 half-duplex bus direction (`Transmit` / `Receive`); a no-op on RS-232. |
| `transport::IoError` | Transport error (`Timeout`, `Device`, `ShortTransfer`, `Direction`). |
| `host::HostSerial` | Enum factory selecting RS-232/RS-485 from config without boxing (feature `host`). |
| `host::SerialKind` | `Rs232` / `Rs485` selector. |
| `driver::read_telemetry` / `read_fault` | Run one request/response Seplos exchange over any `SerialTransport` + `BmsVendorProtocol` (builds request, sequences RS-485 direction, reads to the `\r` delimiter, parses). |
| `driver::ExchangeError` | `Io` / `Parse`. |
| `embassy_uart` | embassy-task UART reader over the embedded-io-async/HAL traits (features `stm32`/`nrf`/`rp2040`); the MCU forward path. |
| `mock` | Scripted transport for tests (feature `mock` or under `test`). |

## Dependencies and dependents

- **Depends on:** `lib-parse` (for the exchange), `heapless`; optionally `tokio` +
  `tokio-serial` + `log` (host), or the embassy HAL crates (MCU).
- **Depended on by:** `impl-app`, `bin-rpi`. `lib-mqtt` uses it (feature `mock`) as a
  dev-dependency to drive its e2e test.

## Testing

- `tests/host_pty.rs` — exercises the **real** `tokio-serial` transport against a synthetic Seplos
  responder over a pty pair (`nix` dev-dep + the `seplos` parser to build/decode frames).
- `tests/host_timeout.rs` — the bounded-read timeout path (a silent bus → `IoError::Timeout`, never
  a hang).
- The `mock` transport backs `run_cycle` / broker e2e tests in `impl-app` and `lib-mqtt`.
