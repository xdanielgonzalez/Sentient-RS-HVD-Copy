*** Settings ***
Documentation    Full functional e2e (Task #156 / Story #159): Seplos BMS simulator -> bin-rpi
...    (the merged sysext app, booted as sentient-image + sentient-app-sysext under runqemu
...    qemuarm64) -> its embedded MQTT broker (rumqttd, via impl_app::host::HostPlatform /
...    lib_link::LocalBroker) -> correct TMS telemetry data, asserted by an external stock paho
...    MQTT client. This is the FIRST *.robot suite in distro/qemu/ — until this file existed,
...    uat.yml's "Run Robot e2e" step auto-skipped with a notice (see that step's own guard:
...    `ls distro/qemu/*.robot`).
...
...    Contract asserted (verified against the real source, not assumed — see each test's own
...    [Documentation] for file:line citations):
...    - Topics: tms/METEOR-001/dc_measurement_update, tms/METEOR-001/power_hardware_update,
...      tms/METEOR-001/active_diagnostic_state — built by lib_mapping::sink::topic_into as
...      `{prefix}/{device_id}/{snake_case(TmsTopic)}` (crates/lib-mapping/src/sink/mod.rs:54-70),
...      fed the shipped sentient.toml's prefix="tms"/device.id="METEOR-001"
...      (distro/meta-sentient/recipes-sentient/sentient-agent/files/sentient.toml) and the
...      TMS topic-name constants in crates/lib-schema/src/lib.rs:28-33.
...    - Payload schema/values: crates/lib-mapping/src/transform.rs's ToTms/ToDiagnostics impls,
...      driven by BatteryState::sample_14_cell() (crates/test-kit/src/seplos/impl_battery_state.rs)
...      — the deterministic dataset this suite feeds via seplos-tcp-sim.
...
...    Reused, not reinvented: lib/{MqttKeywords.py,SeplosSimKeywords.py,tms_validate.py} are
...    used verbatim (via e2e_common.resource). They came unchanged from the MCU e2e harness and
...    were relocated into distro/qemu/lib/ when that harness was retired with the Embassy pivot
...    (docs/decisions/0010-mcu-runtime-embassy.md); the pure TMS validators are the SAME file
...    distro/qemu/lib/tms_mqtt_check.py imports, so the schema has exactly one source of truth.
...    Only the QEMU launch mechanism (YoctoQemuKeywords, this directory) is new, because
...    `runqemu` replaces a bare qemu-system-arm invocation — see that library's module docstring.
Resource    e2e_common.resource
Library     Collections

Suite Setup       Standard Yocto E2e Setup    normal    sample14
Suite Teardown    Standard Yocto E2e Teardown

*** Test Cases ***
Device Broker Accepts A Stock MQTT Client And Publishes DC Measurement
    [Documentation]    Proves the embedded broker (impl_app::host::HostPlatform::build ->
    ...    LocalBroker::start, crates/impl-app/src/host/impl_host_platform.rs:13-14) accepts a
    ...    plain external paho CONNECT/SUBSCRIBE and that DcMeasurementUpdate is published to the
    ...    topic HostPlatform::emit_reading renders it to
    ...    (crates/impl-app/src/host/impl_host_platform.rs:82-99, topics::DC_MEASUREMENT_UPDATE).
    ${dc_msg}=    MqttKeywords.Wait For Message On Topic    ${TOPIC_DC}    ${FIRST_TIMEOUT}
    Should Not Be Empty    ${dc_msg}
    ${violations}=    Validate Dc Measurement Update    ${dc_msg}
    Should Be Empty    ${violations}    msg=DcMeasurementUpdate schema violations: ${violations}

