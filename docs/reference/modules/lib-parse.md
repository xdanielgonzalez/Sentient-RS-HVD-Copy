# `lib-parse` — BMS vendor protocol parsers

Raw response bytes → a vendor-neutral canonical battery reading. This is the bottom of the data
pipeline: the pure, platform-free protocol layer that every downstream stage consumes.

## Role in the pipeline

```
Seplos bus bytes ──▶ lib-parse ──▶ CanonicalReading / CanonicalFault ──▶ lib-mapping
```

Each battery vendor sits behind a cargo feature and implements one trait
(`vendor::BmsVendorProtocol`); the downstream TMS mapper consumes *only* the canonical model, so
adding a vendor is a new feature-gated parser with **zero mapper change**. Correctness is measured
against the Seplos protocol + real device captures, never the Python prototype (see
[`../../../KICKOFF.md`](../../../KICKOFF.md)).

## `no_std` status

`#![cfg_attr(not(feature = "std"), no_std)]` + `#![forbid(unsafe_code)]`. Pure, `no_std`-friendly
(`default-features = false`), zero I/O, zero platform surface. Fixed-point raw units are preserved
(no floats); scaling to engineering units is a downstream concern. Storage is bounded (`heapless`).

Features: `default = ["std", "seplos", "voltronic"]`; `std` (enables the std surface), `seplos`
(the Seplos BMS vendor parser module), `voltronic` (the Voltronic/PI30 inverter vendor parser
module — see below).

## Key public types

| Item | What it is |
|---|---|
| `vendor::BmsVendorProtocol` | The per-vendor trait: `parse_reading`, `parse_fault`, `build_request`. |
| `vendor::BmsCommand` | The command enum: `Telemetry`, `Fault`, `SerialNumber`. |
| `vendor::ParseError` | Vendor-neutral parse error (`BadFraming`, `Malformed`, `BadChecksum`, `DeviceError`, `AddressMismatch`, `CapacityExceeded`). |
| `canonical::CanonicalReading` | Vendor-neutral telemetry reading in raw fixed-point units (cell voltages mV, temps raw, current cА, SOC/SOH deci-percent, …). Bounded `heapless::Vec` storage (`MAX_CELLS = 16`, `MAX_TEMPS = 8`). |
| `canonical::CanonicalFault` | Vendor-neutral fault/warning flags. |
| `seplos::Seplos` | The stateless Seplos parser implementing `BmsVendorProtocol` (feature `seplos`). |
| `seplos::SeplosError` | Seplos wire-mechanics error, mapped into `ParseError`. |
| `vendor::InverterVendorProtocol` | The per-vendor inverter trait (feature `voltronic` today): `parse_reading`, `parse_fault`, `build_request`, `validate_frame`. |
| `vendor::InverterCommand` | The inverter command enum: `Status`, `Mode`, `Fault`, `Ratings`. |
| `canonical::CanonicalInverterReading` / `CanonicalInverterFault` | Vendor-neutral inverter telemetry/fault model (PV/AC/battery measurements, QPIWS fault bits), bounded `heapless` storage, mirroring `CanonicalReading`/`CanonicalFault`'s battery-side shape. |
| `voltronic::Voltronic` | The stateless Voltronic/PI30 parser implementing `InverterVendorProtocol` (feature `voltronic`). PROVISIONAL — see below. |
| `voltronic::crc::crc16_xmodem` | The CRC-16/XMODEM frame checksum (polynomial `0x1021`, init `0x0000`, no reflection). |

The `seplos` submodule holds the wire mechanics: `checksum`, `command`, `error`, `fault`, `frame`,
`serial_number`, `telemetry`. The Seplos protocol is RS-485 ASCII: `~`-framed request/response,
hex-encoded fields, a 16-bit ASCII-sum two's-complement frame checksum, LENGTH = LENID + a checksum
nibble, and command codes `0x42` (telemetry) / `0x44` (fault) / `0xA4` (serial number).

## Voltronic/PI30 inverter parser (feature `voltronic`, decision 0001)

