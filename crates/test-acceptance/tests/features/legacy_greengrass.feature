# AWS Greengrass backwards-compat merged flat battery JSON payload (Scope D, closes #52-#54,
# reconciled against the real baseline — gap-closure #269) — see
# docs/gap-analysis-meteor-parity.md's "Greengrass local IPC publish" finding.
#
# Exercises the real, reused lib_mapping::legacy::battery_payload builder — see
# tests/steps/legacy.rs's module doc comment for what this suite covers (the flat-JSON SHAPE) and
# does NOT cover (broker publish, mosquitto-bridge render — see
# test-e2e/tests/legacy_greengrass_e2e.rs for those).
#
# BASELINE-VERIFIED (docs/tms-compliance.md §7) — every battery scenario below DROPS the
# @provisional tag it carried before this reconciliation. /mnt/d/hvd/meteor-adapter's real
# example/aws/src/greengrass/telemetry/battery/{battery_reader.py,config.yaml} and
# common/device_telemetry_base.py are checked in and were read directly (see
# lib_mapping::legacy's own module doc comment): every field name, scale factor, and omission
# pinned here is a confirmed spec, not a best-effort derivation. (The Voltronic/inverter side of
# Scope D still has no canonical inverter-reading type to build against — it stays out of scope
# for this file until a later wave.)

Feature: AWS Greengrass backwards-compat merged flat battery JSON payload

  Scenario: The merged payload carries the caller's own configured device identity
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload device id is "METEOR-001"
    And the payload device type is "battery"
    And the payload dip address is 3

  Scenario: The merged payload's dip address tracks the Seplos address it was read from
    Given the golden canonical reading at Seplos address 10
    When the reading is rendered as a merged battery payload
    Then the payload dip address is 10
    But the payload device id is "METEOR-001"

  Scenario: The merged payload's timestamp is an ISO-8601 UTC string ending in Z
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload timestamp ends with "Z"

  Scenario: The merged payload mirrors the canonical reading's baseline-named electrical fields
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload current is approximately -1.44 A
    And the payload residual capacity is approximately 139.53 Ah
    And the payload battery capacity is approximately 180.0 Ah
    And the payload state of charge is approximately 77.5 percent
    And the payload state of health is approximately 100.0 percent
    And the payload has 2 num_cycles
    And the payload port voltage is approximately 54.53 V

  Scenario: The merged payload omits the baseline's disabled fields entirely
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload has no "total_voltage" key
    And the payload has no "rated_capacity" key
    And the payload has no "custom_number" key

  Scenario: The merged payload carries flat 0-based cell voltage keys and 1-based temperature keys
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload has 2 num_cells
    And payload cell_0_voltage is 3894 mV
    And payload cell_1_voltage is 3901 mV
    And payload temperature_1 is approximately 26.0 C

  Scenario: The merged payload has no location tag when none is available
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload has no "latitude" key
    And the payload has no "longitude" key
    And the payload location source is "unknown"

  Scenario: The merged payload embeds a location tag when one is available
    Given the golden canonical reading at Seplos address 3
    And a legacy location tag at lat 37.7749 lon -122.4194 from gps_nmea
    When the reading is rendered as a merged battery payload with its location tag
    Then the payload location source is "gps_nmea"
    And the payload latitude is approximately 37.7749

  Scenario: A payload with no fault polled yet omits every fault-flag key entirely
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload has no "high_voltage_protection" key
    And the payload has no "mosfet_fault" key

  @synthetic-vector
  Scenario Outline: Each set canonical fault flag merges into the SAME payload as its own 0/1 key
    Given the golden canonical reading at Seplos address 3
    And a canonical fault with flag <flag> set
    When the reading and fault are rendered as a merged battery payload
    Then the payload fault flag <flag> is 1

    Examples:
      | flag                                |
      | high_voltage_protection             |
      | low_voltage_protection              |
      | high_temperature_charge_protection   |
      | low_temperature_charge_protection    |
      | charge_overcurrent_protection        |
      | discharge_overcurrent_protection     |
      | short_circuit_protection             |
      | mosfet_fault                         |
      | cell_balance_warning                 |
      | low_capacity_warning                 |
      | high_temperature_discharge_warning   |
      | low_temperature_discharge_warning    |
      | low_voltage_warning                  |
      | high_voltage_warning                 |
      | discharge_current_warning            |
      | charge_current_warning               |

  Scenario: A fault with no flags set still merges every fault-flag key, each as 0
    Given the golden canonical reading at Seplos address 3
    And a canonical fault with no flags set
    When the reading and fault are rendered as a merged battery payload
    Then the payload fault flag high_voltage_protection is 0
    And the payload fault flag mosfet_fault is 0

  Scenario: The merged payload is valid JSON
    Given the golden canonical reading at Seplos address 3
    When the reading is rendered as a merged battery payload
    Then the payload is valid JSON with a string device_id