DC Measurement Carries The Known Seplos Pack Voltage Current And Power
    [Documentation]    seplos-tcp-sim's default fixture is BatteryState::sample_14_cell()
    ...    (crates/test-kit/src/seplos/impl_battery_state.rs): 14 cells at 3800..3813 mV whose sum
    ...    DERIVES total_voltage_cv = 5329 (real-Seplos fidelity — the pack voltage IS the cell sum,
    ...    not an independent hardcoded field; the fixture previously hardcoded an inconsistent 5452),
    ...    current_ca=-164. crates/lib-mapping/src/transform.rs's power_line_measurement_from
    ...    converts: voltage = cv_to_v(5329) = 53.29 V; amperage = -ca_to_a(-164) = 1.64 A
    ...    (SIGN-INVERTED: vendor charge-positive -> TMS source-positive, per transform.rs's own
    ...    header comment); power = voltage * amperage = 87.3956 W. Keep these three in lockstep with
    ...    sample_14_cell() — if the fixture's cells change, re-derive here (sum(mv)/10 -> cv_to_v).
    ${dc_msg}=    MqttKeywords.Wait For Message On Topic    ${TOPIC_DC}    ${STD_TIMEOUT}
    ${voltage}=    MqttKeywords.Get Json Field    ${dc_msg}    external_measurement.0.line.0.voltage
    ${amperage}=    MqttKeywords.Get Json Field    ${dc_msg}    external_measurement.0.line.0.amperage
    ${power}=       MqttKeywords.Get Json Field    ${dc_msg}    external_measurement.0.line.0.power
    Should Be True    abs(${voltage} - 53.29) < 0.01     msg=voltage ${voltage} != 53.29 V
    Should Be True    abs(${amperage} - 1.64) < 0.01     msg=amperage ${amperage} != 1.64 A (sign-inversion check)
    Should Be True    abs(${power} - 87.3956) < 0.01     msg=power ${power} != 87.3956 W
    # Cross-check via tms_validate's own sign-inversion rule (check_sign_inversion,
    # tms_validate.py:313-361): sample_14_cell's current_ca=-164 is a DISCHARGE in vendor
    # convention, so the rule requires amperage > 0 on the rendered payload.
    ${sign_violations}=    Check Sign Inversion    ${dc_msg}    -164
    Should Be Empty    ${sign_violations}    msg=sign-inversion check failed: ${sign_violations}

Power Hardware Update Carries The Known Storage Cell And SoC Values
    [Documentation]    energy_storage_state_from (crates/lib-mapping/src/transform.rs:143-163)
    ...    maps sample_14_cell's 14 cell_voltages_mv (3800..3813, i.e. cell i = 3800+i mV) via
    ...    scale::mv_to_v (/1000) to 3.800..3.813 V, one StorageCellState per cell
    ...    (cell_number = index). soc_dpct=775 -> dpct_to_pu(775) = 0.775 for EVERY cell (Seplos
    ...    reports one pack-level SOC broadcast identically per transform.rs's own FromTms doc
    ...    comment, line 401-402). Temperatures: sample_14_cell's raw Kelvin*10 sensor 0 =
    ...    2731+250 = 2981 -> dk_to_c = (2981-2731)/10 = 25.0 C (nearest_temp assigns sensor 0 to
    ...    cell 0). This is the same PowerHardwareUpdate golden fixture tms_validate.py's
    ...    self-test valid_hw encodes (storage_cells[0]: voltage=3.894, though note sample_14_cell
    ...    here starts cell 0 at 3800 mV = 3.800 V, not tms_validate's illustrative 3.894 V
    ...    example — this test asserts against the REAL sample_14_cell values, not that example).
    ${hw_msg}=    MqttKeywords.Wait For Message On Topic    ${TOPIC_HW}    ${STD_TIMEOUT}
    Should Not Be Empty    ${hw_msg}
    ${violations}=    Validate Power Hardware Update    ${hw_msg}
    Should Be Empty    ${violations}    msg=PowerHardwareUpdate schema violations: ${violations}

    ${cells}=    MqttKeywords.Get Json Field    ${hw_msg}    energy_storage.storage_cells
    ${cell_count}=    Get Length    ${cells}
    Should Be Equal As Integers    ${cell_count}    14    msg=expected 14 storage cells (sample_14_cell)

    ${cell0}=    Get From List    ${cells}    0
    ${cell0_voltage}=    Get From Dictionary    ${cell0}    voltage
    ${cell0_soc}=        Get From Dictionary    ${cell0}    state_of_charge
    ${cell0_temp}=       Get From Dictionary    ${cell0}    temperature
    Should Be True    abs(${cell0_voltage} - 3.800) < 0.001    msg=cell0 voltage ${cell0_voltage} != 3.800 V
    Should Be True    abs(${cell0_soc} - 0.775) < 0.001        msg=cell0 SoC ${cell0_soc} != 0.775
    Should Be True    abs(${cell0_temp} - 25.0) < 0.05         msg=cell0 temperature ${cell0_temp} != 25.0 C

    ${cell13}=    Get From List    ${cells}    13
    ${cell13_voltage}=    Get From Dictionary    ${cell13}    voltage
    Should Be True    abs(${cell13_voltage} - 3.813) < 0.001    msg=cell13 voltage ${cell13_voltage} != 3.813 V (3800+13 mV)

