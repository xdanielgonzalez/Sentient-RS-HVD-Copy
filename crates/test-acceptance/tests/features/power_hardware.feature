# tms::PowerHardwareUpdate — IDL `chapter-d.txt` lines 8035-8128.
# Compliance: docs/tms-compliance.md §3.2 ("tms::PowerHardwareUpdate -> PowerHardwareUpdate").
# For a battery device, `energy_storage` is the ONLY populated subsystem (engine/fuel/generator/
# power_electronics/thermal are all @optional and omitted — see §3.2 of the compliance doc); the
# nested EnergyStorageState/StorageCellState field shapes are covered by energy_storage.feature.

Feature: PowerHardwareUpdate carries energy storage as its only populated subsystem

  Scenario: A discharging pack's power hardware update carries populated energy storage
    Given the tms_transform sample reading (3 cells, 2 temps, discharging 1.44 A)
    When the reading is mapped to power hardware
    Then there are 3 storage cells
    And the minimum temperature is approximately 7.0 degC
    And the maximum temperature is approximately 8.0 degC
    And the average temperature is approximately 7.5 degC
    And storage cell 0 voltage is approximately 3.894 V
    And storage cell 0 state of charge is approximately 0.775
    And storage cell 0 cycle counter is approximately 2.0
