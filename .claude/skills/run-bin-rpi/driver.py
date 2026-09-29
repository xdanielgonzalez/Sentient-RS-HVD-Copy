#!/usr/bin/env python3
"""Driver for bin-rpi: launches it against a simulated Seplos BMS on a pty,
subscribes to its embedded MQTT broker, and reports what it publishes.

Subcommands:
    up      -- build (if needed), launch the Seplos pty simulator + bin-rpi in the
               foreground, block until SIGTERM/SIGINT, then clean up. Meant to be run
               under `nohup ... &`; see SKILL.md.
    sub     -- connect to the running broker and print N seconds of tms/# messages.
    down    -- stop a background `up` started via its PID file.
    smoke   -- one-shot: up (bounded), sub, down. Exits 0 iff readings were published.

State (workdir, pid, broker port, config path) is written to <workdir>/state.json.
"""
import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import threading
import time
import tty
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
BIN_RPI = REPO_ROOT / "target" / "debug" / "bin-rpi"

# The live 14-cell Seplos telemetry reply for DIP address 0 (from
# crates/test-e2e/tests/seplos_at_rate_e2e.rs::LIVE_FRAME) -- a byte-exact real capture, answered
# verbatim to any command frame the simulated bus receives (bin-rpi's default poll config only
# polls DIP 0, so no request-parsing is needed on this side -- same shortcut host_pty.rs's own
# tests take).
LIVE_FRAME = (
    b"~20004600A08E00000E0F360F360F360F360F350F360F360F360F360F370F370F360F360F35"
    b"060AF10AF90AF90AF30B370B0CFF5C154C36820A465003074650000203E8154CFFF800000079"
    b"004DDEC8\r"
)

DEFAULT_WORKDIR = Path("/tmp/bin-rpi-driver")


def log(msg):
    print(f"[driver] {msg}", file=sys.stderr, flush=True)


def build():
    log("cargo build -p bin-rpi ...")
    subprocess.run(
        ["cargo", "build", "-p", "bin-rpi"], cwd=REPO_ROOT, check=True
    )


def write_config(workdir: Path, pty_slave: str, broker_port: int, interval_ms: int) -> Path:
    cfg = f"""[device]
id = "METEOR-DRIVER"

[serial]
kind = "rs232"        # rs232, not rs485: a pty has no RTS modem line for direction control
port = "{pty_slave}"
baud = 19200
read_timeout_ms = 1000

[poll]
dip_addresses = [0]
interval_ms = {interval_ms}
fault_every = 0

[broker]
host = "127.0.0.1"
port = {broker_port}

[publish]
prefix = "tms"
"""
    cfg_path = workdir / "sentient.toml"
    cfg_path.write_text(cfg)
    return cfg_path


def spawn_pty_responder(stop_event: threading.Event):
    """Open a pty pair and start a thread that answers every command frame (bytes up to '\\r')
    with LIVE_FRAME. Returns the slave device path (e.g. /dev/pts/4); the master fd is kept open
    for the life of the process (closing it would hang up the slave bin-rpi has open)."""
    master_fd, slave_fd = os.openpty()
    slave_path = os.ttyname(slave_fd)
    # Set the slave to raw mode to prevent canonical/echo modes that would interfere with
    # binary telemetry frames. Keep the slave_fd open (raw mode settings persist on the device
    # via the open fd) to ensure proper PTY lifecycle and prevent "Device or resource busy"
    # errors when HostSerial::open tries to configure the slave.
    tty.setraw(slave_fd)

    def responder():
        buf = bytearray()
        while not stop_event.is_set():
            try:
                chunk = os.read(master_fd, 256)
            except OSError:
                break
            if not chunk:
                break
            log(f"pty rx: {chunk!r}")
            buf += chunk
            if b"\r" in buf:
                buf.clear()
                try:
                    os.write(master_fd, LIVE_FRAME)
                    log(f"pty tx: {LIVE_FRAME!r}")
                except OSError:
                    break

    t = threading.Thread(target=responder, daemon=True)
    t.start()
    return slave_path, master_fd, slave_fd