First Cycle Diagnostics Published With Zero Overflow And No Faults
    [Documentation]    crates/impl-app/src/lib.rs's run_cycle computes
    ...    `fault_due = fault_every > 0 && cycle.is_multiple_of(fault_every)` (line 134) — cycle 0
    ...    is always a multiple, so ActiveDiagnosticState publishes on the FIRST cycle. sample_14_cell
    ...    sets `faults: FaultFlags::default()` (all-clear), so to_active_diagnostics
    ...    (crates/lib-mapping/src/transform.rs:320-357) emits an empty `codes` list and
    ...    `overflow=0` — matching the topic HostPlatform::emit_fault renders to
    ...    (topics::ACTIVE_DIAGNOSTIC_STATE, impl_host_platform.rs:129-137).
    ${diag_msg}=    MqttKeywords.Wait For Message On Topic    ${TOPIC_DIAG}    ${STD_TIMEOUT}
    ${violations}=    Validate Active Diagnostic State    ${diag_msg}
    Should Be Empty    ${violations}    msg=ActiveDiagnosticState schema violations: ${violations}
    ${overflow}=    MqttKeywords.Get Json Field    ${diag_msg}    overflow
    Should Be Equal As Integers    ${overflow}    0
    ${codes}=    MqttKeywords.Get Json Field    ${diag_msg}    codes
    ${code_count}=    Get Length    ${codes}
    Should Be Equal As Integers    ${code_count}    0    msg=sample_14_cell has no faults set; expected 0 DTCs

All Three TMS Payload Kinds Carry The Correct Device Id
    [Documentation]    Every TMS payload's device_id is RunConfig.device_id
    ...    (crates/bin/rpi/src/main.rs's run_config, line 46), sourced from the shipped
    ...    sentient.toml's [device] id = "METEOR-001". Confirms the topic path AND the payload
    ...    body agree — not just that something arrived under tms/METEOR-001/*.
    ${dc_msg}=      MqttKeywords.Latest Payload On Topic    ${TOPIC_DC}
    ${hw_msg}=       MqttKeywords.Latest Payload On Topic    ${TOPIC_HW}
    ${diag_msg}=    MqttKeywords.Latest Payload On Topic    ${TOPIC_DIAG}
    ${dc_id}=    MqttKeywords.Get Json Field       ${dc_msg}      device_id
    ${hw_id}=    MqttKeywords.Get Json Field       ${hw_msg}      device_id
    ${diag_id}=    MqttKeywords.Get Json Field    ${diag_msg}    device_id
    Should Be Equal As Strings    ${dc_id}      ${DEVICE_ID}
    Should Be Equal As Strings    ${hw_id}      ${DEVICE_ID}
    Should Be Equal As Strings    ${diag_id}    ${DEVICE_ID}

NaN Policy Fields Are Rendered As JSON Null
    [Documentation]    power_rate_of_change (power_port_measurement_from, transform.rs:129-135)
    ...    and energy_storage.run_time (energy_storage_state_from, transform.rs:156-163) are both
    ...    hard-coded f32::NAN — no Seplos telemetry field maps to either. The wire serializer's
    ...    NaN -> JSON `null` policy (lib_mapping::wire; also asserted in tms_validate.py's
    ...    check_nan_policy) must hold on the REAL rendered/broker-relayed payload, not just in
    ...    a unit test.
    ${dc_msg}=    MqttKeywords.Latest Payload On Topic    ${TOPIC_DC}
    ${hw_msg}=    MqttKeywords.Latest Payload On Topic    ${TOPIC_HW}
    ${violations}=    Check Nan Policy    ${dc_msg}    ${hw_msg}
    Should Be Empty    ${violations}    msg=NaN policy violations: ${violations}
