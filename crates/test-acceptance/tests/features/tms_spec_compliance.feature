# TMS (MIL-STD-3071) schema STRUCTURAL spec-compliance — field shape, topic literals, enum
# discriminants, and MAXLEN bounds vs the IDL, as opposed to `transform_conventions.feature` /
# `dc_measurement.feature` / `power_hardware.feature` / `active_diagnostic.feature`, which assert
# mapped VALUES (scaling, sign, round-trip) via `test-kit`/`lib-parse` decode. Every
# step here namespaces its phrasing with "the TMS schema ..." to avoid ambiguous-match collisions
# with `steps/mapping.rs`; every step delegates to the real `lib_tms` types via
# `serde_json` — no protocol/transform logic is re-implemented, and enum discriminants are always
# read off the real enum with `as u8`, never hand-copied as a literal.
#
# Oracle: docs/compliance/tms/mil-std-3071_data_model.idl (MIL-STD-3071 TMS Data Model,
# TMS_VERSION "2.0.2"). IDL line numbers below are that file:
#   * DtcSeverity enum 498-510 (IDL declaration order == Rust declaration order, 0-10, SEV_* wire
#     tokens).
#   * FailureModeIndicator enum 869-894 (FMI_* wire tokens; the IDL assigns no @value, so the Rust
#     discriminants are declaration-order ordinals 0..23, Unknown first — the J1939 numbers are
#     comments only).
#   * MAX_PORTS = 64 at 903; MAXLEN_storageCells = 50 at 1487 (both confirmed — Rust caps 4 and 50
#     respectively are deliberate narrowings/exact matches).
#   * EnergyStorageState 1555-1598, StorageCellState 1496-1536 (@nested), PowerHardwareUpdate
#     2359-2407, DiagnosticTroubleCode 2416-2465, ActiveDiagnosticState, dc::MeasurementUpdate.
#
# @dds-out-of-scope: per tms-compliance.md §6, the standard's data model assumes DDS-XTypes wire
# serialization. As of story #321 (WP1-WP7, #325) this adapter DOES carry a real DDS/RTPS transport
# beside MQTT — RTPS/UDP over IPv6, domain 0, XCDR2 — verified against a real CycloneDDS 11.0.1
# participant (`rtps_spec.feature`'s hex-table scenarios + `test-e2e/tests/dds_interop_e2e.rs`'s
# live-wire e2e + `lib-mapping/tests/wire_cdr_*.rs`'s byte-oracle chain covering the FULL 136-type
# model, goldens included). This file's own scope is unchanged and still legitimate: it asserts JSON
# schema-shape structural conformance to the IDL for this adapter's MQTT/JSON transport (serde_json
# here; serde-json-core in production) — no scenario here asserts DDS/XTypes/CDR/QoS conformance,
# because that assertion now lives in `rtps_spec.feature`/`dds_interop_e2e.rs`, not because it is
# out of scope for the product. The remaining gap, tracked honestly rather than glossed over: DDS
# LIVE writer coverage is PARTIAL — only `Heartbeat`, `DcMeasurementUpdate`, `DcLoadSharingState`,
# and the `Reply` response leg have a dust_dds patch type; the other 7 publish topics + the 2 DDS
# request readers are blocked on a `dust_dds_derive` `@optional` gap (issue #332). Full-model XCDR2
# wire validity (including the 7 blocked topics) is still proven — by the static byte-oracle chain,
# not yet by a live DDS writer — until #332 lands.
#
# @unverified-maxlen: reserved for MAX_DC_LINES=3 (Rust `PowerPortMeasurement.line: Vec<_,3>`,
# lib-tms/src/lib.rs) — the nested `dc::PowerLineMeasurement`/`dc::PowerPortMeasurement`
# IDL bodies were never separately expanded from chapter-d.txt (tms-compliance.md §7d), so this one
# bound remains a documented assumption, not a confirmed IDL match. MAX_PORTS=4≤64 and
# MAX_STORAGE_CELLS=50 are both confirmed and therefore untagged.