def wait_for_ready(log_path: Path, timeout=15):
    deadline = time.time() + timeout
    marker = "embedded MQTT server up"
    while time.time() < deadline:
        if log_path.exists() and marker in log_path.read_text(errors="ignore"):
            return True
        time.sleep(0.2)
    return False


def wait_for_port(port: int, timeout=10):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return True
        except OSError:
            time.sleep(0.2)
    return False


def port_in_use(port: int) -> bool:
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.3):
            return True
    except OSError:
        return False


def cmd_up(args):
    workdir = Path(args.workdir)
    workdir.mkdir(parents=True, exist_ok=True)

    # A stale bin-rpi (or anything else) already listening on this port is dangerous, not just
    # inconvenient: bin-rpi's embedded rumqttd doesn't fail loudly if its own bind loses a race, so
    # a fresh bin-rpi can end up as a CLIENT of the stray listener instead of the broker -- its
    # telemetry publishes still "succeed" (against the wrong broker), producing a false-positive
    # smoke test. Refuse to start rather than risk that.
    if port_in_use(args.port):
        log(
            f"port {args.port} is already in use -- refusing to start (a stale bin-rpi from a "
            f"previous run? check `ps aux | grep bin-rpi` and `down`/kill it, or pick a different "
            f"--port)"
        )
        sys.exit(1)

    if not BIN_RPI.exists() or args.build:
        build()
    if not BIN_RPI.exists():
        log(f"missing {BIN_RPI} after build")
        sys.exit(1)

    stop_event = threading.Event()
    slave_path, master_fd, slave_fd = spawn_pty_responder(stop_event)
    log(f"seplos simulator on pty {slave_path}")

    cfg_path = write_config(workdir, slave_path, args.port, args.interval_ms)
    log_path = workdir / "bin-rpi.log"

    env = dict(os.environ)
    env["SENTIENT_CONFIG"] = str(cfg_path)
    env["RUST_LOG"] = env.get("RUST_LOG", "info")
    with open(log_path, "wb") as logf:
        proc = subprocess.Popen(
            [str(BIN_RPI)], cwd=REPO_ROOT, env=env, stdout=logf, stderr=subprocess.STDOUT
        )

    state = {
        "driver_pid": os.getpid(),
        "binrpi_pid": proc.pid,
        "workdir": str(workdir),
        "cfg_path": str(cfg_path),
        "log_path": str(log_path),
        "broker_port": args.port,
        "pty_slave": slave_path,
    }
    (workdir / "state.json").write_text(json.dumps(state, indent=2))
    (workdir / "driver.pid").write_text(str(os.getpid()))

    if not wait_for_ready(log_path, timeout=args.ready_timeout):
        log(f"bin-rpi did not report ready within {args.ready_timeout}s; see {log_path}")
        proc.terminate()
        sys.exit(1)
    log(f"bin-rpi ready; broker on 127.0.0.1:{args.port}; log={log_path}")

    stopping = {"flag": False}

    def handle_signal(signum, frame):
        stopping["flag"] = True

    if threading.current_thread() is threading.main_thread():
        signal.signal(signal.SIGTERM, handle_signal)
        signal.signal(signal.SIGINT, handle_signal)

    if args.seconds:
        deadline = time.time() + args.seconds
    else:
        deadline = None

    try:
        while not stopping["flag"] and proc.poll() is None:
            if deadline and time.time() >= deadline:
                break
            time.sleep(0.2)
    finally:
        stop_event.set()
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        os.close(master_fd)
        os.close(slave_fd)
        log("bin-rpi stopped")


