# Meshtastic mesh-radio secondary publish target — see
# docs/gap-analysis-meteor-parity.md's "Meshtastic mesh-radio forwarding" finding.
#
# Exercises the real, reused lib_mesh::condense greedy priority-ordered JSON condensation directly
# (pure, in-process) — see tests/steps/mesh.rs's module doc comment for what this suite covers
# (field priority / budget behavior) and does NOT cover (radio serial I/O, the hand-rolled
# Meshtastic protobuf + stream framing — byte-level golden tests for those live in
# crates/lib-mesh/src/impl_frame.rs, which needs no broker/serial harness).
#
# HONESTY TAGGING (docs/tms-compliance.md §7) — the whole feature is @provisional: no reference
# `meteor-adapter` condense_reading implementation was available in this environment (CLAUDE.md's
# reference-prototype policy; /home/user/meteor-adapter was absent), so the field set, priority
# order, and JSON key names are a best-effort derivation from the gap-analysis finding's own
# suggested priority (SOC, pack V, current, min/max cell, temps, faults, location) — see
# crates/lib-net/src/mesh/impl_condense.rs's module doc comment for the one remaining deliberate
# deviation this forces (raw fixed-point source units, not floating-point engineering units).
# #387 closed the other deviation this note used to list: `condense` now takes an optional
# `CanonicalFault` (`CondenseInput::fault`), encoded as a compact "f" bitmask field, prioritized
# above location but below the reading's own core fields — see the two scenarios below tagged with
# "fault" in their name. Run everything except this file with:
#   cargo test -p test-acceptance --test cucumber -- --tags "not @provisional"

@provisional
Feature: Meshtastic mesh-radio payload condensation

  Background:
    Given a typical battery reading

  Scenario: A generous budget includes every priority field in source-unit order
    When the reading is condensed for mesh transmission with a budget of 200 bytes
    Then the condensed payload is valid JSON at most 200 bytes
    And the condensed payload keys are, in order, "soc,v,i,cmin,cmax,t"

  Scenario: A location fix adds lat/lon as the lowest-priority fields
    Given a location fix at lat 37.77493 lon -122.41942
    When the reading is condensed for mesh transmission with a budget of 200 bytes
    Then the condensed payload is valid JSON at most 200 bytes
    And the condensed payload keys are, in order, "soc,v,i,cmin,cmax,t,lat,lon"

  Scenario: A tight budget keeps only the state of charge, the highest-priority field
    When the reading is condensed for mesh transmission with a budget of 12 bytes
    Then the condensed payload is valid JSON at most 12 bytes
    And the condensed payload keys are, in order, "soc"
    And the condensed payload contains soc 775

  Scenario: A budget too small for any field yields the empty object
    When the reading is condensed for mesh transmission with a budget of 2 bytes
    Then the condensed payload is valid JSON at most 2 bytes
    And the condensed payload keys are, in order, ""

  Scenario: A budget below the smallest possible JSON object writes nothing at all
    When the reading is condensed for mesh transmission with a budget of 1 byte
    Then the condensed payload is empty

  Scenario: A reading with no cells or temperature sensors omits those fields
    Given a battery reading with no cell or temperature data
    When the reading is condensed for mesh transmission with a budget of 200 bytes
    Then the condensed payload keys are, in order, "soc,v,i"

  # #387 — mesh fault forwarding. The mesh link is the out-of-band channel that survives a network
  # outage, exactly when a pack fault must get out; these two scenarios are the acceptance-level
  # coverage for that behavior (see crates/lib-net/src/mesh/impl_condense.rs's `fault_bitmask` unit
  # tests for the bit-order coverage this suite deliberately does not re-derive).

  Scenario: An active fault is forwarded as a compact bitmask field
    Given an active high-voltage protection fault
    When the reading is condensed for mesh transmission with a budget of 200 bytes
    Then the condensed payload is valid JSON at most 200 bytes
    And the condensed payload keys are, in order, "soc,v,i,cmin,cmax,t,f"
    And the condensed payload contains fault bitmask 1

  Scenario: A fault outranks location — a tight budget drops the location, not the fault
    Given a location fix at lat 37.77493 lon -122.41942
    And an active high-voltage protection fault
    When the reading is condensed for mesh transmission with a budget of 75 bytes
    Then the condensed payload is valid JSON at most 75 bytes
    And the condensed payload keys are, in order, "soc,v,i,cmin,cmax,t,f"
    And the condensed payload contains fault bitmask 1

  Scenario Outline: The condensed payload never exceeds the requested budget, at any budget size
    When the reading is condensed for mesh transmission with a budget of <budget> bytes
    Then the condensed payload is valid JSON at most <budget> bytes

    Examples:
      | budget |
      | 20     |
      | 40     |
      | 75     |
      | 128    |
