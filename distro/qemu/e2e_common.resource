*** Settings ***
Documentation    Shared setup/teardown + variables for the Yocto qemuarm64 functional e2e
...    (Task #156): boot the merged-sysext `sentient-image`, feed simulated Seplos frames,
...    assert real TMS payloads over the embedded MQTT broker via an external stock paho client.
...
...    Originally modeled on the retired MCU harness's e2e_common.resource (same "one Suite
...    Setup keyword, fully qualified library calls, retained-message-friendly ordering" shape)
...    but genuinely different underneath: the boot mechanism is `runqemu` (YoctoQemuKeywords,
...    NOT a bare qemu-system-arm launch), and the guest is a full Yocto Linux image (systemd +
...    the merged sentient-app sysext), not a bare-metal firmware ELF.
# Library import notes (kept as plain comments, NOT `...` continuations — those are Robot
# argument-continuation syntax and would be parsed as extra Library arguments, not documentation):
#
# - ALL four libraries live in this directory's lib/. They used to be split across two trees:
#   MqttKeywords.py, SeplosSimKeywords.py and tms_validate.py were imported cross-tree from the
#   MCU (Zephyr) harness so the TMS schema had exactly one source of truth. That harness was
#   retired with the Embassy pivot (docs/decisions/0010-mcu-runtime-embassy.md) and its tree
#   deleted, which broke every Library import here; the three platform-agnostic libraries were
#   relocated into lib/ — this suite is now their sole owner and consumer.
# - lib/YoctoQemuKeywords.py boots the image via `runqemu`; see its own module docstring for why
#   the launch mechanism is necessarily its own file.
# - lib/MqttKeywords.py and lib/SeplosSimKeywords.py are platform-agnostic (a paho subscriber and
#   a seplos-tcp-sim subprocess controller) — they were reused unchanged and stay unchanged.
#   SeplosSimKeywords takes no binary-path constructor arg here: its __init__ already defaults to
#   ./target/debug/seplos-tcp-sim when present — exactly the path uat.yml's "Build Seplos sim"
#   step produces (`cargo build --locked -p test-kit --bin seplos-tcp-sim --features tcp-sim`,
#   invoked from repo root).
# - lib/tms_validate.py is pure (no imports, no I/O) — the same file distro/qemu/lib/
#   tms_mqtt_check.py imports as a sibling module; Robot's own `Library` resolution loads it
#   directly here. One TMS schema source of truth, now single-tree.
Library    ${CURDIR}/lib/YoctoQemuKeywords.py
Library    ${CURDIR}/lib/MqttKeywords.py
Library    ${CURDIR}/lib/SeplosSimKeywords.py
Library    ${CURDIR}/lib/tms_validate.py

*** Variables ***
# --- Broker + topics --------------------------------------------------------------------------
# Device id + topic prefix are the SHIPPED distro/meta-sentient/recipes-sentient/sentient-agent/
# files/sentient.toml defaults ([device] id = "METEOR-001", [publish] prefix = "tms") — this suite
# does NOT override the on-image config (unlike distro/qemu/yocto-e2e.sh's standalone dev harness,
# which renders + wic-cp's an e2e-only sentient.toml). See topic_into() in
# crates/lib-mapping/src/sink/mod.rs (`{prefix}/{device_id}/{snake_case(TmsTopic)}`) and
# crates/lib-schema/src/lib.rs's `pub mod topics` for the exact slug strings this derives from.
${DEVICE_ID}          METEOR-001
${TOPIC_DC}           tms/METEOR-001/dc_measurement_update
${TOPIC_HW}           tms/METEOR-001/power_hardware_update
${TOPIC_DIAG}         tms/METEOR-001/active_diagnostic_state

# --- Yocto / runqemu wiring --------------------------------------------------------------------
${MACHINE}            qemuarm64
# IMAGE_DIR / KAS_WORK_DIR are supplied by uat.yml's `robot --variable` invocation (or a local
# `yocto-e2e.sh`-style caller); defaults here are for a manual `robot` run from repo root.
${IMAGE_DIR}          %{IMAGE_DIR=./yocto/build/tmp/deploy/images/qemuarm64}
${KAS_WORK_DIR}       %{KAS_WORK_DIR=./yocto}
# SEPLOS_SIM_BIN is NOT read here: SeplosSimKeywords.__init__ self-discovers
# ./target/debug/seplos-tcp-sim (see the Library import comment above). uat.yml still exports
# the env var (for parity with a possible future direct-Python caller) but this suite's Robot
# side does not consume it — documented so the unused-looking env var isn't mistaken for a bug.
${MQTT_HOST_PORT}     18830
${SEPLOS_HOST_PORT}   15001
${BROKER_HOST}        127.0.0.1

# --- Boot / banner text ---
# "Started ... Sentient METEOR TMS telemetry adapter" is systemd logging
# sentient-agent.service's [Unit] Description (see distro/meta-sentient/recipes-sentient/
# sentient-agent/files/sentient-agent.service) when the unit reaches `active`; uat.yml's own
# preflight step greps this same distinctive text (with the "METEOR" substring) as its
# success signal. "poll loop started" is impl_app::run's own log::info! banner
# (crates/impl-app/src/lib.rs) once the poll loop begins — the more precise readiness signal
# for THIS suite, since it means the run loop (not just the systemd unit) is live.
${AGENT_STARTED}      METEOR
${POLL_STARTED}       poll loop started

${BOOT_TIMEOUT}       300
${FIRST_TIMEOUT}      90
${STD_TIMEOUT}        30

*** Keywords ***
Standard Yocto E2e Setup
    [Documentation]    Boot sentient-image under runqemu, start the Seplos sim, connect the
    ...    external MQTT subscriber. Retained messages make subscribe timing race-free — a late
    ...    subscribe still immediately receives the last-published payload on each topic.
    [Arguments]    ${behavior}=normal    ${state}=sample14
    ${ports}=    Start Yocto Qemu    ${IMAGE_DIR}    machine=${MACHINE}    kas_work_dir=${KAS_WORK_DIR}
    ...    mqtt_host_port=${MQTT_HOST_PORT}    seplos_host_port=${SEPLOS_HOST_PORT}
    Log    runqemu launched: ${ports}
    Wait For Console Line    ${AGENT_STARTED}    timeout=${BOOT_TIMEOUT}
    Wait For Console Line    ${POLL_STARTED}    timeout=${FIRST_TIMEOUT}
    Start Seplos Sim    127.0.0.1:${SEPLOS_HOST_PORT}    behavior=${behavior}    state=${state}
    MqttKeywords.Connect External Subscriber    ${BROKER_HOST}    ${MQTT_HOST_PORT}    timeout=${BOOT_TIMEOUT}
    MqttKeywords.Subscribe    tms/#
    MqttKeywords.Subscribe    monitor/#

Standard Yocto E2e Teardown
    [Documentation]    Best-effort teardown; continue on individual failures so one broken step
    ...    doesn't mask console/log evidence needed to diagnose the others.
    Run Keyword And Ignore Error    Save Console Dump    ${OUTPUT_DIR}/qemu_console.log
    Run Keyword And Ignore Error    Stop Yocto Qemu
    Run Keyword And Ignore Error    Stop Seplos Sim
    Run Keyword And Ignore Error    MqttKeywords.Disconnect Subscriber
