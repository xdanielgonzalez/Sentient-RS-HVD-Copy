#!/usr/bin/env bash
# run_conformance.sh — WP7a stock-tooling DDS conformance oracle (story #321/#324).
#
# Runs INSIDE the `conformance` service (tools.Dockerfile, FROM the WP5 cyclonedds-probe image) with
# the stock CycloneDDS Python CLI (`cyclonedds ls/typeof/subscribe`) + the WP5 local-type probe
# (subscribe_assert.py) against the real `adapter` (bin-rpi, [dds] on) + real `sim` (seplos-tcp-sim),
# all sharing one network namespace (compose.yaml `network_mode: host`).
#
# Checks, mapped to the MIL-STD-3071 clauses the plan cites:
#   (a) cyclonedds ls   -> D.2 domain 0 + D.3 topic names + D.5 QoS/USER_DATA visible via SEDP
#                          discovery (NO XTypes needed). HARD assert.
#   (b) cyclonedds typeof -> XTypes TypeInformation propagation datum. REPORT ONLY this round.
#   (c) cyclonedds subscribe -> D.1 XCDR2 stock delivery oracle (XTypes-gated type discovery).
#                          HARD assert (samples must arrive).
#   (d) subscribe_assert.py -> local-type reader. Isolates the XTypes gate from the DATA-delivery
#                          gate: if (d) delivers but (c) does not, the blocker is XTypes type
#                          propagation, not the RTPS DATA path. REPORT ONLY (isolation datum).
#
# Exit: 0 iff every HARD assert passed; nonzero otherwise. A nonzero exit that pinpoints the
# WP6 zero-delivery blocker with stock tooling is the intended, honest outcome — not a harness bug.
set -uo pipefail

DOMAIN="${DDS_DOMAIN:-0}"
DEVICE_ID="${DDS_DEVICE_ID:-METEOR-001}"
WARMUP="${DDS_WARMUP_SECS:-25}"     # adapter needs time: serial link up + first DDS writes
SCAN="${DDS_SCAN_SECS:-8}"          # `-r` discovery-scan runtime for ls/typeof/subscribe
SUB_SECS="${DDS_SUB_SECS:-15}"      # bounded wall-clock per subscribe attempt
OUT="${DDS_OUT_DIR:-/out}"
mkdir -p "$OUT" 2>/dev/null || true

# TMS writer topics that lib_dds's lazy writer map ACTUALLY creates writers for today — the only
# ones that can appear over SEDP. HARD-asserted present. DcLoadSharingState joined in WP6.5 (the one
# optional-free topic of the eight that dust_dds_derive can mirror).
LS_EXPECT_TOPICS=(Heartbeat DcMeasurementUpdate DcLoadSharingState)
# Topics the poll/state tasks emit but lib_dds drops with "no writer mapping ... yet" — no writer, so
# not discoverable. REPORT-ONLY: their absence is the known writer-map gap (7 @optional-bearing topics
# blocked at the dust_dds_derive layer — see lib_dds::dustdds::types::patch module doc), not a
# harness/transport regression.
LS_WP6_GAP_TOPICS=(DeviceInfo EnergyStartStopState ActiveMicrogridControllerState PowerHardwareUpdate StorageUpdate ActiveDiagnosticState PowerPortState)
TYPEOF_TOPICS=(Heartbeat DcMeasurementUpdate)
SUBSCRIBE_TOPICS=(Heartbeat DcMeasurementUpdate)

fail=0
note() { printf '\n========== %s ==========\n' "$*"; }

note "conformance harness config"
echo "CYCLONEDDS_URI=${CYCLONEDDS_URI:-<unset>}"
echo "domain=$DOMAIN device=$DEVICE_ID warmup=${WARMUP}s scan=${SCAN}s sub=${SUB_SECS}s"
ip -6 -o addr show 2>/dev/null | awk '{print "iface:", $2, $4}' || true

echo "waiting ${WARMUP}s for the adapter's DDS participant + first samples ..."
sleep "$WARMUP"

# ---- (a) cyclonedds ls : discovery + QoS (SEDP; no XTypes) -----------------------------------
note "(a) cyclonedds ls  [D.2 domain / D.3 topics / D.5 QoS+USER_DATA]"
LS_OUT="$OUT/ls.txt"
cyclonedds ls -i "$DOMAIN" -r "${SCAN}s" --qos --show-self --suppress-progress-bar --color none \
  > "$LS_OUT" 2>&1
cat "$LS_OUT"

note "(a) ls assertions"
for t in "${LS_EXPECT_TOPICS[@]}"; do
  if grep -qw "$t" "$LS_OUT"; then
    echo "PASS ls: writer topic '$t' discovered on domain $DOMAIN"
  else
    echo "FAIL ls: writer topic '$t' NOT discovered"
    fail=1
  fi