A second, independent vendor-protocol module — same crate, a different device class (RS-232
inverter rather than RS-485 BMS) behind its own trait
(`vendor::InverterVendorProtocol`/`InverterCommand`), added for the RS-232 Voltronic/MPP-Solar PI30
inverter leg (`docs/decisions/0001-rs232-inverter-leg-in-scope.md`, in-scope for gap-closure
wave 3). `Voltronic` (`voltronic::Voltronic`) is stateless and maps `InverterCommand::{Status,
Mode, Fault, Ratings}` to the ASCII commands `QPIGS` (status)/`QMOD` (mode)/`QPIWS` (fault/warning
bits)/`QPIRI` (ratings); `QID` (serial number) is issued directly by the caller at startup.

**Wire mechanics:** 2400 baud 8N1 ASCII. Request = command bytes + **CRC-16/XMODEM** (2 bytes,
big-endian) + `\r`; response = `(` + space-separated ASCII payload + CRC-16/XMODEM (2 bytes) + `\r`.
`voltronic::crc::crc16_xmodem` is a hand-rolled (no `crc` crate dependency) byte-at-a-time port of
the baseline Python `calculate_crc`, verified bit-for-bit against that algorithm's actual `python3`
output (`tests/voltronic_crc_vectors.rs` — the bounded-datum way to prove a CRC: run the real
algorithm, assert the exact output, never a guess). `voltronic::fields` decodes the
already-CRC-validated ASCII payload into canonical inverter types
(`CanonicalInverterReading`/`CanonicalInverterFault`/`InverterMode`/`InverterRatings`) — pure,
`no_std`, no floats (every fixed-point field parses straight from decimal ASCII into a scaled
integer).

Unlike the baseline Python reference (which **discards** the response CRC), this parser
**verifies** it (`validate_frame`) — a real CRC byte can equal `0x0D` (the frame terminator), which
is the caller's (driver-level) problem to mitigate via bounded continuation reads, not this
module's (see issue #268, a related open item: `SerialTransport` has no `discard_input()` yet to
clear stale RX bytes after a failed exchange).

**PROVISIONAL, no reference hardware:** this environment had no real Voltronic/QPIGS wire captures
when the parser was authored — every field layout is derived from the legacy Python reference plus
public PI30 protocol documentation, not a verified capture (decision 0001's open risk). Each
mapping choice is doc-commented at its site; `crates/lib-mapping/src/transform_inverter.rs` (the
downstream TMS mapper, `docs/tms-compliance.md` §4.8) carries the same caveat for the
canonical→TMS step.

`voltronic`'s own tests: `tests/voltronic_crc_vectors.rs` (CRC bit-for-bit vs. the Python
reference), `tests/voltronic_fields.rs` (field decode), `tests/voltronic_vectors.rs` (worked
request/response frame vectors).

## Dependencies and dependents

- **Depends on:** `bytes`, `heapless` (both `no_std`-friendly workspace deps), and `lib-schema`
  (the centralized TMS/canonical types — `canonical::CanonicalReading`/`CanonicalFault` are
  re-exports of `lib_schema`'s types, not defined here; see
  [`lib-schema`](./lib-schema.md)).
- **Depended on by:** `lib-mapping`, `lib-io`, `lib-mqtt`, `impl-app` (and
  `lib-io`'s dev-dependencies for the pty responder tests).

## Testing

Rich in-crate `tests/` directory, all built from capture-derived / spec vectors:

- `tests/seplos_vectors.rs` — worked frame vectors.
- `tests/fault_vector.rs` — the `0x44` fault decode.
- `tests/canonical.rs` — canonical model assertions.
- `tests/seplos_hardening.rs` — malformed / truncated / bad-checksum fault paths.
- `tests/seplos_telemetry_proptest.rs` — property-based telemetry decode (`proptest` dev-dep).

The Seplos frame parser is also the fuzz target `fuzz_parse_telemetry` (see
[`../usage.md`](../../deployment/usage.md)) and a mutation-testing target (`just mutants`).
