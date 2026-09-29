# 0001 — RS-232 inverter leg descoped this round

**Status:** Locked (2026-07-16)

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

Neither the Python prototype's own `src/` nor sentient-rs implements any inverter protocol today;
the rewrite has only ever covered the battery leg. KICKOFF's deliverable text ("the RPi comms
module at each TPM/TEM") does not itself resolve whether both device classes are in scope for this
round.

## Decision

**Descope the RS-232 inverter leg for this round.** sentient-rs ships with telemetry coverage for
the RS-485 battery/Seplos path only. No Voltronic/QPIGS parser, no second serial leg, no inverter
TMS mapping is built in this pass.

## Consequences

- A METEOR unit with an inverter module has **no telemetry path for it** — only the battery side
  reports. This is a known, accepted gap, not an oversight.
- `bin-rpi`'s config/runtime seams (`[[bus]]`, `BusConfig`, `HostPlatform::build_port` — see
  decision 0004) are protocol-parametric enough that adding a second protocol later is an additive
  change (a new `SerialTransport`-consuming parser behind the existing bus abstraction), not a
  redesign — but no inverter-specific code exists to make that concrete yet.
- The acronym contradiction in the legacy code (TPM/TEM swap between `main.py` and `recipe.yaml`)
  is noted here for whoever picks this back up — resolve it against the interface, not either
  legacy label, since the interfaces (RS-485 vs RS-232) are the only unambiguous ground truth.

## Revisit trigger

- An actual inverter deployment is scheduled, or
- Real Voltronic/QPIGS wire captures become available to derive/verify a parser against (this
  environment had none — see CLAUDE.md's reference-prototype policy), or
- A stakeholder confirms the inverter leg is contractually required for the current rollout.
