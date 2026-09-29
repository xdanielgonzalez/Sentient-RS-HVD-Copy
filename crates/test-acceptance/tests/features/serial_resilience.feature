# Serial-link resilience — the device must be able to START and stay up when its RS-485 serial link
# is unavailable (unplugged/faulty/busy dongle, or a port it may not open). The mechanism is a
# "disconnected" serial transport: reads/writes time out benignly (IoError::Timeout) so the poll
# loop treats it as a silent bus and keeps retrying via reconnect(), instead of the agent exiting.
# The platform-level "starts degraded, broker stays up" behavior is covered by impl-app host tests
# and the QEMU e2e preflight; this pins the transport contract that makes it possible.

Feature: A disconnected serial transport degrades gracefully so the device can start without a serial link

  Scenario: Reading from a disconnected serial transport times out instead of failing hard
    Given a disconnected host serial transport
    When a read is attempted on the disconnected transport
    Then the read returns an IoError timeout without panicking

  Scenario: Writing to a disconnected serial transport times out instead of failing hard
    Given a disconnected host serial transport
    When a write is attempted on the disconnected transport
    Then the write returns an IoError timeout without panicking

  Scenario: Direction control on a disconnected transport is a benign no-op
    Given a disconnected host serial transport
    When the transport direction is set
    Then the direction change succeeds
