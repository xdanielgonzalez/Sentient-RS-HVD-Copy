#!/usr/bin/env bash
# rtt-capture.sh — capture defmt RTT from BOOT. Holds the target halted with the
# RTT server up, connects the capture first, then resumes via the telnet port so
# the one-shot boot logs (e.g. "ui: first frame flushed OK") land in the window.
# Decode /tmp/rtt.bin locally: `defmt-print -e <ELF> < rtt.bin`.
set -u
SECS="${1:-15}"
SWCLK="${2:-25}"
SWDIO="${3:-24}"
OUT=/tmp/rtt.bin

pkill -f "rtt server" 2>/dev/null || true
pkill openocd 2>/dev/null || true
sleep 1

# init + reset halt: target is HALTED and stays that way. stdin held open (no EOF -> no self-shutdown).
setsid openocd \
  -c "adapter driver linuxgpiod" \
  -c "adapter gpio swclk -chip 0 ${SWCLK}" -c "adapter gpio swdio -chip 0 ${SWDIO}" \
  -c "transport select swd" -c "source [find target/rp2350.cfg]" \
  -c "init" -c "reset halt" \
  -c "rtt setup 0x20000000 0x80000 {SEGGER RTT}" -c "rtt start" \
  -c "rtt server start 9090 0" \
  0< <(sleep $((SECS + 15))) >/tmp/ocd-rtt.log 2>&1 &

for _ in $(seq 1 16); do
  ss -ltn 2>/dev/null | grep -q ':9090 ' && break
  sleep 0.5
done
if ! ss -ltn 2>/dev/null | grep -q ':9090 '; then
  echo "FAIL: openocd not listening on 9090"; tail -8 /tmp/ocd-rtt.log; pkill openocd 2>/dev/null; exit 1
fi

# Connect the capture BEFORE resuming, so boot output isn't lost.
timeout "${SECS}" nc localhost 9090 > "${OUT}" &
NCPID=$!
sleep 1.5

# Resume via the telnet command port (target boots now, into the connected capture).
{ echo "resume"; sleep 0.3; } | nc -q1 localhost 4444 >/dev/null 2>&1 || true
echo "resumed; capturing ${SECS}s from boot -> ${OUT}"

wait "${NCPID}" 2>/dev/null || true
BYTES=$(stat -c %s "${OUT}" 2>/dev/null || echo 0)
echo "captured ${BYTES} bytes"

pkill -f "rtt server" 2>/dev/null || true
pkill openocd 2>/dev/null || true
grep -iE "control block found|Listening on port 9090|Error|halted" /tmp/ocd-rtt.log | head -4
