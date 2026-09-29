# Honesty tagging per docs/tms-compliance.md §7 ("Caveats and known gaps"): these scenarios
# exercise real adapter behavior TODAY, but the behavior itself rests on an empirical, synthetic,
# placeholder, or unconfirmed input. They RUN and stay GREEN (the code genuinely behaves this
# way) — the tag renders them as visibly-unconfirmed, not as a reason to skip or hide them.
#
# Run everything EXCEPT this file's scenarios with: cargo test -p test-acceptance --test
# cucumber -- --tags "not @provisional"

@provisional
Feature: Provisional / unconfirmed spec-alignment scenarios

  @empirical-faultmap
  Scenario: The Seplos fault (byte,bit) map is empirical, not confirmed against the vendor spec
    # PROVISIONAL: lib-parse/src/seplos/fault.rs's module doc flags this decode as observed
    # on live hardware in the prototype, NOT confirmed against the Seplos protocol spec
    # (tms-compliance.md §7a).
    Given a canonical fault with flag high_voltage_protection set
    When the faults are mapped to active diagnostics
    Then there is 1 diagnostic code

  @synthetic-vector
  Scenario: No real 0x44 fault capture exists yet — this vector is synthesized, not captured
    # PROVISIONAL: no genuine Seplos 0x44 fault-frame capture exists yet (tms-compliance.md §7a);
    # this scenario's SeplosSim-encoded fault frame is synthetic, built per the protocol
    # description rather than sourced from real hardware.
    Given the 14-cell sample battery
    And the simulated battery reports fault flag mosfet_fault set
    When device 0 polls for faults
    Then the read should succeed
    And the decoded fault has flag mosfet_fault set

  @placeholder-spn
  Scenario: Suspect-parameter SPNs 1-16 are adapter-local placeholders, not assigned J1939 SPNs
    # PROVISIONAL: transform.rs's FAULT_TABLE SPNs 1-16 are adapter-local integers, not assigned
    # J1939 SPNs (tms-compliance.md §7c); occurrence_count is uniformly 1 and severity is
    # uniformly Major — no per-fault classification.
    Given a canonical fault with flag high_voltage_protection set
    When the faults are mapped to active diagnostics
    Then diagnostic code 0 has suspect parameter 1
    And every diagnostic code is a Major ConditionExists fault with no optional fields

  @unverified-maxlen
  Scenario: MAX_PORTS / MAX_STORAGE_CELLS bounds are documented assumptions, not confirmed IDL values
    # PROVISIONAL: the concrete IDL values of MAX_PORTS (DC PowerPortMeasurementSequence) and
    # MAXLEN_storageCells were not located in chapter-d.txt; the adapter's caps (4 and 50) are
    # documented assumptions, not verified matches (tms-compliance.md §7d).
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to a DC measurement
    And the reading is mapped to power hardware
    Then there is 1 external measurement port
    And there are 3 storage cells
