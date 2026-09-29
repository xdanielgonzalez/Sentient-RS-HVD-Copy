# MQTT 3.1.1 (OASIS, <https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/>) wire-format and connection-flow
# spec-compliance for sentient-rs's owned client: CONNECT/CONNACK section 3.1/3.2, PUBLISH section
# 3.3, PINGREQ/PINGRESP section 3.13/3.14, Remaining Length encoding section 2.2.3. NOT
# chapter-d.txt / MIL-STD-3071 (this file has no TMS-IDL content) — that's mapping/tms's territory.
#
# Implementation under test: lib-link/src/mqtt_client.rs (packet encode/parse — pure, no
# transport) and lib-link/src/mqtt_session.rs (the CONNECT->CONNACK->PUBLISH ordering +
# keep-alive state machine). Both are exercised purely in-process (byte-in, byte-out / state-in,
# state-out) — no broker, no socket. The broker-delivery loopback (TLS + a real "broker" TCP peer)
# is test-e2e/tests/owned_mqtt.rs's `loopback` module, out of this suite's scope by design.
#
# `@provisional` scenarios assert real, current adapter behavior that is NOT yet spec-complete —
# they run and stay GREEN; the tag flags the gap rather than hiding it (same convention as
# provisional.feature, scoped to this file's own known gaps instead of moved there).

