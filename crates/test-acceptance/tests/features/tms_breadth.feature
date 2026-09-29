# TMS model-breadth (Heartbeat, DeviceInfo family, StorageUpdate, Request/Reply) — see
# docs/gap-analysis-meteor-parity.md's "TMS data models" section.
#
# Pure in-process, same convention as tms_spec_compliance.feature: constructed structs +
# serde_json only, no broker/network. The actual Request/Reply round-trip-under-the-0.1s-budget
# behavior against a real embedded broker is covered by
# test-e2e/tests/request_reply_e2e.rs (bin-rpi ships no library target for this suite to import
# `tasks::reply_listener` directly — see that test file's doc comment).

Feature: TMS model breadth beyond the three Seplos-derived topics

  # ---- Heartbeat (#240) --------------------------------------------------------------------------

  Scenario: Heartbeat carries the device id and sequence number
    Given a Heartbeat with sequence number 7
    Then the TMS breadth JSON field device_id equals "METEOR-001"
    And the TMS breadth JSON field sequence_number equals 7

  Scenario: Heartbeat sequence number wraps at u32::MAX rather than panicking
    Given a Heartbeat with sequence number 4294967295
    When the heartbeat sequence number is incremented with wrapping
    Then the TMS breadth JSON field sequence_number equals 0

  # ---- DeviceInfo family (#244) — 4-field top-level shape (device_id/role/product/topics) --------

  Scenario: DeviceInfo announces the STOR role and product info
    Given a DeviceInfo built from the STOR device config
    Then the TMS breadth JSON field role equals "ROLE_STORAGE"
    And the TMS breadth JSON field product.manufacturer_name equals "HVD"
    And the TMS breadth JSON field product.model_name equals "METEOR-STOR"

  Scenario: DeviceInfo TopicInfo declares the data model version (2.0.2)
    Given a DeviceInfo built from the STOR device config
    Then the TMS breadth JSON field topics.data_model_version equals "2.0.2"

  Scenario: DeviceInfo honestly declares no supported request topics
    Given a DeviceInfo built from the STOR device config
    Then the TMS breadth JSON array topics.supported_request_topics is empty

  # ---- StorageUpdate (#240) — reshaped to the 2.0.2 IDL (no cell list; far smaller than the old
  # EnergyStorageState-carrying shape) --------------------------------------------------------------

  Scenario: A StorageUpdate mapped from the deployed 14-cell hardware's reading fits MAX_PAYLOAD
    Given a canonical reading shaped like the deployed 14-cell Seplos hardware
    When the reading is mapped to a StorageUpdate
    Then the StorageUpdate JSON-encodes within the 2048-byte MAX_PAYLOAD budget

  # ---- Request/Reply control path (#246) — revised D6: well-formed -> REPLY_NOT_IMPLEMENTED,
  # undecodable -> REPLY_BAD_REQUEST (this adapter implements no command surface) -------------------

  Scenario: A Reply to a well-formed request echoes the requester id and sequence, NotImplemented
    Given a request from "MC-1" with sequence 42
    When a Reply is built answering the well-formed request with NotImplemented
    Then the TMS breadth JSON field requesting_device_id equals "MC-1"
    And the TMS breadth JSON field request_sequence_id equals 42
    And the TMS breadth JSON field status.code equals "REPLY_NOT_IMPLEMENTED"

  Scenario: A Reply to an undecodable request reports BadRequest, never silence
    When a Reply is built for an undecodable request
    Then the TMS breadth JSON field status.code equals "REPLY_BAD_REQUEST"

  # ---- Required STOR state publishers (#263) — EnergyStartStopState, PowerPortState,
  # DcLoadSharingState, ActiveMicrogridControllerState. Honest content for a telemetry-only battery
  # (PROVISIONAL where the device can't source a field — see tasks::state_publisher). --------------

  Scenario: EnergyStartStopState reports OPERATIONAL with a startup reason and no pending transition
    Given an EnergyStartStopState for a healthy STOR device
    Then the TMS breadth JSON field present_level equals "ESSL_OPERATIONAL"
    And the TMS breadth JSON field future_level equals "ESSL_OPERATIONAL"
    And the TMS breadth JSON field request_lock equals boolean false
    And the TMS breadth JSON field present_level_reason equals "PSR_STARTUP"
    And the TMS breadth JSON field future_level_reason equals "PSTC_NONE"
    And the TMS breadth JSON field present_level_actor is null
    And the TMS breadth JSON field remaining_time is null

  Scenario: PowerPortState declares one port with unknown cable status and no switch
    Given a PowerPortState for the single declared port
    Then the TMS breadth JSON array power_ports has length 1
    And the TMS breadth JSON field power_ports.0.cable_status equals "CS_UNKNOWN"
    And the TMS breadth JSON field power_ports.0.cable_id is null
    And the TMS breadth JSON field power_ports.0.switch_status is null

  Scenario: DcLoadSharingState is all-NaN-honest for a non-load-sharing battery
    Given a DcLoadSharingState for a non-load-sharing battery
    Then the TMS breadth JSON field config equals "CONFIG_UNKNOWN"
    And the TMS breadth JSON array parameters is empty

  Scenario: ActiveMicrogridControllerState carries the selected MC id
    Given an ActiveMicrogridControllerState with master "MC-1"
    Then the TMS breadth JSON field device_id equals "METEOR-001"
    And the TMS breadth JSON field master_id equals "MC-1"

  Scenario: ActiveMicrogridControllerState reports no master when no MC is active
    Given an ActiveMicrogridControllerState with no master selected
    Then the TMS breadth JSON field master_id is null

  # ---- Topic constants ------------------------------------------------------------------------------

  Scenario Outline: the new TMS breadth topic constants match their IDL string literals
    Then the TMS schema topic constant <name> equals "<expected>"

    Examples:
      | name                              | expected                       |
      | HEARTBEAT                         | Heartbeat                      |
      | DEVICE_INFO                       | DeviceInfo                     |
      | REPLY                             | Reply                          |
      | ENERGY_START_STOP_REQUEST         | EnergyStartStopRequest         |
      | POWER_SWITCH_REQUEST              | PowerSwitchRequest             |
      | ENERGY_START_STOP_STATE           | EnergyStartStopState           |
      | POWER_PORT_STATE                  | PowerPortState                 |
      | ACTIVE_MICROGRID_CONTROLLER_STATE | ActiveMicrogridControllerState |
      | DC_LOAD_SHARING_STATE             | DcLoadSharingState             |
