"""
Robot Framework library for managing the seplos-tcp-sim subprocess.
Controls a Rust simulator that emulates a Seplos BMS over TCP.
"""

import subprocess
import threading
import time
import re
from pathlib import Path


class SeplosSimKeywords:
    """
    Manages seplos-tcp-sim subprocess and provides control keywords.
    """

    ROBOT_LIBRARY_SCOPE = 'TEST SUITE'

    def __init__(self, sim_binary_path=None):
        """
        Initialize the simulator keywords.

        Args:
            sim_binary_path: Path to seplos-tcp-sim binary or command.
                             Defaults to ./target/debug/seplos-tcp-sim if it exists,
                             otherwise uses 'cargo run -p test-kit --features tcp-sim --bin seplos-tcp-sim'
        """
        if sim_binary_path:
            self.sim_binary_path = sim_binary_path
        else:
            # Try to find prebuilt binary first
            default_binary = Path('./target/debug/seplos-tcp-sim')
            if default_binary.exists():
                self.sim_binary_path = str(default_binary)
            else:
                self.sim_binary_path = 'cargo run -p test-kit --features tcp-sim --bin seplos-tcp-sim --'

        self.process = None
        self.stdout_thread = None
        self.stderr_thread = None
        self.stdout_lines = []
        self.stderr_lines = []
        self.stdout_lock = threading.Lock()
        self.stderr_lock = threading.Lock()

    def start_seplos_sim(self, connect_addr, behavior="normal", state="sample14", extra_args=None):
        """
        Start the seplos-tcp-sim subprocess and apply the initial state/behavior.

        `seplos-tcp-sim` (crates/test-kit/src/bin/seplos_tcp_sim.rs) accepts only
        `--connect HOST:PORT` / `--listen PORT` / `--dump-fixtures DIR` on argv;
        behavior and state are NOT CLI flags. They are set via the stdin control
        protocol (`behavior <name>`, `state <name>`) after the process starts, the
        same mechanism `set_sim_behavior`/`set_sim_state` use mid-run. The sim
        already defaults to Behavior::Normal + BatteryState::sample_14_cell(), so
        these two lines simply make the requested initial state explicit.

        Args:
            connect_addr: Address to connect to (e.g., '192.0.2.1:5001')
            behavior: Behavior mode (default 'normal')
            state: Initial state (default 'sample14')
            extra_args: Additional command-line arguments (list)

        Raises:
            RuntimeError: If subprocess fails to start
        """
        if self.process is not None:
            raise RuntimeError("Simulator already running. Call stop_seplos_sim() first.")

        cmd = []
        if ' ' in self.sim_binary_path or self.sim_binary_path.startswith('cargo'):
            # Parse cargo command form
            cmd = self.sim_binary_path.split() if self.sim_binary_path.startswith('cargo') else [self.sim_binary_path]
        else:
            cmd = [self.sim_binary_path]

        cmd.extend(['--connect', connect_addr])

        if extra_args:
            cmd.extend(extra_args)

        try:
            self.process = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1
            )
        except Exception as e:
            raise RuntimeError(f"Failed to start seplos-tcp-sim: {e}")

        # Start threads to capture stdout/stderr
        self.stdout_thread = threading.Thread(target=self._read_stdout, daemon=True)
        self.stderr_thread = threading.Thread(target=self._read_stderr, daemon=True)
        self.stdout_thread.start()
        self.stderr_thread.start()

        # Apply initial state/behavior via the stdin control protocol (state first,
        # since `state <name>` replaces the whole BatteryState including faults).
        self.set_sim_state(state)
        self.set_sim_behavior(behavior)

    def _read_stdout(self):
        """Read stdout lines from the process."""
        if not self.process:
            return
        try:
            for line in self.process.stdout:
                with self.stdout_lock:
                    self.stdout_lines.append(line.rstrip('\n'))
        except Exception:
            pass

    def _read_stderr(self):
        """Read stderr lines from the process."""
        if not self.process:
            return
        try:
            for line in self.process.stderr:
                with self.stderr_lock:
                    self.stderr_lines.append(line.rstrip('\n'))
        except Exception:
            pass

    def set_sim_behavior(self, behavior):
        """
        Set simulator behavior mode (write to stdin).

        Args:
            behavior: Behavior name (e.g., 'normal', 'fault')
        """
        self._write_sim_command(f"behavior {behavior}")

    def set_sim_faults(self, names):
        """
        Set active faults (write comma-separated list to stdin).

        Args:
            names: Comma-separated fault names (e.g., 'overvoltage,overcurrent')
        """
        self._write_sim_command(f"faults {names}")

    def set_sim_state(self, state):
        """
        Set simulator state (write to stdin).

        Args:
            state: State name (e.g., 'sample14', 'charging')
        """
        self._write_sim_command(f"state {state}")

    def _write_sim_command(self, command):
        """Write a command to the simulator's stdin."""
        if not self.process or not self.process.stdin:
            raise RuntimeError("Simulator not running or stdin not available")
        try:
            self.process.stdin.write(f"{command}\n")
            self.process.stdin.flush()
        except Exception as e:
            raise RuntimeError(f"Failed to write command to simulator: {e}")

    def sim_response_events(self):
        """
        Parse stdout events from the simulator.
        Looks for lines matching: 'responded cmd=0x<CC> seq=<n> t=<monotonic_ns>'

        Returns:
            List of dicts with keys {cmd, seq, t_ns}
        """
        events = []
        pattern = r'responded\s+cmd=0x([0-9a-fA-F]+)\s+seq=(\d+)\s+t=(\d+)'

        with self.stdout_lock:
            for line in self.stdout_lines:
                match = re.search(pattern, line)
                if match:
                    events.append({
                        'cmd': int(match.group(1), 16),
                        'seq': int(match.group(2)),
                        't_ns': int(match.group(3))
                    })

        return events

    def stop_seplos_sim(self):
        """
        Stop the simulator subprocess cleanly.

        Returns:
            Tuple of (stdout_lines, stderr_lines)
        """
        if not self.process:
            return ([], [])

        try:
            self.process.terminate()
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        except Exception:
            pass

        # Wait for capture threads to finish
        if self.stdout_thread:
            self.stdout_thread.join(timeout=1)
        if self.stderr_thread:
            self.stderr_thread.join(timeout=1)

        with self.stdout_lock:
            stdout = list(self.stdout_lines)
        with self.stderr_lock:
            stderr = list(self.stderr_lines)

        self.process = None
        return (stdout, stderr)