def cmd_sub(args):
    try:
        import paho.mqtt.client as mqtt
    except ImportError:
        log("paho-mqtt not installed: pip install --user paho-mqtt")
        sys.exit(1)

    messages = []

    def on_message(client, userdata, msg):
        messages.append((msg.topic, msg.payload.decode(errors="replace")))
        print(f"{msg.topic} {msg.payload.decode(errors='replace')}")

    # Use an explicit client ID to avoid "InvalidClientId" errors with rumqttd
    client = mqtt.Client(mqtt.CallbackAPIVersion.VERSION2, client_id="driver-sub")
    client.on_message = on_message
    client.connect("127.0.0.1", args.port, keepalive=10)
    client.subscribe(args.topic)
    client.loop_start()
    time.sleep(args.seconds)
    client.loop_stop()
    client.disconnect()

    log(f"received {len(messages)} message(s) on '{args.topic}'")
    if args.require and not messages:
        sys.exit(1)


def cmd_down(args):
    workdir = Path(args.workdir)
    state_path = workdir / "state.json"
    if not state_path.exists():
        log(f"no state file at {state_path}; nothing to stop")
        return
    state = json.loads(state_path.read_text())
    for key in ("driver_pid", "binrpi_pid"):
        pid = state.get(key)
        if not pid:
            continue
        try:
            os.kill(pid, signal.SIGTERM)
            log(f"sent SIGTERM to {key}={pid}")
        except ProcessLookupError:
            pass
    time.sleep(0.5)
    for key in ("driver_pid", "binrpi_pid"):
        pid = state.get(key)
        if not pid:
            continue
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


def cmd_smoke(args):
    workdir = Path(args.workdir)
    up_args = argparse.Namespace(
        workdir=str(workdir),
        port=args.port,
        interval_ms=args.interval_ms,
        build=args.build,
        ready_timeout=15,
        seconds=args.seconds + 3,
    )
    up_thread = threading.Thread(target=cmd_up, args=(up_args,), daemon=True)
    up_thread.start()

    if not wait_for_port(args.port, timeout=20):
        log("broker port never opened")
        sys.exit(1)
    time.sleep(1.0)  # let the poll loop complete at least one cycle

    sub_args = argparse.Namespace(port=args.port, seconds=args.seconds, topic="tms/#", require=True)
    try:
        cmd_sub(sub_args)
    finally:
        up_thread.join(timeout=args.seconds + 5)
    log("smoke test passed" if up_thread.is_alive() is False else "smoke test: up still running")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)

    p_up = sub.add_parser("up", help="launch simulator + bin-rpi in the foreground")
    p_up.add_argument("--workdir", default=str(DEFAULT_WORKDIR))
    p_up.add_argument("--port", type=int, default=18830)
    p_up.add_argument("--interval-ms", type=int, default=500)
    p_up.add_argument("--build", action="store_true")
    p_up.add_argument("--ready-timeout", type=int, default=15)
    p_up.add_argument("--seconds", type=int, default=0, help="0 = run until signaled")
    p_up.set_defaults(func=cmd_up)

    p_sub = sub.add_parser("sub", help="subscribe and print messages")
    p_sub.add_argument("--port", type=int, default=18830)
    p_sub.add_argument("--seconds", type=float, default=5)
    p_sub.add_argument("--topic", default="tms/#")
    p_sub.add_argument("--require", action="store_true", help="exit 1 if nothing received")
    p_sub.set_defaults(func=cmd_sub)

    p_down = sub.add_parser("down", help="stop a background `up`")
    p_down.add_argument("--workdir", default=str(DEFAULT_WORKDIR))
    p_down.set_defaults(func=cmd_down)

    p_smoke = sub.add_parser("smoke", help="one-shot up+sub+down")
    p_smoke.add_argument("--workdir", default=str(DEFAULT_WORKDIR))
    p_smoke.add_argument("--port", type=int, default=18830)
    p_smoke.add_argument("--interval-ms", type=int, default=500)
    p_smoke.add_argument("--seconds", type=float, default=5)
    p_smoke.add_argument("--build", action="store_true")
    p_smoke.set_defaults(func=cmd_smoke)

    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
