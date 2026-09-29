# RTPS/XCDR2 wire-encoding conformance (MIL-STD-3071 -> DDSI-RTPS 2.3 / DDS-XTypes 1.3), story #321
# WP7 (#325). Every scenario here renders a REAL `lib_tms`/`lib_mapping` XCDR2 chain
# (`lib_mapping::render_cdr::render_cdr_value`) and asserts on the ACTUAL emitted bytes — see
# `tests/steps/rtps.rs`'s module doc for the "no copied literals" wiring discipline (byte-search,
# differencing, and self-consistency formulas in place of hand-derived absolute offsets).
#
# Oracle: `crates/lib-tms/src/cdr/mod.rs`'s module doc — every numbered "Resolved XCDR2 verify-item"
# there carries the exact DDS-XTypes 1.3 (OMG formal/2020-02-04) section cite this file's scenarios
# below exercise. Two of those items were OPEN questions earlier in this round and are now SETTLED by
# empirical arbitration against a real CycloneDDS 11.0.1 participant (WP5/WP7a,
# `deploy/dds-probe/dump_reference.py` + `deploy/dds-conformance/`):
#   - optional-member presence encoding (item 3): CycloneDDS's own encoder/decoder matches this
#     module's spec-literal form byte-for-byte both directions — NO code change, confirmed.
#   - sequence-of-non-primitive-elements NEXTINT/DHEADER (item 7, "rule (12)"): CycloneDDS's decoder
#     REJECTS a sequence missing it (live `DcMeasurementUpdate` decode failure, WP7a) — the delimiter
#     is RE-ADOPTED in the chains and the vendored dust_dds patch alike (`vendor/dust_dds/PATCHES.md`).
# Every scenario below is therefore written as CONFIRMED (no `@provisional` tag) — this is a
# genuinely-settled round, not an aspiration.
Feature: RTPS/XCDR2 wire-encoding conformance

  # ---- Encapsulation identifier (DDS-XTypes 1.3 SS7.6.3.1.2 Table 60) ---------------------------

  Scenario: Every rendered XCDR2 payload begins with the D_CDR2_LE encapsulation identifier
    # An APPENDABLE type, encoding version 2, little-endian, uses D_CDR2_LE = {0x00, 0x09} on the
    # RTPS wire (every TMS IDL type is APPENDABLE — lib_tms::cdr module doc item 2), followed by a
    # 2-byte options field whose HIGH byte is always zero (only the low byte's low 2 bits are used,
    # for the trailing-pad count set at finalization).
    Given the rendered XCDR2 payload for Heartbeat device "RTPS-01" sequence 3
    Then bytes 0-1 of the rendered payload equal hex "00 09"
    And byte 2 of the rendered payload equals hex "00"

  Scenario: DcLoadSharingState's rendered payload also begins with D_CDR2_LE
    Given the rendered XCDR2 payload for DcLoadSharingState device "RTPS-01" config Active
    Then bytes 0-1 of the rendered payload equal hex "00 09"
    And byte 2 of the rendered payload equals hex "00"

  # ---- DHEADER (DDS-XTypes 1.3 SS7.4.3.4.1 + rule (30) SS7.4.3.5.3) -------------------------------

  Scenario: The top-level DHEADER equals the byte length of the members that follow, plus trailing pad
    # DHEADER(O) = O.ssize, the byte length of the serialized members that follow, NOT including the
    # DHEADER itself; the whole payload is then padded to a 4-byte boundary, with the pad count
    # recorded in the encapsulation options' low 2 bits (lib_tms::cdr module doc item 1).
    Given the rendered XCDR2 payload for Heartbeat device "RTPS-01" sequence 3
    Then the DHEADER of the rendered payload self-consistently frames its members and trailing pad

  Scenario: DcMeasurementUpdate's DHEADER is also self-consistent
    Given the rendered XCDR2 payload for DcMeasurementUpdate device "RTPS-01" with one internal port and no external ports
    Then the DHEADER of the rendered payload self-consistently frames its members and trailing pad

  # ---- Optional members in APPENDABLE (DDS-XTypes 1.3 SS7.4.3.5.2 + rule (20) SS7.4.3.5.3) --------

  Scenario: An @optional member's <is_present> boolean is emitted for both present and absent values
    # WP5/WP7a CycloneDDS arbitration CONFIRMED (no flip): this module's spec-literal form (always
    # emit the boolean, DDS-XTypes 1.3 rule (20)) matches CycloneDDS's own encoder/decoder both
    # directions (lib_tms::cdr module doc item 3). `ActiveMicrogridControllerState.master_id` is the
    # minimal real @optional field this round arbitrated against.
    Given the rendered XCDR2 payloads for ActiveMicrogridControllerState device "RTPS-01" master present and absent
    Then the byte where the present/absent payloads first diverge is 0x01 in the present payload and 0x00 in the absent payload

  # ---- Alignment: XCDR2 max alignment is 4, not 8 (DDS-XTypes 1.3 SS7.4.2/SS7.4.3.2) ---------------

  Scenario: A u64 field (EnergyStartStopRequest.sequence_id) aligns to 4 bytes, never to 8
    # "INT64, UINT64, FLOAT64 ... aligned to 4 rather than 8" — DDS-XTypes 1.3 SS7.4.2, quoted
    # verbatim in lib_tms::cdr::types::cdr_sink::CdrSink::u64's doc comment (lib_tms::cdr module doc
    # item 4). Probed across a family of preceding-field lengths so at least one naturally lands on
    # an offset that is 4-aligned but NOT 8-aligned — the only way that could happen is if the real
    # alignment cap is 4.
    When EnergyStartStopRequest's sequence_id (u64) is probed across a family of device_id lengths
    Then EnergyStartStopRequest's sequence_id aligns to 4 bytes but not always to 8

  # ---- Enums (DDS-XTypes 1.3 SS7.4.3.5.3 rule (5) + SS7.3.1.2.1.5 Table 32) -----------------------

  Scenario: An enum serializes as a fixed-width 4-byte holder regardless of variant
    # The default @bit_bound for an enum is 32 ("32 bits is the default size, and corresponds to all
    # enumerated types prior to this specification" — DDS-XTypes 1.3 SS7.3.1.2.1.5, lib_tms::cdr
    # module doc item 5). Two DcLoadSharingState renders differing ONLY in `config` must therefore be
    # the SAME total length, differ in exactly one contiguous 4-byte run, and that run's little-endian
    # value must equal the real `ConfigId` ordinal (read via `as u32`, never hand-copied).
    Then the rendered payload for DcLoadSharingState config Unknown is byte-identical to config Active except for one 4-byte enum run

  # ---- Strings: UInt32 length (including NUL) + NUL terminator (DDS-XTypes 1.3 rule (3)) ----------

  Scenario: A string field is a UInt32 length-including-NUL prefix, the body, then a trailing NUL
    Given the rendered XCDR2 payload for Heartbeat device "ZQXJ7AB" sequence 0
    Then the string "ZQXJ7AB" in the rendered payload is length-prefixed and NUL-terminated

  # ---- Sequences of non-primitive elements: rule (12) NEXTINT (DDS-XTypes 1.3 SS7.4.3.5.3) ---------

  Scenario: A sequence of non-primitive (struct) elements is prefixed by a rule-(12) NEXTINT delimiter
    # A sequence of NON-primitive elements (struct/string/enum) is preceded by a NEXTINT/DHEADER
    # UInt32 giving the byte length of `{ length : UInt32 } << element*` (DDS-XTypes 1.3 SS7.4.3.5.3
    # rule (12) — lib_tms::cdr module doc item 7). This is the item WP7a settled with STOCK
    # CycloneDDS 11.0.1: its decoder REJECTS a non-primitive sequence missing this delimiter (a live
    # `DcMeasurementUpdate` decode failure was the decisive datum) — the delimiter is RE-ADOPTED here
    # and in the vendored+patched dust_dds oracle alike, so `chains == dust_dds(patched) == what
    # CycloneDDS accepts` once again.
    Given the rendered XCDR2 payload for DcMeasurementUpdate device "Q" with one internal port and no external ports
    Then the rendered payload's rule-(12) NEXTINT correctly delimits the internal-port sequence

  # ---- KeyHash (DDSI-RTPS 2.3 SS9.6.3.8) — honestly scoped to what is actually implemented ----------

  Scenario: The @key-field emit plumbing serializes Heartbeat's sole key field (device_id) in order
    # KeyHash_t (DDSI-RTPS 2.3 SS9.6.3.8) is a 16-byte value computed from the @key members serialized
    # in declaration order using BIG-ENDIAN CDR, zero-filled to 16 bytes (max key size <= 128 bits) or
    # MD5'd otherwise. Only the declaration-order emit PLUMBING is implemented today
    # (`lib_tms::cdr::emit_key` + each generated `<Topic>_cdr::emit_key`) — the big-endian sink and the
    # zero-fill/MD5 wrapper are explicitly DEFERRED (lib_tms::cdr module doc item 8: "only if the
    # winning DDS adapter needs an explicit KeyHash" — USER_DATA = deviceId carries instance identity
    # today, so it hasn't been needed). This scenario asserts what exists, not a fictional complete
    # KeyHash_t.
    Given the KeyHash plumbing bytes for Heartbeat device "RTPS-01"
    Then the KeyHash plumbing bytes equal the device_id string encoding of "RTPS-01"
