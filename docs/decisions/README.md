# Decision records

Architecture Decision Records (ADRs) for the gap-closure umbrella story (#237) — the "decide,
don't drift" items `docs/gap-analysis-meteor-parity.md` flagged as needing an explicit, written
call rather than a silent default. Each record follows the same five-field shape: **Status**
(locked/open/superseded), **Context**, **Decision**, **Consequences**, **Revisit-trigger**.

These are records of calls already made (dated 2026-07-16 unless noted) — they document *why*,
not a proposal awaiting approval. Cross-reference: every "decide, don't drift" item the gap
analysis lists has a record here; see that document's §"Recommended path to parity", step 6.

| # | Title | One-line decision |
|---|---|---|
| [0001](./0001-rs232-inverter-leg-in-scope.md) | RS-232 inverter leg **in scope** *(supersedes the original descope call)* | The second METEOR device class (Voltronic/QPIGS inverter over RS-232) is in scope this round after all — both files exist ([descoped](./0001-rs232-inverter-leg-descoped.md), superseded, kept for history; [in-scope](./0001-rs232-inverter-leg-in-scope.md), current) — in-scope wins. |
| [0002](./0002-runtime-hw-autodetect-descoped.md) | Runtime hardware auto-detect descoped | Configured DIP addresses stand; no startup bus-scan/auto-identify. |
| [0003](./0003-tms-device-role-stor.md) | TMS device role = STOR *(superseded in part 2026-07-16 — see 0007)* | This adapter is a STOR-role power device proxying the battery, never a passive MON — makes Heartbeat/DeviceInfo/Reply mandatory, not optional. Role is now per-bus (0007) and the Reply posture is now spec-clean against the landed IDL, not this record's original generic-topic/unconditional-Ok shape. |
| [0004](./0004-multi-port-in-process.md) | Multi-port serial = in-process | One `bin-rpi` process supervises every configured `[[bus]]`, sharing one embedded broker — not one process per bus. |
| [0005](./0005-rs485-direction-mechanism.md) | RS-485 direction mechanism = config seam | RTS+drain is the default; kernel `TIOCSRS485` and a dedicated GPIO DE/RE line are selectable per port. Field task: verify on the shipping HAT. |
| [0006](./0006-meshtastic-protobuf-approach.md) | Meshtastic framing = hand-rolled minimal protobuf | A ~20-line varint/length-delimited encoder for the one message shape needed, not the full `meshtastic`/`prost` dependency graph. |
| [0007](./0007-tpm-tem-naming.md) | TPM/TEM naming locked | TEM = battery = `ROLE_STORAGE`, TPM = inverter = `ROLE_CONVERSION` — resolves the legacy code's internal acronym contradiction against the weight of evidence. |
| [0008](./0008-passive-serial-streaming-descoped.md) | Passive/unsolicited serial streaming descoped | Serial stays strictly request/response; the Voltronic CRC-verify + bounded continuation-read hook landing this round is the recorded stopping point — true passive streaming and a generic framing layer stay descoped. |
| [0009](./0009-spec-app-ports-and-adapters.md) | Ports & adapters: extract the app contract into `spec-app` | The `Platform` port + app vocabulary (`RunConfig`/`Health`) move to a new contract crate `spec-app`; `impl-app` becomes pure policy; the RPi platform (`HostPlatform`) externalizes to a `bin-rpi-platform` adapter symmetric with `bin-mcu-platform`; binaries become thin composition roots. |
| [0010](./0010-mcu-runtime-embassy.md) | MCU runtime pivot Zephyr → Embassy | The Cortex-M target drops Zephyr for **Embassy** (`bin-mcu-platform` + the excluded `bin/mcu-rp2350` workspace, modelled on `hil/pico-seplos-emu`); the Pi is unchanged (Yocto/std/tokio). Bootloader Gate-0-deferred; **no RP2350 emulation exists** — verification is build/link + HIL smoke on the bench Pico 2. |
| [0011](./0011-mcu-security-boundary.md) | MCU security boundary: three concentric boundaries (RP2350) | Silicon RoT + SE (hardens the key, does not anchor the chain) ▸ SAU S/NSC/NS (keys-in-S, protocol-in-NS, plaintext-never-in-S, C `-mcmse` veneers) ▸ MPU_NS + optional core split (fault containment via a by-value channel boundary). Hosts the **RP2350 memory-layout diagram**. Demo scaffold, A4-only; explicit non-claims (no FIPS/attestation/anti-tamper; DMA bypasses SAU). |

## Why these eight and not more

The gap analysis's "decide, don't drift" list also named location tagging and the multi-port
config surface; location tagging is covered by the multi-tier fallback design itself (see
`lib-net`'s `locate` module in the [API reference](../reference/README.md), no separate binary in/out call needed —
it's opt-in by `[location]` section presence, same as disk/Meshtastic/Greengrass), and the
multi-port *config* surface is the direct mechanical consequence of decision 0004, not a second
decision.

0007 and 0008 are additions from the 2026-07-16 re-verification round (the IDL landing and the
inverter leg going in-scope respectively raised them), not from the original six's "decide, don't
drift" list — they're recorded here because they're the same kind of explicit, written call, not
because the umbrella story's original list named them.

Note also that 0001 keeps its original number across both its descoped and in-scope files rather
than taking a new number — the decision *content* superseded itself; the numbering didn't need to
move to record that.

**0009–0011 are a different lineage.** They are not gap-closure "decide, don't drift" items — 0009
records a crate-architecture / dependency-injection call from the post-consolidation work
(#285–#289), and **0010–0011 record the MCU security-path calls from EPIC #350** (the Zephyr→Embassy
runtime pivot and the three-concentric-boundary security architecture, story S6 #357). All three
are kept in this same log because they are the same kind of explicit, written architectural
decision.
