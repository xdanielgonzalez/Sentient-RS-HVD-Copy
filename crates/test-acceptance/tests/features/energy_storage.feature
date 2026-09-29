# Nested tms::EnergyStorageState — IDL `chapter-d.txt` lines 6197-6323.
# Nested tms::StorageCellState — IDL `chapter-d.txt` lines 6064-6169.
# Compliance: docs/tms-compliance.md §3.2.1.
# Covers the per-cell field derivation: voltage (mV->V), per-cell temperature (nearest sensor,
# clamped to the last sensor when cells outnumber sensors), and the read/decode-direction
# reconstruction of temperatures_raw documented as EXACT in that case (transform.rs's
# `FromTms` doc).

Feature: EnergyStorageState / StorageCellState field derivation

  Scenario: Each storage cell voltage matches the mV -> V scaling of its Seplos cell reading
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to power hardware
    Then storage cell 0 voltage is approximately 3.894 V
    And storage cell 1 voltage is approximately 3.893 V
    And storage cell 2 voltage is approximately 3.895 V

  Scenario: Per-cell temperature is the nearest sensor reading, clamped to the last sensor
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to power hardware
    Then storage cell 0 temperature is approximately 7.0 degC
    And storage cell 1 temperature is approximately 8.0 degC
    And storage cell 2 temperature is approximately 8.0 degC

  Scenario: Temperatures reconstruct exactly on read-back when cells outnumber sensors
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to a DC measurement
    And the reading is mapped to power hardware
    And the dc and hw payloads round-trip through JSON
    Then the round-tripped reading matches the original telemetry
    And the round-tripped temperatures raw match the original
