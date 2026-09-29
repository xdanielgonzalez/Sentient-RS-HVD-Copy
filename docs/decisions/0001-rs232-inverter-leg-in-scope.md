# 0001 — RS-232 inverter leg in scope this round

**Status:** Locked (2026-07-16, supersedes the earlier descope call)

## Context

The legacy AWS Greengrass pipeline (`example/aws/.../inverter/`, not sentient-rs's own port
target — see `KICKOFF.md`'s "reference-only" framing) polls **two** devices per Pi: the
battery/Seplos BMS on `/dev/ttySC0` (RS-485, the leg this repo implements) and an inverter on
`/dev/ttySC1` (RS-232 @ 2400 baud, a Voltronic/MPP-Solar-style ASCII protocol — `QID` serial
query, `QPIGS` status polls, CRC-16/XMODEM, `\r`-terminated). This is the second METEOR device
class. `docs/gap-analysis-meteor-parity.md`'s "Legacy AWS pipeline — not re-homed" section
documents it in full, including a real contradiction in the legacy code: `main.py:135-147` labels
inverter=TPM/battery=TEM, while `recipe.yaml:14` has it the other way around — the interface
mapping (RS-485=battery, RS-232=inverter) is the only unambiguous part.

KICKOFF's deliverable text ("the RPi comms module at each TPM/TEM") covers **both** device
classes; a METEOR unit with an inverter module needs a telemetry path for it, so the inverter leg
belongs in this round, not a later one.

## Decision

**Include the RS-232 inverter leg this round.** sentient-rs ships telemetry coverage for both the
RS-485 battery/Seplos path **and** the RS-232 Voltronic/QPIGS inverter path: a `QPIGS`/`QID`
ASCII parser (CRC-16/XMODEM, `\r`-terminated framing), a second serial leg on the multi-port bus
abstraction, and the inverter TMS mapping.

## Consequences

- A METEOR unit with an inverter module reports on **both** sides — battery and inverter — for
  full device coverage.
- The inverter parser is a new `SerialTransport`-consuming parser behind the existing bus
  abstraction (`[[bus]]`, `BusConfig`, `HostPlatform::build_port` — see decision 0004). Because
  those seams are protocol-parametric, this is an additive change, not a redesign.
- The acronym contradiction in the legacy code (TPM/TEM swap between `main.py` and `recipe.yaml`)
  must be resolved against the interface, not either legacy label — the interfaces (RS-485 vs
  RS-232) are the only unambiguous ground truth. This now blocks the inverter TMS mapping and must
  be settled before that mapping ships.
- **Open risk:** this environment had no real Voltronic/QPIGS wire captures (see CLAUDE.md's
  reference-prototype policy). The parser is derived from the legacy Python + protocol docs and
  must be verified against real captures — or a hardware bench — before an inverter deployment is
  trusted.

## Revisit trigger

- Real Voltronic/QPIGS wire captures contradict the derived parser, or
- An inverter model outside the Voltronic/MPP-Solar ASCII family appears in a deployment (different
  command set / framing), or
- The TPM/TEM label contradiction is resolved in a way that changes the inverter TMS role mapping.
