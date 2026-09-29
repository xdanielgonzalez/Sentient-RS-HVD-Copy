# Seplos 0x44 fault/warning decode -> CanonicalFault -> TMS ActiveDiagnosticState (DTCs).
# Exercises the real decoder (lib_parse::seplos::fault::decode_fault), the 16 boolean flags +
# any() (lib_canonical::CanonicalFault), and the real fault<->DTC mapper
# (lib_mapping::transform::{ToDiagnostics, FromDiagnostics}, backed by FAULT_TABLE).
#
# Schema oracle: TMS DiagnosticTroubleCode / ActiveDiagnosticState IDL `chapter-d.txt` lines
# 8149-8377; adapter mapping documented in docs/tms-compliance.md §3.3.
#
# HONESTY TAGGING (docs/tms-compliance.md §7) — the whole feature is @provisional because every
# scenario rests on an empirical / synthetic / placeholder input, even though the code genuinely
# behaves as asserted TODAY. Run everything except this file with:
#   cargo test -p test-acceptance --test cucumber -- --tags "not @provisional"

@provisional
Feature: Seplos fault-domain decode and DTC mapping (provisional inputs)

  @empirical-faultmap @synthetic-vector
  Scenario Outline: Each (byte,bit) of the empirical fault map decodes to exactly one flag
    # PROVISIONAL: the (byte,bit)->flag map is observed-on-hardware in the prototype, NOT confirmed
    # against the Seplos protocol spec (lib-parse/src/seplos/fault.rs:3-6, tms-compliance §7a);
    # and no genuine 0x44 capture exists yet, so the frame is a synthesized vector (§7a).
    Given a synthetic Seplos fault frame with INFO byte <byte> bit <bit> set
    When the fault frame is decoded
    Then the decoded fault sets only the <flag> flag

    Examples:
      | flag                                | byte | bit |
      | high_voltage_protection             | 3    | 0   |
      | low_voltage_protection              | 3    | 1   |
      | high_temperature_charge_protection  | 4    | 0   |
      | low_temperature_charge_protection   | 4    | 1   |
      | charge_overcurrent_protection       | 5    | 0   |
      | discharge_overcurrent_protection    | 5    | 1   |
      | short_circuit_protection            | 5    | 2   |
      | mosfet_fault                        | 6    | 0   |
      | cell_balance_warning                | 7    | 0   |
      | low_capacity_warning                | 7    | 1   |
      | high_temperature_discharge_warning  | 8    | 0   |
      | low_temperature_discharge_warning   | 8    | 1   |
      | low_voltage_warning                 | 9    | 0   |
      | high_voltage_warning                | 9    | 1   |
      | discharge_current_warning           | 10   | 0   |
      | charge_current_warning              | 10   | 1   |

  Scenario: any() is true when a protection or warning flag is set
    Given the fault has flag short_circuit_protection set
    Then the decoded fault reports any as true

  Scenario: any() is false when no flag is set
    Given the fault has no flags set
    Then the decoded fault reports any as false

  @placeholder-spn
  Scenario Outline: Each set flag emits exactly one Major/ConditionExists DTC at its SPN and hint
    # PROVISIONAL: FAULT_TABLE SPNs 1-16 are adapter-local integers, not assigned J1939 SPNs
    # (transform.rs, tms-compliance §7c); severity is uniformly Major and occurrence_count is
    # uniformly 1 — the adapter carries no per-fault classification.
    Given the fault has flag <flag> set
    When the fault is mapped to diagnostics
    Then there is exactly one diagnostic code
    And the diagnostic code has suspect parameter <spn>
    And the diagnostic code hint is <hint>
    And the diagnostic code is a Major ConditionExists fault occurring once

    Examples:
      | flag                                | spn | hint                    |
      | high_voltage_protection             | 1   | "cell overvoltage"      |
      | low_voltage_protection              | 2   | "cell undervoltage"     |
      | high_temperature_charge_protection  | 3   | "hi-temp charge"        |
      | low_temperature_charge_protection   | 4   | "lo-temp charge"        |
      | charge_overcurrent_protection       | 5   | "charge overcurrent"    |
      | discharge_overcurrent_protection    | 6   | "discharge overcurrent" |
      | short_circuit_protection            | 7   | "short circuit"         |
      | mosfet_fault                        | 8   | "mosfet fault"          |
      | cell_balance_warning                | 9   | "cell imbalance"        |
      | low_capacity_warning                | 10  | "low capacity"          |
      | high_temperature_discharge_warning  | 11  | "hi-temp discharge"     |
      | low_temperature_discharge_warning   | 12  | "lo-temp discharge"     |
      | low_voltage_warning                 | 13  | "low voltage warn"      |
      | high_voltage_warning                | 14  | "high voltage warn"     |
      | discharge_current_warning           | 15  | "discharge current"     |
      | charge_current_warning              | 16  | "charge current"        |

  @placeholder-spn
  Scenario: A set flag round-trips flag -> DTC -> JSON codec -> DTC -> flag
    # PROVISIONAL: rides the same adapter-local SPN table (§7c) as the write path.
    Given the fault has flag mosfet_fault set
    When the fault round-trips through the diagnostics JSON codec
    Then the round-tripped fault sets only the mosfet_fault flag

  Scenario: A DTC with an unrecognised (non-Seplos) suspect parameter is silently dropped on decode
    Given a diagnostics payload carrying only suspect parameter 9999
    When the diagnostics payload is decoded to canonical faults
    Then the decoded fault reports any as false

  @empirical-faultmap @synthetic-vector
  Scenario: A short fault INFO payload reads its missing bytes as clear and never errors
    # PROVISIONAL: empirical map + synthetic vector (§7a). A missing INFO byte leaves its flags
    # false (never an error) per fault.rs — resilient to a device that returns a short 0x44 frame.
    Given a synthetic Seplos fault frame with only 5 INFO bytes
    When the fault frame is decoded
    Then the fault decode succeeds
    And the decoded fault reports any as false
