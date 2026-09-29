# RS-232 Voltronic inverter leg — Canonical -> TMS CONV-role mapping (D10, decision 0001).
#
# All @provisional: every port assignment, SPN, and scale below is DERIVED from the legacy Python
# reference + protocol docs, with NO real Voltronic/QPIGS wire captures (decision 0001's open risk).
# The mapping is exercised end-to-end through the REAL reused code — `test_kit::VoltronicSim` (the
# shared-CRC sim), `lib_parse::voltronic` (the real decoder), and `lib_mapping::transform_inverter`
# (the real CONV mapper) — never re-implemented in step code.

@provisional
Feature: Voltronic inverter Canonical -> TMS mapping

  Scenario: QPIGS status maps to two external AC ports (output + grid)
    Given a Voltronic QPIGS status reading from the sim
    When the inverter reading is mapped to an AC measurement
    Then there are 2 external AC measurement ports
    And AC port 0 voltage is approximately 230.0 V
    And AC port 1 voltage is approximately 230.4 V
    And AC port 1 amperage is not available

  Scenario: QPIGS status maps to two external DC ports (PV + battery, source-positive)
    Given a Voltronic QPIGS status reading from the sim
    When the inverter reading is mapped to a DC measurement
    Then there are 2 external DC measurement ports
    And DC port 2 voltage is approximately 380.0 V
    And DC port 3 voltage is approximately 51.2 V
    And DC port 3 amperage is approximately -8.0 A

  Scenario: QPIGS status maps power electronics temperature only
    Given a Voltronic QPIGS status reading from the sim
    When the inverter reading is mapped to power hardware
    Then the power electronics temperature is approximately 27.0 C
    And there is no energy storage state

  Scenario: QPIWS overload fault maps to a Major SPN 115 DTC, disjoint from battery SPNs
    Given a Voltronic QPIWS fault with the overload bit set
    And the inverter is in Line mode
    When the inverter fault is mapped to diagnostics
    Then the diagnostic count is 1
    And there is a diagnostic with SPN 115
    And every diagnostic is Major severity with a ConditionExists failure mode
    And every inverter SPN is disjoint from the battery SPN range

  Scenario: A Fault operating mode synthesizes an SPN 130 DTC (QMOD has no faithful TMS home)
    Given the inverter reports a Fault mode
    When the inverter mode is mapped to diagnostics
    Then the diagnostic count is 1
    And there is a diagnostic with SPN 130
