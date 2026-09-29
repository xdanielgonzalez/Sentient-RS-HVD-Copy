#!/usr/bin/env bash
# gpio-watch.sh — live High/Low level monitor for bench GPIO lines (libgpiod v2).
#
# Polls a set of lines and prints their level, refreshing in place, so you can
# watch a wire go H/L while probing power / reset / SWD by hand — no meter, no
# openocd. Each poll forces the line to input (high-Z) and reads the level the
# *external* circuit is holding, then releases it.
#
# Handy bring-up trick: with the debug JST-SH seated and the Feather POWERED,
# SWDIO reads H (the RP2350's internal pull-up on the SWD data line). SWDIO
# stuck L with power on ⇒ the target isn't driving it (unpowered / held in
# reset / GND not shared) — the same failure `mcu-swd-probe` reports as
# "cannot read IDR", but visible live while you wiggle the wiring.
#
# NOTE: stop openocd first. It holds SWCLK/SWDIO, so a poll of a busy line
# shows "ERR: ... Device or resource busy" here rather than a level.
#
# Usage: gpio-watch.sh [-c CHIP] [-i INTERVAL_SECS] SPEC...
#   SPEC = OFFSET[:LABEL]     e.g.  11:SWCLK  10:SWDIO  9:RUN
#   defaults: -c gpiochip0  -i 0.2  and SPECs "11:SWCLK 10:SWDIO"
set -u

CHIP=gpiochip0
INT=0.2
while getopts "c:i:h" opt; do
  case "$opt" in
    c) CHIP=$OPTARG ;;
    i) INT=$OPTARG ;;
    h) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "bad option; -h for help" >&2; exit 2 ;;
  esac
done
shift $((OPTIND - 1))

SPECS=("$@")
# Default: both SWD legs — DEVICE (Feather, 24/25) + SIMULATOR (Pico2W, 21/20).
[ ${#SPECS[@]} -eq 0 ] && SPECS=(24:DEV_SWCLK 25:DEV_SWDIO 21:SIM_SWCLK 20:SIM_SWDIO)

command -v gpioget >/dev/null 2>&1 || { echo "gpioget (libgpiod v2) not found on this host" >&2; exit 1; }

OFFS=()
LABS=()
for s in "${SPECS[@]}"; do
  off=${s%%:*}
  lab=${s#*:}
  [ "$lab" = "$s" ] && lab="gpio$off"
  OFFS+=("$off")
  LABS+=("$lab")
done

printf 'watching %s on %s every %ss (Ctrl-C to stop; stop openocd first)\n' \
  "${SPECS[*]}" "$CHIP" "$INT"
trap 'printf "\n"; exit 0' INT

while :; do
  if raw=$(gpioget -c "$CHIP" --numeric "${OFFS[@]}" 2>&1); then
    read -ra V <<<"$raw"
    line=""
    for i in "${!OFFS[@]}"; do
      v=${V[i]:-?}
      case "$v" in 1) hl=H ;; 0) hl=L ;; *) hl=? ;; esac
      line+=$(printf '%s(%s)=%s  ' "${LABS[i]}" "${OFFS[i]}" "$hl")
    done
    printf '\r%-72s' "$line"
  else
    printf '\r%-72s' "ERR: $raw"
  fi
  sleep "$INT"
done
