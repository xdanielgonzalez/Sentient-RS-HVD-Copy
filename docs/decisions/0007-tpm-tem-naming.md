# 0007 — TPM/TEM naming locked: TEM = battery, TPM = inverter

**Status:** Locked (2026-07-16)

## Context

The legacy Python prototype (`/mnt/d/hvd/meteor-adapter`, reference-only — see `KICKOFF.md`'s
"reference-only" framing) uses two METEOR acronyms, TEM and TPM, to tag the two device classes it
polls, and — as decisions 0001 (both the original descope and the superseding in-scope record) and
0002 already flag — the legacy code is internally inconsistent about which acronym means which
device. That inconsistency was carried forward as an open question rather than resolved; this
record resolves it against the weight of evidence, not either single contradictory file.

**The evidence, all read-only from `/mnt/d/hvd/meteor-adapter`:**

- `example/aws/src/field-mappings/tem_field_mappings.json` — every field is a battery/BMS reading:
  `state_of_charge` → `soc`, `cell_0_voltage`…`cell_13_voltage`, `num_cycles`, `state_of_health`,
  `high_voltage_protection`, `mosfet_fault`, `dip_address`, etc. No inverter concept appears.
- `example/aws/src/field-mappings/tpm_field_mappings.json` — every field is an inverter reading:
  `variant`, `output_mode`, `grid_voltage`, `grid_frequency`, `ac_output_voltage`,
  `ac_output_frequency`, `ac_output_apparent_power`, `ac_output_active_power`, `pv_voltage`,
  `pv_charging_power`, `inverter_temperature`, `battery_type` (the inverter's *configured battery
  type* setting, not a battery device itself). No battery-BMS concept appears.
- `example/aws/README.md` line 3: "Collects high-frequency telemetry from battery modules (TEM)
  and inverters (TPM), streams data to AWS…" — restated at lines 63, 109-110, 268-269, 1154.
- `example/aws/src/greengrass/cot-forwarder/main.py` — line 62 labels a battery-branch device
  `"(TEM)"`, line 126 labels an inverter-branch device `"(TPM)"`, and line 234's ternary makes it
  explicit: `" (TEM)" if device_type == "battery" else " (TPM)" if device_type == "inverter"`.
- `example/aws/src/greengrass/kinesis-forwarder/main.py` — lines 227-237: `# Load TEM (battery)
  config` / `# Load TPM (inverter) config`, loading `tem_field_mappings`/`tpm_field_mappings`
  accordingly.
- `example/aws/lib/mcp-agent/docker-image/server.py` lines 160-161 (the natural-language agent's
  own system prompt, so it has to get this right to answer questions correctly): `"TEM (Battery):
  Energy storage with SoC%, voltage, current, protection/warning flags"` / `"TPM (Inverter): DC-AC
  converter with load, solar input, fault flags"`.

Four independent subsystems (field-mapping catalogue, CoT forwarder, Kinesis forwarder, MCP agent)
agree: **TEM = battery, TPM = inverter.**

**The buggy inversions**, both in `example/aws/src/greengrass/telemetry/`:

- `recipe.yaml` line 14: `testModeType: "tem"  # "tem" for inverter, "tpm" for battery` — the
  comment has the mapping backwards relative to every other subsystem above.
- `main.py` lines 201-204:
  ```python
  if test_mode_type == 'tem':
      device_type = 'inverter'
  elif test_mode_type == 'tpm':
      device_type = 'battery'
  ```
  This is the same inversion as `recipe.yaml`, and it contradicts **its own file's** comments at
  lines 135-147 (`# Test connections - INVERTER/TPM FIRST (faster RS232 response), then
  BATTERY/TEM`), which have the mapping the *correct* way round. `main.py` disagrees with itself.

Given a 6-source agreement (field mappings ×2, README, cot-forwarder, kinesis-forwarder, mcp-agent)
against one file's test-mode branch that also contradicts its own comments, the test-mode code is
the bug, not the ground truth.

**Naming collision to flag:** MIL-STD-3071's own acronym list uses **TPM = Trusted Platform
Module** — an unrelated hardware-security sense already in active use elsewhere in this repo (see
`docs/scope-e-tpm-signing-report.md`, the SLB9672 SPI TPM 2.0 HAT, `docs/audit-2026-07.md`'s Scope
E). Any doc or conversation using "TPM" for the METEOR inverter sense must disambiguate explicitly
— the two senses will collide in this codebase specifically, not just in the abstract.

## Decision

**TEM = battery = `ROLE_STORAGE`. TPM = inverter = `ROLE_CONVERSION`.** Both role names are real
MIL-STD-3071 `DeviceRole` enumerators (`docs/compliance/tms/mil-std-3071_data_model.idl`, lines
481-491: `ROLE_STORAGE` — "Storage power device"; `ROLE_CONVERSION` — "Conversion power device").
This resolves the acronym question against the interface/evidence, per decisions 0001 and 0002's
open flag, and gives the inverter leg (decision 0001, in-scope) a concrete TMS role to ship under.

## Consequences

- The inverter's TMS device role is `ROLE_CONVERSION`, not `ROLE_STORAGE` — a distinct role from
  the battery proxy's `ROLE_STORAGE` (decision 0003's original STOR call, now confirmed against the
  real IDL rather than the "not IDL-verified" placeholder that decision recorded at the time).
- **Per-bus identity derives role from bus protocol**, not from a single global device role: the
  RS-485/Seplos bus maps to `ROLE_STORAGE`, the RS-232/Voltronic bus maps to `ROLE_CONVERSION`. A
  unit with both legs wired (decision 0001, in-scope) genuinely runs two roles concurrently, one
  per bus — see decision 0003's "Superseded in part" section for the mechanical consequence.
- Every doc/comment/identifier introduced under this naming must say which sense of "TPM" it means
  wherever the METEOR sense is used near the security sense, to avoid the collision flagged above.
- The `recipe.yaml`/`main.py` inversion is now a **confirmed bug**, not an ambiguity — anyone
  porting the legacy test-mode branch must invert it back before reuse.

## Revisit trigger

- An authoritative METEOR program document (not the reference prototype) states the acronym
  mapping differently and contradicts the field-mapping evidence above — at that point the
  authoritative source wins and this record is superseded.
- MIL-STD-3071 issues a version that renames or removes `ROLE_STORAGE`/`ROLE_CONVERSION`.