Feature: MQTT 3.1.1 wire encoding, CONNACK parsing, and session connection-flow

  # ---- CONNECT — MQTT 3.1.1 section 3.1 --------------------------------------------------------

  Scenario Outline: CONNECT encodes byte-exact for client id, keep-alive, and clean session
    Given a MQTT CONNECT with client id "<client_id>", keep-alive <keep_alive> s, clean session <clean_session>
    When the MQTT CONNECT is encoded
    Then the MQTT encoded bytes equal hex "<hex>"

    Examples:
      | client_id | keep_alive | clean_session | hex                                                              |
      | dev01     | 60         | true           | 10 11 00 04 4D 51 54 54 04 02 00 3C 00 05 64 65 76 30 31         |

  Scenario: CONNECT sets the username and password Connect Flags bits (section 3.1.2.3)
    Given a MQTT CONNECT with client id "dev01", keep-alive 60 s, clean session false
    And the MQTT CONNECT has username "user" and password "pass"
    When the MQTT CONNECT is encoded
    Then the MQTT CONNECT flags byte equals hex "C0"

  @provisional @no-subscribe-disconnect-will
  Scenario: The owned client has no SUBSCRIBE, DISCONNECT, or Will support yet
    # PROVISIONAL: mqtt_client.rs implements only CONNECT/CONNACK/PUBLISH(QoS0)/PINGREQ — there is no
    # encode_subscribe, no encode_disconnect, and `ConnectOptions` carries no Will (Last Will and
    # Testament, MQTT 3.1.1 section 3.1.2.5) fields. This pins the plain CONNECT flags byte to show
    # the Will Flag/Will QoS/Will Retain bits (0x04/0x18/0x20) are structurally never set — there is
    # no code path that could set them.
    Given a MQTT CONNECT with client id "dev01", keep-alive 60 s, clean session true
    When the MQTT CONNECT is encoded
    Then the MQTT CONNECT flags byte equals hex "02"

  # ---- PUBLISH — MQTT 3.1.1 section 3.3 ---------------------------------------------------------

  Scenario Outline: PUBLISH (QoS 0) encodes byte-exact for topic and payload
    Given a MQTT PUBLISH to topic "<topic>" with payload "<payload>"
    When the MQTT PUBLISH is encoded
    Then the MQTT encoded bytes equal hex "<hex>"

    Examples:
      | topic | payload | hex                                        |
      | t/1   | hi      | 30 07 00 03 74 2F 31 68 69                |

  Scenario: PUBLISH with QoS greater than 2 is rejected as Malformed
    Given a MQTT PUBLISH to topic "t/1" with payload "hi"
    And the MQTT PUBLISH has QoS 3
    When the MQTT PUBLISH is encoded
    Then the MQTT encode error is Malformed

  Scenario: PUBLISH at QoS 1 or 2 without a packet id is rejected as MissingPacketId
    Given a MQTT PUBLISH to topic "t/1" with payload "hi"
    And the MQTT PUBLISH has QoS 1
    When the MQTT PUBLISH is encoded
    Then the MQTT encode error is MissingPacketId

  Scenario: PUBLISH with a topic exceeding the 16-bit wire length limit is rejected as FieldTooLong
    Given a MQTT PUBLISH to topic "t/1" with payload "hi"
    And the MQTT PUBLISH topic is 65536 bytes long
    When the MQTT PUBLISH is encoded
    Then the MQTT encode error is FieldTooLong

  Scenario: PUBLISH into an undersized output buffer fails closed as BufferTooSmall
    Given a MQTT PUBLISH to topic "t/1" with payload "hi"
    And the MQTT PUBLISH output buffer is 3 bytes
    When the MQTT PUBLISH is encoded
    Then the MQTT encode error is BufferTooSmall

  # ---- PINGREQ — MQTT 3.1.1 section 3.13 --------------------------------------------------------

  Scenario: PINGREQ is always the fixed two-byte packet
    When the MQTT PINGREQ is encoded
    Then the MQTT encoded bytes equal hex "C0 00"

  # ---- CONNACK — MQTT 3.1.1 section 3.2 ---------------------------------------------------------

  Scenario Outline: CONNACK parses the session-present flag and return code
    Given MQTT CONNACK bytes "<hex>"
    When the MQTT CONNACK is parsed
    Then the MQTT CONNACK session present is <session_present>
    And the MQTT CONNACK return code is <return_code>

    Examples:
      | hex                  | session_present | return_code |
      | 20 02 00 00           | false            | 0           |
      | 20 02 01 00           | true             | 0           |

  Scenario Outline: CONNACK rejects reserved Connect Acknowledge Flags bits and malformed length
    Given MQTT CONNACK bytes "<hex>"
    When the MQTT CONNACK is parsed
    Then the MQTT encode error is Malformed

    Examples:
      | hex                       |
      | 20 02 02 00                |
      | 20 02                      |
      | 20 02 00 00 00              |

  # ---- MqttSession — connection-flow ordering + keep-alive --------------------------------------

  Scenario: The first poll emits CONNECT and the session is not yet connected
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    When the MQTT session is polled at 0 ms with no incoming bytes
    Then the MQTT session output equals hex "10 11 00 04 4D 51 54 54 04 02 00 3C 00 05 64 65 76 30 31"
    And the MQTT session is not connected

  Scenario: Publishing before CONNACK is rejected as NotConnected
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    When the MQTT session publish is attempted with topic "t/1" and payload "hi"
    Then the MQTT session error is NotConnected

  Scenario: CONNACK return code 0 transitions the session to Connected
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    And the MQTT session has been polled at 0 ms with no incoming bytes
    When the MQTT session is polled at 10 ms with incoming hex "20 02 00 00"
    Then the MQTT session is connected

  Scenario: CONNACK return code 5 refuses the connection and kills the session
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    And the MQTT session has been polled at 0 ms with no incoming bytes
    When the MQTT session is polled at 10 ms with incoming hex "20 02 00 05"
    Then the MQTT session error is Refused
    And the MQTT session is not connected

  Scenario: An idle link past the keep-alive interval emits a PINGREQ
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    And the MQTT session has been polled at 0 ms with no incoming bytes
    And the MQTT session has been polled at 10 ms with incoming hex "20 02 00 00"
    When the MQTT session is polled at 70000 ms with no incoming bytes
    Then the MQTT session output equals hex "C0 00"

  @provisional @qos-not-implemented
  Scenario: A PUBLISH from the session is always QoS 0 (no PUBACK/QoS1/2 delivery path yet)
    # PROVISIONAL: MqttSession::publish (mqtt_session.rs) hardcodes QoS 0 with no packet id — there
    # is no PUBACK/PUBREC/PUBREL/PUBCOMP handling in the session, so QoS 1/2 end-to-end delivery is
    # not implemented even though mqtt_client::encode_publish supports the QoS 1/2 wire format. The
    # fixed-header byte 0x30 below (QoS bits = 00) is the observable proof.
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    And the MQTT session has been polled at 0 ms with no incoming bytes
    And the MQTT session has been polled at 10 ms with incoming hex "20 02 00 00"
    When the MQTT session publish is attempted with topic "t/1" and payload "hi"
    And the MQTT session is polled at 20 ms with no incoming bytes
    Then the MQTT session output equals hex "30 07 00 03 74 2F 31 68 69"

  @provisional @pingresp-not-validated
  Scenario: A PINGRESP is consumed by byte match only, not validated against an outstanding PINGREQ
    # PROVISIONAL: consume_inbound (mqtt_session.rs) drops any exact `D0 00` prefix while Connected
    # without tracking whether a PINGREQ is actually outstanding, and any other unrecognised bytes
    # are silently left in `rx` rather than surfacing a session error — there is no protocol-level
    # validation of PINGRESP, just a byte-pattern match.
    Given a MQTT session for client id "dev01" with keep-alive 60 s
    And the MQTT session has been polled at 0 ms with no incoming bytes
    And the MQTT session has been polled at 10 ms with incoming hex "20 02 00 00"
    When the MQTT session is polled at 20 ms with incoming hex "D0 00"
    Then the MQTT session is connected

  # ---- lib_mapping::sink publish-contract bounds --------------------------------------------

  Scenario: topic_into builds the {prefix}/{device_id}/{snake_case slug} topic
    Given a MQTT topic device id "dev"
    When the MQTT topic is built with prefix "tms" and tms topic "DcMeasurementUpdate"
    Then the MQTT built topic equals "tms/dev/dc_measurement_update"

  Scenario: A topic exceeding MAX_TOPIC (128 bytes) fails to build as TopicTooLong
    Given a MQTT topic device id 130 bytes long
    When the MQTT topic is built with prefix "tms" and tms topic "DcMeasurementUpdate"
    Then the MQTT topic build error is TopicTooLong

  Scenario: A TMS payload exceeding MAX_PAYLOAD (2048 bytes) fails to serialize as Serialize
    When a maximally-sized ActiveDiagnosticState TMS payload is published via lib_mapping::sink
    Then the MQTT publish error is Serialize
