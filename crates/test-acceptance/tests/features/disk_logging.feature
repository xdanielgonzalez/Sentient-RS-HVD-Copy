# Disk / JSON-lines telemetry logging (Scope A, #55/#56/#57) — see
# docs/gap-analysis-meteor-parity.md's "Disk / JSON-lines telemetry logging" finding, and
# docs/demo-disk-logging.md for the runbook these scenarios mirror.
#
# Every scenario drives the real `lib_io::persist::run_writer` task end-to-end against a real temp
# directory (never a re-implementation of its rotation/disk-full logic — see tests/steps/disk.rs),
# then inspects the resulting *.jsonl files directly. The filesystem itself is the state shared
# between a scenario's steps, so no extra in-memory scenario state is needed.
#
# The precise ENOSPC-specific recovery algorithm (delete oldest rotated segment, retry once, then
# degrade with a periodic re-probe) is unit-tested directly against a faulting `WriteBackend` fake
# in crates/lib-io/src/persist/impl_jsonl_writer.rs's own test module — this suite instead covers the
# three human-readable, black-box acceptance criteria for Scope A.

Feature: Disk / JSON-lines telemetry logging (Scope A)

  Scenario: Every published TMS payload appears as one JSONL line
    Given a fresh disk log scratch dir "envelope-shape"
    When 3 telemetry records are published to a disk log in scratch dir "envelope-shape" with max_file_bytes 1048576 and max_files 3
    Then scratch dir "envelope-shape" contains 3 JSONL lines
    And every JSONL line in scratch dir "envelope-shape" carries its published ts, bus, topic, and payload verbatim

  Scenario: The disk log is bounded via size-based rotation, deleting the oldest segment first
    Given a fresh disk log scratch dir "rotation"
    When 60 telemetry records are published to a disk log in scratch dir "rotation" with max_file_bytes 200 and max_files 2
    Then scratch dir "rotation" retains at most 2 rotated segments plus the active file
    And the oldest surviving segment in scratch dir "rotation" is not the very first one written

  Scenario: The disk log degrades without crashing when its storage location cannot be written
    Given a disk log directory "unwritable" that cannot be written to
    When 5 telemetry records are published to a disk log in scratch dir "unwritable" with max_file_bytes 1048576 and max_files 3
    Then the disk-logging pipeline completes without panicking or hanging
    And no JSONL file was created in scratch dir "unwritable"
