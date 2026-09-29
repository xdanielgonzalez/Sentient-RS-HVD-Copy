# Multi-port serial (the N-port InterfaceManager-equivalent) — see
# docs/gap-analysis-meteor-parity.md's "Multi-port serial (InterfaceManager)" finding.
#
# Scope note: these scenarios exercise the REAL bin_rpi_platform::HostPlatform::build_port seam —
# the exact shape bin-rpi::runtime::supervise uses (N HostPlatforms sharing one embedded broker
# via BrokerPublisher handles minted off it), proving the platform/broker fan-out composes.
#
# TOML-level config normalization (the [[bus]] array-of-tables shape, legacy [serial]/[poll]
# single-bus back-compat, and the "both [[bus]] and [serial] present is a config error" rule) is
# unit-tested directly in bin-rpi's own test suite instead of here:
# crates/bin/rpi/src/config/mod.rs's `buses_normalizes_legacy_single_serial_to_one_default_bus`,
# `buses_returns_configured_bus_array_entries_verbatim`,
# `buses_errors_when_both_bus_array_and_legacy_serial_present`, and
# `buses_errors_when_neither_bus_array_nor_legacy_serial_present` — bin-rpi is a binary-only crate
# (no `[lib]` target), so its `Config` type is not reachable from this crate's cucumber steps.

Feature: Multi-port serial bus supervision

  Scenario: A single bus behaves like the pre-multi-port shape
    Given a fresh embedded broker
    When 1 host platforms are built against it, one per bus
    Then each platform builds successfully
    And the broker is still running

  Scenario: Two buses share one broker and each gets an independent publisher
    Given a fresh embedded broker
    When 2 host platforms are built against it, one per bus
    Then each platform builds successfully
    And the broker is still running

  Scenario: Four buses still share exactly one broker
    Given a fresh embedded broker
    When 4 host platforms are built against it, one per bus
    Then each platform builds successfully
    And the broker is still running

  # D13 (decision 0007): a unit with both a battery (RS-485/Seplos) and an inverter (RS-232/
  # Voltronic) leg runs TWO TMS identities concurrently — METEOR-001 (ROLE_STORAGE, telemetry/
  # battery/…) and METEOR-001-inv (ROLE_CONVERSION, telemetry/inverter/…) — each its own bus/tap on
  # the one shared broker. The identity DERIVATION + duplicate-id rejection (Config::identities) is
  # unit-tested in bin-rpi (Config isn't reachable from cucumber — see the header note); this
  # scenario proves the two-identity platform/tap fan-out composes on one broker.
  Scenario: A battery identity and an inverter identity share one broker
    Given a fresh embedded broker
    When a battery bus and an inverter bus are built against it
    Then each platform builds successfully
    And the broker is still running
