# A real 14-cell Seplos 48150-1101-10E telemetry capture (DIP 0) -> Canonical -> TMS.
# The LIVE_FRAME bytes are copied verbatim from lib-mapping/tests/tms_live_frame.rs (itself
# cross-checked against lib-parse/tests/seplos_vectors.rs::decodes_live_telemetry), so this
# suite binds to the SAME known-good bytes and known values as those Rust unit tests — both fail
# together if the decode/transform ever drifts.
#
# The second half of this file exercises frame-INTEGRITY negative paths via SeplosSim's injected
# bus faults (reusing lib_parse::seplos::checksum, never a re-derived checksum) — a corrupted
# checksum, a reply for the wrong DIP, a truncated frame, a silent bus, and non-frame garbage must
# all be rejected, never silently accepted as a valid telemetry reading.

Feature: A real Seplos telemetry capture decodes to known TMS values; malformed frames are rejected

  Scenario: LIVE_FRAME decodes to the known TMS field values (IDL 13882-13926 / 8035-8128)
    Given the real 14-cell LIVE_FRAME capture from a Seplos 48150-1101-10E (DIP 0)
    When the frame is decoded into a canonical reading
    And the reading is mapped to a DC measurement
    And the reading is mapped to power hardware
    Then the read should succeed
    And the DC line voltage is approximately 54.52 V
    And the DC line amperage is approximately 1.64 A
    And there are 14 storage cells
    And storage cell 0 voltage is approximately 3.894 V
    And storage cell 0 state of charge is approximately 0.775
    And storage cell 0 cycle counter is approximately 2.0
    And the minimum cell voltage in millivolts is 3893
    And the maximum cell voltage in millivolts is 3895

  Scenario: LIVE_FRAME round-trips through JSON back to the same canonical telemetry
    Given the real 14-cell LIVE_FRAME capture from a Seplos 48150-1101-10E (DIP 0)
    When the frame is decoded into a canonical reading
    And the reading is mapped to a DC measurement
    And the reading is mapped to power hardware
    And the dc and hw payloads round-trip through JSON
    Then the round-tripped reading matches the original telemetry


  Scenario: A corrupted checksum is never accepted as valid telemetry
    Given the 14-cell sample battery
    And the simulator behavior is corrupted checksum
    When device 0 polls for telemetry
    Then the read should fail

  Scenario: A reply for a different DIP is rejected for the polled device
    Given the 14-cell sample battery
    And the simulator behavior is a wrong address reply
    When device 0 polls for telemetry
    Then the read should fail

  Scenario: A truncated reply that never reaches the delimiter is rejected
    Given the 14-cell sample battery
    And the simulator behavior is truncated to 10 bytes
    When device 0 polls for telemetry
    Then the read should fail

  Scenario: A silent bus produces no bytes and is rejected, not treated as an all-clear reading
    Given the 14-cell sample battery
    And the simulator behavior is silence
    When device 0 polls for telemetry
    Then the read should fail

  Scenario: Non-frame garbage bytes are rejected, not mistaken for a valid frame
    Given the 14-cell sample battery
    And the simulator behavior is garbage bytes
    When device 0 polls for telemetry
    Then the read should fail
