#!/bin/sh
# socat bridges a local pty (bin-rpi's "serial port") to the seplos-tcp-sim TCP service.
# ptys cannot cross container boundaries (the link resolves to /dev/pts/N in this
# container's devpts), so socat and bin-rpi share this one container.
set -eu
: "${SIM_HOST:?}" "${SIM_PORT:?}"
PTY_LINK="${PTY_LINK:-/tmp/ttySIM}"

# Address order matters: pty first, tcp second — the pty exists while TCP still
# retries, and bin-rpi tolerates the silent bus (read_timeout_ms -> IoError::Timeout)
# until the sim answers. retry= covers the sim Service not resolving/accepting yet.
socat -d pty,raw,echo=0,link="$PTY_LINK" \
      "tcp:${SIM_HOST}:${SIM_PORT},connect-timeout=5,retry=120,interval=2" &
SOCAT_PID=$!

i=0
while [ ! -e "$PTY_LINK" ]; do
  i=$((i+1))
  [ "$i" -gt 60 ] && { echo "fatal: pty $PTY_LINK never appeared" >&2; exit 1; }
  kill -0 "$SOCAT_PID" 2>/dev/null || { echo "fatal: socat exited before creating the pty" >&2; exit 1; }
  sleep 1
done

# If socat dies later (e.g. the sim pod restarted), the pty is stale — terminate so
# Kubernetes restarts this container and the bridge comes back clean. $$ becomes
# bin-rpi's PID after the exec below.
( while kill -0 "$SOCAT_PID" 2>/dev/null; do sleep 2; done
  echo "socat exited — terminating bin-rpi for a clean container restart" >&2
  kill -TERM $$ 2>/dev/null ) &

exec /usr/local/bin/bin-rpi