done
if grep -q "$DEVICE_ID" "$LS_OUT"; then
  echo "PASS ls: device id '$DEVICE_ID' visible (USER_DATA / participant data)"
else
  echo "WARN ls: device id '$DEVICE_ID' not visible in ls output (USER_DATA surfacing is CLI-dependent)"
fi
echo "--- WP6 writer-map gap (report only; these topics have no lib_dds writer yet) ---"
for t in "${LS_WP6_GAP_TOPICS[@]}"; do
  if grep -qw "$t" "$LS_OUT"; then
    echo "INFO ls: '$t' unexpectedly discovered (writer map extended?)"
  else
    echo "INFO ls: '$t' absent (expected — WP6 'no writer mapping yet')"
  fi
done

# ---- (b) cyclonedds typeof : XTypes TypeInformation propagation (REPORT ONLY) ----------------
note "(b) cyclonedds typeof  [XTypes TypeInformation propagation — REPORT ONLY]"
for t in "${TYPEOF_TOPICS[@]}"; do
  echo "----- typeof $t -----"
  timeout $((SCAN + 12)) cyclonedds typeof "$t" -i "$DOMAIN" -r "${SCAN}s" --suppress-progress-bar \
    > "$OUT/typeof_$t.txt" 2>&1
  rc=$?
  cat "$OUT/typeof_$t.txt"
  echo "(typeof $t exit=$rc — nonzero/empty => remote writer did not propagate XTypes TypeInformation)"
done

# ---- (c) cyclonedds subscribe : stock delivery oracle (XTypes-gated) -------------------------
note "(c) cyclonedds subscribe  [D.1 XCDR2 stock delivery oracle]"
for t in "${SUBSCRIBE_TOPICS[@]}"; do
  echo "----- subscribe $t (bounded ${SUB_SECS}s) -----"
  SUB_OUT="$OUT/subscribe_$t.txt"
  timeout "${SUB_SECS}s" cyclonedds subscribe "$t" -i "$DOMAIN" -r "${SCAN}s" \
    --type scan-random --qos scan-random --suppress-progress-bar --color none \
    > "$SUB_OUT" 2>&1
  rc=$?
  cat "$SUB_OUT"
  if grep -qiE "${DEVICE_ID}|sequence_number|device_id" "$SUB_OUT"; then
    echo "PASS subscribe: '$t' delivered >=1 decodable sample via stock CLI (rc=$rc)"
  else
    echo "FAIL subscribe: '$t' NO sample decoded by stock CLI (rc=$rc)"
    fail=1
  fi
done

# ---- (d) local_delivery_probe.py : BestEffort local-type reader (ISOLATION datum) ------------
# The published telemetry writers are Medium-profile = BestEffort/Volatile/KeepLast(1) (confirmed in
# the ls dump above). This probe offers exactly that, so a miss here is a DATA-delivery miss, NOT a
# QoS-mismatch artifact. (deploy/dds-probe/subscribe_assert.py requests Reliable and therefore will
# NOT match a BestEffort writer — it is not a valid DATA oracle for these topics; see its output.)
note "(d) local_delivery_probe.py  [BestEffort local-typed reader — isolates XTypes gate vs DATA gate]"
timeout $((SUB_SECS + 10)) python3 /probe/local_delivery_probe.py \
  --topic Heartbeat --topic DcMeasurementUpdate --domain "$DOMAIN" --timeout "$SUB_SECS" \
  > "$OUT/local_delivery.txt" 2>&1
rc=$?
cat "$OUT/local_delivery.txt"
echo "(local_delivery_probe exit=$rc — 0 => matching-QoS local reader DID receive: RTPS DATA path"
echo " works and the (c)/(b) miss is purely the XTypes TypeInformation gate. nonzero => DATA never"
echo " arrived even with matched QoS + local type: the dust_dds RTPS DATA-delivery path is the blocker.)"

# For contrast, also run the Reliable-QoS bespoke probe to SHOW the QoS-mismatch (report only).
echo "----- subscribe_assert.py (Reliable — expected NO match vs BestEffort writers) -----"
timeout $((SUB_SECS + 8)) python3 /probe/subscribe_assert.py \
  --topic Heartbeat --domain "$DOMAIN" --timeout 5 --expect-user-data "$DEVICE_ID" \
  > "$OUT/assert_Heartbeat.txt" 2>&1 || true
cat "$OUT/assert_Heartbeat.txt"

note "VERDICT"
if [ "$fail" -eq 0 ]; then
  echo "conformance: PASS (all hard asserts green)"
else
  echo "conformance: FAIL (hard assert(s) failed — see per-check lines; findings recorded to $OUT)"
fi
exit "$fail"
