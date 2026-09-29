# ActiveDiagnosticState.overflow — IDL `chapter-d.txt` lines 10053-10111 (field `overflow` @
# 10107-10108). Counts DTCs beyond the 64-entry `codes` cap (MAX_DTC = 64,
# lib-tms/src/lib.rs; confirmed equal to the IDL MAXLEN 64, tms-compliance.md §3.3).
#
# transform.rs's FAULT_TABLE has exactly 16 boolean protection/warning flags, so no Seplos-sourced
# fault set can ever actually overflow the 64-code cap. This feature documents that structural
# headroom honestly, rather than fabricating an impossible 17th flag to force an overflow that the
# real protocol can never produce.

Feature: DTC overflow accounting has headroom under Seplos's fixed 16-flag fault table

  Scenario: Every possible Seplos fault flag still fits comfortably under the 64-code cap
    Given a canonical fault with every flag set
    When the faults are mapped to active diagnostics
    Then there are 16 diagnostic codes
    And there is 0 diagnostic overflow