@dds-out-of-scope
Feature: TMS schema structural conformance to the MIL-STD-3071 IDL

  # ---- Field presence/count vs IDL (tms-compliance.md §3) ---------------------------------------

  Scenario: DcMeasurementUpdate has exactly the 4 IDL fields (deviceId/timeMeasured/external/internal)
    Given the TMS schema fully-populated DcMeasurementUpdate
    Then the TMS schema JSON has 4 top-level fields
    And the TMS schema JSON top-level field device_id is present
    And the TMS schema JSON top-level field time_measured is present
    And the TMS schema JSON top-level field external_measurement is present
    And the TMS schema JSON top-level field internal_measurement is present

  Scenario: Nested dc::PowerPortMeasurement has exactly its 3 IDL fields
    Given the TMS schema fully-populated PowerPortMeasurement
    Then the TMS schema JSON has 3 top-level fields
    And the TMS schema JSON top-level field port_number is present
    And the TMS schema JSON top-level field line is present
    And the TMS schema JSON top-level field power_rate_of_change is present

  Scenario: Nested dc::PowerLineMeasurement has exactly its 3 IDL fields (voltage/amperage/power)
    Given the TMS schema fully-populated PowerLineMeasurement
    Then the TMS schema JSON has 3 top-level fields
    And the TMS schema JSON top-level field voltage is present
    And the TMS schema JSON top-level field amperage is present
    And the TMS schema JSON top-level field power is present

  Scenario: PowerHardwareUpdate omits its absent @optional subsystems entirely
    # engine/fuel/generator/power_electronics/thermal are @optional IDL fields (IDL 2359-2406). Per
    # MIL-STD-3071 A.9.7.5 / Table A.13 an absent optional is "Empty (no contained value)", so the
    # generated types carry #[serde(skip_serializing_if, default)]: a battery-only device populates
    # only energy_storage and the other five subsystems are OMITTED from the JSON, not nulled.
    Given the TMS schema fully-populated PowerHardwareUpdate
    Then the TMS schema JSON has 3 top-level fields
    And the TMS schema JSON top-level field device_id is present
    And the TMS schema JSON top-level field timestamp is present
    And the TMS schema JSON top-level field energy_storage is present
    And the TMS schema JSON top-level field engine is absent
    And the TMS schema JSON top-level field fuel is absent
    And the TMS schema JSON top-level field generator is absent
    And the TMS schema JSON top-level field power_electronics is absent
    And the TMS schema JSON top-level field thermal is absent

  Scenario: Nested EnergyStorageState has exactly its 5 IDL fields
    Given the TMS schema fully-populated EnergyStorageState
    Then the TMS schema JSON has 5 top-level fields
    And the TMS schema JSON top-level field min_temperature is present
    And the TMS schema JSON top-level field average_temperature is present
    And the TMS schema JSON top-level field max_temperature is present
    And the TMS schema JSON top-level field run_time is present
    And the TMS schema JSON top-level field storage_cells is present

  Scenario: Nested StorageCellState has exactly its 5 IDL fields
    Given the TMS schema fully-populated StorageCellState
    Then the TMS schema JSON has 5 top-level fields
    And the TMS schema JSON top-level field cell_number is present
    And the TMS schema JSON top-level field cycle_counter is present
    And the TMS schema JSON top-level field state_of_charge is present
    And the TMS schema JSON top-level field temperature is present
    And the TMS schema JSON top-level field voltage is present

  Scenario: ActiveDiagnosticState has exactly its 3 IDL fields (deviceId/codes/overflow)
    Given the TMS schema fully-populated ActiveDiagnosticState
    Then the TMS schema JSON has 3 top-level fields
    And the TMS schema JSON top-level field device_id is present
    And the TMS schema JSON top-level field codes is present
    And the TMS schema JSON top-level field overflow is present

  Scenario: Nested DiagnosticTroubleCode has exactly its 9 IDL fields when fully populated
    Given the TMS schema fully-populated DiagnosticTroubleCode
    Then the TMS schema JSON has 9 top-level fields
    And the TMS schema JSON top-level field port_number is present
    And the TMS schema JSON top-level field suspect_parameter is present
    And the TMS schema JSON top-level field failure_mode is present
    And the TMS schema JSON top-level field occurrence_count is present
    And the TMS schema JSON top-level field time_raised is present
    And the TMS schema JSON top-level field severity is present
    And the TMS schema JSON top-level field hint is present
    And the TMS schema JSON top-level field estimate_time is present
    And the TMS schema JSON top-level field estimate_severity is present
    And the TMS schema JSON top-level field affected_identity_id is absent

  # ---- @optional IDL fields default to None, and None is OMITTED from the JSON (MIL-STD-3071
  # A.9.7.5 / Table A.13 "Empty (no contained value)"; the generated types carry
  # #[serde(skip_serializing_if, default)]) ------------------------------------------------------

  Scenario: DiagnosticTroubleCode's 3 @optional fields default to None and are omitted
    Given the TMS schema DiagnosticTroubleCode with all optional fields set to None
    Then the TMS schema JSON has 6 top-level fields
    And the TMS schema JSON top-level field port_number is absent
    And the TMS schema JSON top-level field estimate_time is absent
    And the TMS schema JSON top-level field estimate_severity is absent

  Scenario: PowerHardwareUpdate's subsystems (@optional) default to None and are omitted
    Given the TMS schema PowerHardwareUpdate with all optional fields set to None
    Then the TMS schema JSON has 2 top-level fields
    And the TMS schema JSON top-level field energy_storage is absent
    And the TMS schema JSON top-level field engine is absent
    And the TMS schema JSON top-level field thermal is absent

  # ---- NaN-for-"not available" (tms-compliance.md §4/§3.2.1): these two non-optional floats have
  # no Seplos source, so the mapper always writes f32::NAN — which, via plain serde_json (not the
  # no_std serde-json-core codec production uses), serializes as present-but-null, same as an
  # @optional None. Documented here rather than silently treated as a normal float. -------------

  Scenario: EnergyStorageState.runTime has no Seplos source and is present-but-NaN (serializes null)
    Given the TMS schema fully-populated EnergyStorageState
    Then the TMS schema JSON top-level field run_time is null

  Scenario: dc::PowerPortMeasurement.power_rate_of_change has no source and is present-but-NaN
    Given the TMS schema fully-populated PowerPortMeasurement
    Then the TMS schema JSON top-level field power_rate_of_change is null

  # ---- JSON field-name deviation (real finding): serialized keys are snake_case, NOT the IDL's
  # camelCase — no #[serde(rename)] anywhere in lib-tms. This is a documented STRUCTURAL /
  # shape-parity deviation, not a DDS/CDR wire-parity claim (see @dds-out-of-scope above). --------

  Scenario: Serialized JSON keys are snake_case (device_id), not the IDL's camelCase (deviceId)
    Given the TMS schema fully-populated DcMeasurementUpdate
    Then the TMS schema JSON top-level field device_id is present
    And the TMS schema JSON top-level field deviceId is absent

  # ---- Topic-string literals (tms-compliance.md §5; chapter-d.txt TOPIC_* decls) -----------------

  Scenario: The 3 emitted TOPIC_* constants match the IDL string literals exactly
    Then the TMS schema topic constant DC_MEASUREMENT_UPDATE equals "DcMeasurementUpdate"
    And the TMS schema topic constant POWER_HARDWARE_UPDATE equals "PowerHardwareUpdate"
    And the TMS schema topic constant ACTIVE_DIAGNOSTIC_STATE equals "ActiveDiagnosticState"

  Scenario: topic_into builds {prefix}/{device_id}/{snake_case slug} for each emitted topic
    Then the TMS schema topic_into prefix "tms" device "dev" topic DC_MEASUREMENT_UPDATE yields "tms/dev/dc_measurement_update"
    And the TMS schema topic_into prefix "tms" device "dev" topic POWER_HARDWARE_UPDATE yields "tms/dev/power_hardware_update"
    And the TMS schema topic_into prefix "tms" device "dev" topic ACTIVE_DIAGNOSTIC_STATE yields "tms/dev/active_diagnostic_state"

  # ---- Emitted-vs-total scope (tms-compliance.md §1, §5, §7d): the standard defines 35 @topic
  # structs. This adapter emitted exactly 3 at the time this suite was first written;
  # `topics::STORAGE_UPDATE` was a real, correctly-spelled 4th constant defined but never wired to
  # a `topic_into`/`publish_tms` call site. #240 (TMS breadth) wired it end to end — see
  # `tms_breadth.feature` for the current full emitted-topic set. -------------------------------

  Scenario: STORAGE_UPDATE is a defined topic constant this adapter now publishes (#240, #265)
    Then the TMS schema topic constant STORAGE_UPDATE equals "StorageUpdate"
    And the TMS schema STORAGE_UPDATE topic constant is published at the spec Periodic 10 s rate

  # ---- Enum ordering/discriminants, read off the real enum via `as u8` (never hand-copied) -------

  Scenario Outline: DtcSeverity discriminants follow IDL declaration order 0-10 (chapter-d.txt 2730-2843)
    Then the TMS schema DtcSeverity variant <variant> has discriminant <discriminant>

    Examples:
      | variant      | discriminant |
      | Unknown      | 0            |
      | Clear        | 1            |
      | Informative  | 2            |
      | Preventative | 3            |
      | Degraded     | 4            |
      | Warning      | 5            |
      | Minor        | 6            |
      | Major        | 7            |
      | Critical     | 8            |
      | Fatal        | 9            |
      | HumanSafety  | 10           |

  Scenario Outline: FailureModeIndicator discriminants are declaration-order ordinals 0..23 (IDL 869-894)
    # The 2.0.2 IDL assigns no explicit @value to FailureModeIndicator, so the discriminants are the
    # plain declaration-order ordinals (Unknown first at 0). The J1939 FMI numbers survive only as
    # comments in the Rust enum, NOT as the discriminant. The wire token is the FMI_* serde rename.
    Then the TMS schema FailureModeIndicator variant <variant> has discriminant <discriminant>

    Examples:
      | variant                     | discriminant |
      | Unknown                     | 0            |
      | AboveNormalMostSevere       | 1            |
      | BelowNormalMostSevere       | 2            |
      | DataErratic                 | 3            |
      | VoltageHigh                 | 4            |
      | VoltageLow                  | 5            |
      | CurrentLow                  | 6            |
      | CurrentHigh                 | 7            |
      | NotResponding               | 8            |
      | AbnormalFrequency           | 9            |
      | AbnormalUpdateRate          | 10           |
      | AbnormalRateOfChange        | 11           |
      | FailureCodeNotIdentifiable  | 12           |
      | BadIntelligentDevice        | 13           |
      | OutOfCalibration            | 14           |
      | SpecialInstructions         | 15           |
      | AboveNormalLeastSevere      | 16           |
      | AboveNormalModeratelySevere | 17           |
      | BelowNormalLeastSevere      | 18           |
      | BelowNormalModeratelySevere | 19           |
      | ReceivedDataError           | 20           |
      | DataDriftedHigh             | 21           |
      | DataDriftedLow              | 22           |
      | ConditionExists             | 23           |

  # ---- MAXLEN bounds: Vec::push past capacity fails (tms-compliance.md §3.2.1/§3.3/§7d) ----------

  @unverified-maxlen
  Scenario: codes: DiagnosticTroubleCodeSequence MAXLEN 64 (confirmed, lib-tms MAX_DTC)
    When 64 TMS schema DiagnosticTroubleCode entries fill the codes sequence and a 65th is pushed
    Then the TMS schema sequence push beyond MAXLEN fails

  Scenario: storage_cells: StorageCellStateSequence MAXLEN 50 (confirmed, chapter-d.txt:6052)
    When 50 TMS schema StorageCellState entries fill the storage_cells sequence and a 51st is pushed
    Then the TMS schema sequence push beyond MAXLEN fails

  @unverified-maxlen
  Scenario: line: PowerLineMeasurementSequence cap 3 (MAX_DC_LINES, doc-comment bound, unconfirmed IDL body)
    When 3 TMS schema PowerLineMeasurement entries fill a port's line sequence and a 4th is pushed
    Then the TMS schema sequence push beyond MAXLEN fails

  Scenario: external_measurement: Rust port cap 4 is a deliberate narrowing of the confirmed IDL MAX_PORTS=64
    When 4 TMS schema PowerPortMeasurement entries fill external_measurement and a 5th is pushed
    Then the TMS schema sequence push beyond MAXLEN fails
    And the TMS schema DcMeasurementUpdate external_measurement cap of 4 is within the IDL MAX_PORTS bound of 64
