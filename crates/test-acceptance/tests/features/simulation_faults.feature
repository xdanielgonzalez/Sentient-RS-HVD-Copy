# SeplosSim fault-injection negative scenarios (#226 stage W2), companion to simulation.feature.
# Every scenario asserts a SPECIFIC typed outcome (a lib_parse::vendor::ParseError variant, or the
# distinct "Silence" outcome for a genuinely non-responding bus) -- never just is_err(). The
# battery state comes from the same W1 spec-grammar path as simulation.feature (here, the
# `sample14` baseline, since these scenarios exercise the response-side behavior matrix / the
# request-side auto-NAK classifier, not the battery-state fields themselves).

Feature: SeplosSim fault-injection behaviors resolve to typed decode outcomes

  Background:
    Given the default sample14 battery state

  # ---- NEGATIVE: the response-side Behavior matrix ----------------------------------------------

  Scenario Outline: A SeplosSim fault-injection behavior resolves to its typed telemetry outcome
    Given the simulator behavior is <behavior>
    When the adapter polls telemetry
    Then the telemetry outcome is <outcome>

    Examples:
      | behavior              | outcome         |
      | corrupted checksum    | BadChecksum     |
      | a wrong address reply | AddressMismatch |
      | truncated to 10 bytes | BadFraming      |
      | silence               | Silence         |

  # ---- NEGATIVE: forced RTN-error codes surface exactly to the decoder --------------------------

  Scenario Outline: A forced RTN-error behavior surfaces its exact code to the real decoder
    Given the simulator behavior is forced RTN error <code>
    When the adapter polls telemetry
    Then the reply is a device error with code <code>

    Examples:
      | code |
      | 2    |
      | 3    |
      | 4    |

  # ---- NEGATIVE: auto-NAK classifies a malformed REQUEST into the matching RTN code -------------

  Scenario: Auto-NAK answers a corrupted request frame checksum with RTN 02
    Given auto-NAK is enabled on the simulator
    When a request with a corrupted frame checksum is sent
    Then the reply is a device error with code 2

  Scenario: Auto-NAK answers a corrupted request length checksum with RTN 03
    Given auto-NAK is enabled on the simulator
    When a request with a corrupted length checksum is sent
    Then the reply is a device error with code 3

  Scenario: Auto-NAK answers an unrecognized request command code with RTN 04
    Given auto-NAK is enabled on the simulator
    When a request with an unrecognized command code is sent
    Then the reply is a device error with code 4
