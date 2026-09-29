# tms::ActiveDiagnosticState — IDL `chapter-d.txt` lines 10053-10111.
# Nested tms::DiagnosticTroubleCode — IDL `chapter-d.txt` lines 8149-8377.
# Compliance: docs/tms-compliance.md §3.3.
# Every SET Seplos protection/warning flag emits exactly one Major/ConditionExists DTC
# (transform.rs's FAULT_TABLE); the read direction reverses through the same table and silently
# drops any DTC whose suspect_parameter is not a recognised Seplos SPN.

Feature: ActiveDiagnosticState / DiagnosticTroubleCode fault mapping

  Scenario: Two set fault flags map to two Major ConditionExists DTCs
    Given a canonical fault with flag high_voltage_protection set
    And a canonical fault with flag short_circuit_protection set
    When the faults are mapped to active diagnostics
    Then there are 2 diagnostic codes
    And there is 0 diagnostic overflow
    And every diagnostic code is a Major ConditionExists fault with no optional fields

  Scenario: No set flags produce no diagnostic codes
    Given a canonical fault with no flags set
    When the faults are mapped to active diagnostics
    Then there are 0 diagnostic codes
    And there is 0 diagnostic overflow

  Scenario: Diagnostics round-trip losslessly through JSON and recover the same flags
    Given a canonical fault with flag high_voltage_protection set
    And a canonical fault with flag mosfet_fault set
    When the faults are mapped to active diagnostics
    And the active diagnostic state round-trips through JSON
    Then the decoded fault has flag high_voltage_protection set
    And the decoded fault has flag mosfet_fault set
    And the decoded fault has flag low_voltage_protection not set

  Scenario: A DTC from an unrecognised SPN has no canonical flag and is silently dropped
    Given a canonical fault with flag high_voltage_protection set
    And a canonical fault with flag short_circuit_protection set
    When the faults are mapped to active diagnostics
    And diagnostic code 0 suspect parameter is overwritten to 999999
    And the diagnostics are decoded back into canonical fault flags
    Then the decoded fault has flag high_voltage_protection not set
    And the decoded fault has flag short_circuit_protection set
