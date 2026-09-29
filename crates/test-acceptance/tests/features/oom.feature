# Bounded-memory / out-of-memory resilience — the whole stack is no_std + no_alloc, every buffer
# is a fixed-capacity heapless collection, so an over-large input must degrade GRACEFULLY (a typed
# error, never a panic, never an unbounded allocation). Exercises the real caps and codecs:
#   * decode caps MAX_CELLS=16 / MAX_TEMPS=8 -> SeplosError::TooManyElements (telemetry.rs);
#   * short INFO -> SeplosError::InfoUnderrun (telemetry.rs);
#   * a deliberately-short output slice -> CodecError::BufferTooSmall (codec/mod.rs, codec/tms.rs);
#   * transform bounded truncation into StorageCellStateSequence (MAX_STORAGE_CELLS=50) and the
#     PowerLineMeasurementSequence (MAX_DC_LINES=3).
# Documented in docs/tms-compliance.md §7.
#
# HEADROOM (documented honestly, NOT re-tested here): a true >64-DTC ActiveDiagnosticState.overflow
# is UNREACHABLE from Seplos's fixed 16-flag fault table (16 << MAX_DTC=64) — see
# tests/features/dtc_overflow.feature, which pins that headroom without fabricating a 17th flag.

Feature: Bounded-memory resilience degrades gracefully under over-cap and short-buffer inputs

  Scenario: A telemetry frame declaring more cells than the cap fails with TooManyElements
    Given a Seplos telemetry frame declaring 17 cells
    When the telemetry frame is decoded
    Then the decode is rejected with error TooManyElements

  Scenario: A telemetry frame declaring more temperature sensors than the cap fails gracefully
    Given a Seplos telemetry frame declaring 1 cell and 9 temperature sensors
    When the telemetry frame is decoded
    Then the decode is rejected with error TooManyElements

  Scenario: A telemetry frame whose INFO underruns the declared layout fails with InfoUnderrun
    Given a Seplos telemetry frame with only 3 INFO bytes
    When the telemetry frame is decoded
    Then the decode is rejected with error InfoUnderrun

  Scenario: Encoding an oversize diagnostics payload into a short JSON buffer fails with BufferTooSmall
    Given an all-faults diagnostics payload
    When the diagnostics are JSON-encoded into a 16-byte buffer
    Then the codec fails with error BufferTooSmall

Scenario Outline: The frame parser is total — no pathological input ever panics
    Given a pathological input named <kind>
    When the input is parsed as a frame
    Then the parser returns without panicking

    Examples:
      | kind          |
      | empty         |
      | soi_only      |
      | all_zeros     |
      | oversized     |
      | nonframe_text |

  Scenario: The storage-cell sequence stays within the confirmed IDL cap of 50
    # CONFIRMED (not provisional): MAX_STORAGE_CELLS=50 is an exact IDL match
    # (chapter-d.txt:6052, MAXLEN_storageCells=50) per tms-compliance.md §7d.
    Given a maximally populated 16-cell canonical reading
    When the reading is mapped to resilient power hardware
    Then the storage-cell count is within the 50-cell cap

  @provisional @unverified-maxlen
  Scenario: The DC line sequence stays within the (unconfirmed) cap of 3
    # PROVISIONAL: MAX_DC_LINES=3 still rests on the un-expanded dc::PowerLineMeasurement IDL body
    # (tms-compliance.md §3.1 / §7d) — the numeric MAXLEN was NOT located in chapter-d.txt, so the
    # cap of 3 is a documented assumption, not a verified match. (Contrast MAX_STORAGE_CELLS=50 and
    # MAX_PORTS=64, both now confirmed — §7d.)
    Given a maximally populated 16-cell canonical reading
    When the reading is mapped to a resilient DC measurement
    Then the DC line count is within the 3-line cap
