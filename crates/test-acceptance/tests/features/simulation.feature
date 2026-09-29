# SeplosSim + BatteryStateSpec runtime simulation surface (#226 stage W2). Every battery state
# below is defined at RUNTIME through the W1 shared spec grammar (test_kit::seplos::
# parse_battery_state_spec, driven here from a Gherkin table) -- never a hand-assembled
# BatteryState literal or a new compile-time constant. Each scenario polls the simulator exactly
# like a real Seplos telemetry exchange (SeplosSim::respond + the real lib_parse decoder,
# in-process, no sockets/ptys) and asserts the canonical reading matches the spec field-for-field.
# Complements mapping.rs's fixed-fixture scenarios with fully runtime-defined battery states, and
# battery_state_spec.rs's Rust-level grammar tests with the acceptance-layer wire round-trip.

Feature: Runtime-defined battery states decode to matching canonical telemetry

  # ---- POSITIVE: table-driven battery states round-trip field-for-field ------------------------

  Scenario: A 16-cell full-capacity battery state round-trips through the simulator unchanged
    Given a battery state defined by the table:
      | key     | value                                                                          |
      | cells   | 4200,4198,4201,4199,4200,4197,4202,4198,4199,4200,4201,4198,4199,4200,4197,4201 |
      | temps   | 2981,2983,2985,2987,2989,2991,2993,2995                                        |
      | current | 0                                                                               |
      | soc     | 1000                                                                            |
      | soh     | 1000                                                                            |
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field
    And the canonical reading has 16 cells

  Scenario: A mid-discharge battery state round-trips through the simulator unchanged
    Given a battery state defined by the table:
      | key       | value          |
      | cells     | 3701,3699,3702 |
      | temps     | 2951,2961,2971 |
      | current   | -820           |
      | soc       | 455            |
      | remaining | 900            |
      | typical   | 1800           |
      | rated     | 1800           |
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field

  Scenario: A charging (positive current) battery state round-trips through the simulator unchanged
    Given a battery state defined by the table:
      | key     | value          |
      | cells   | 3550,3552,3551 |
      | current | 1250           |
      | soc     | 300            |
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field

  # ---- EDGE: bounds, extremes, and the override seam -------------------------------------------

  Scenario: The default sample14 battery state remains byte-stable on the wire
    Given the default sample14 battery state
    When the adapter polls telemetry
    Then the encoded telemetry reply is byte-identical to the accuracy fixture
    And the canonical reading matches the battery state field-for-field

  Scenario: Exactly MAX_CELLS cells are accepted end-to-end through the simulator
    Given a battery-state spec requesting 16 cells
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field
    And the canonical reading has 16 cells

  Scenario: MAX_CELLS + 1 cells is rejected before a simulator can even be built
    Given a battery-state spec requesting 17 cells
    Then the battery-state spec is rejected
    And the rejection message contains "too many cells"

  Scenario Outline: Extreme scalar values round-trip through the simulator unchanged
    Given a battery state defined by the table:
      | key     | value     |
      | current | <current> |
      | soc     | <soc>     |
      | temps   | <temps>   |
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field

    Examples:
      | current | soc  | temps |
      | -32768  | 0    | 0     |
      | 32767   | 1000 | 65535 |

  Scenario: A deliberately-inconsistent total-voltage override propagates through the wire
    Given a battery state defined by the table:
      | key   | value          |
      | cells | 1000,2000,3000 |
      | total | 1              |
    When the adapter polls telemetry
    Then the canonical reading matches the battery state field-for-field
