# Seplos decode hardening — every malformed / adversarial input must fail with a SPECIFIC typed
# error (lib_parse::seplos::error::SeplosError / vendor::ParseError variant), never a panic
# and never a silent wrong answer. Mirrors lib-parse/tests/seplos_hardening.rs at the
# acceptance layer: crafted single-defect frames (checksum-valid except for the one field under
# test) plus the SeplosSim Behavior fault-injection matrix.
#
# NOTE: SeplosError::MissingEoi is intentionally UNREACHABLE — the frame parser is lenient about a
# dropped trailing CR (timeout-framed reads), so no input produces it (see frame.rs Step 2 and the
# reserved-variant doc in error.rs). It is deliberately NOT asserted here.

Feature: Seplos decode rejects malformed input with specific typed errors

  Scenario Outline: A crafted frame with one defect is rejected with its exact SeplosError
    Given a crafted Seplos frame with defect <defect>
    When the input is parsed as a frame
    Then the parse is rejected with error <variant>

    Examples:
      | defect              | variant           |
      | bad_soi             | BadSoi            |
      | non_hex             | NonHex            |
      | bad_length_checksum | BadLengthChecksum |
      | length_mismatch     | LengthMismatch    |
      | rtn_error           | RtnError          |
      | too_many_elements   | TooManyElements   |

  Scenario: A well-formed frame whose telemetry INFO is too short fails with InfoUnderrun
    Given a Seplos telemetry frame with only 2 INFO bytes
    When the telemetry frame is decoded
    Then the decode is rejected with error InfoUnderrun

  Scenario: Building a command for an out-of-range DIP fails with AddrOutOfRange
    When a Seplos command is built for DIP 16
    Then the build is rejected with error AddrOutOfRange

  Scenario Outline: The SeplosSim fault-injection behaviors resolve to their expected outcome
    Given the 14-cell sample battery under test
    When the battery replies with behavior <behavior> and the reply is parsed
    Then the reply resolves to outcome <outcome>

    Examples:
      | behavior        | outcome         |
      | CorruptChecksum | BadChecksum     |
      | WrongAddress    | AddressMismatch |
      | Truncated       | Truncated       |
      | Silence         | Silence         |
      | Garbage         | BadSoi          |
