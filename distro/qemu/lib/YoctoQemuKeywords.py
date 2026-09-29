"""
Robot Framework library for launching and controlling the Yocto `sentient-image` under `runqemu`
(Task #156 — the Seplos -> TMS -> MQTT functional e2e on the merged-sysext app).

This is the Yocto/`runqemu` analogue of the retired MCU harness's `QemuKeywords.py` (which drove a
bare `qemu-system-arm -machine lm3s6965evb` for the Zephyr firmware; that harness was deleted with
the Embassy pivot — see `docs/decisions/0010-mcu-runtime-embassy.md`). It was authored as a NEW
file, not a copy/edit of that one, because the launch mechanism is fundamentally different:

- `runqemu` (not a bare `qemu-system-aarch64` invocation) locates the MACHINE's kernel/rootfs/
  qemuboot.conf artifacts from a deploy-images directory (`IMAGE_DIR`, e.g.
  `<workspace>/yocto/build/tmp/deploy/images/qemuarm64`) and derives most of the qemu-system
  command line itself; extra options are passed through `qemuparams="..."`.
- `runqemu`/`qemu-system-aarch64` are NOT on PATH directly in the `sentient-yocto-builder` CI image
  — they come from the Yocto build's own native sysroot, populated by kas against the SAME
  `KAS_WORK_DIR` the uat.yml preflight step already built. This keyword reaches that environment
  via `kas shell distro/kas/sentient.yml -c "runqemu ..."` — the IDENTICAL mechanism uat.yml's own
  preflight step uses (NOT `source poky/oe-init-build-env`: this repo's kas manifest pins bitbake +
  openembedded-core as discrete repos, not the `poky` combo repo, so no `poky/` dir exists to
  source). BitBake/kas artifacts are owned by the non-root `builder` user (BitBake refuses to run
  as root; see `.ci/scripts/run-kas.sh`'s header), so the whole `kas shell` invocation is wrapped in
  `runuser -u builder -- ...`, mirroring uat.yml's own preflight step exactly.
- Networking is `slirp` user-net + an explicit `hostfwd` for the guest's embedded MQTT broker,
  plus (optionally) a second `-chardev socket` UART for the Seplos link — see this library's
  `start_yocto_qemu` docstring for the exact qemuparams and the load-bearing TODO(verify)s.

Console capture, `wait_for_console_line`, and process lifecycle management otherwise mirror
`QemuKeywords.py`'s shape (reader thread, line buffer, bounded wait, terminate-then-kill teardown)
on purpose — same proven pattern, applied to a different launch command.
"""

import os
import subprocess
import threading
import time


class YoctoQemuKeywords:
    """
    Robot Framework library for booting `sentient-image` under `runqemu` (qemuarm64) and capturing
    its console.

    ROBOT_LIBRARY_SCOPE = 'TEST SUITE'
    """

    ROBOT_LIBRARY_SCOPE = 'TEST SUITE'

    def __init__(self):
        self.qemu_process = None
        self.console_lock = threading.Lock()
        self.console_buffer = []
        self.console_text = []
        self.reader_thread = None
        self._stop_reader = False
        self.start_monotonic = None
        self.kas_work_dir = None
        self.builder_user = "builder"

    def start_yocto_qemu(self, image_dir, machine="qemuarm64", kas_work_dir=None,
                          mqtt_host_port=18830, seplos_host_port=15001,
                          seplos_guest_serial="qemuparams-extra-serial",
                          builder_user="builder", boot_timeout=300,
                          kas_config="distro/kas/sentient.yml", repo_root=None):
        """
        Boot `sentient-image` (base + merged app sysext, already written into IMAGE_DIR's rootfs by
        uat.yml's earlier "Reconstruct deploy dir" step) under `runqemu`, headless, with:

        - slirp user-net + an MQTT hostfwd forwarding a HOST port to the GUEST's embedded MQTT
          broker port 1883, injected via QB_SLIRP_OPT (`hostfwd=tcp:127.0.0.1:${mqtt_host_port}-:1883`)
          — NOT an explicit `-netdev` in qemuparams (that duplicated runqemu's own slirp net0; run 2).
        - BOTH `-serial` devices passed via qemuparams in slot order so QEMU assigns them correctly:
          `-serial mon:stdio` (ttyAMA0, the console → runqemu stdout) then
          `-serial chardev:seplossim` (ttyAMA1, the Seplos socket). Plus the
          `-chardev socket,...,server=on,wait=off` definition so `seplos-tcp-sim` dials in as a TCP
          client on `seplos_host_port` (same "QEMU is the server, the sim dials in" pattern as
          `distro/qemu/yocto-e2e.sh`, and as the retired MCU harness's `qemu-e2e.sh`). See the big SERIAL SLOT
          ORDER comment on the `qemuparams` assignment below for the run-4 fix + source citation.

        # RESOLVED (run 2/3/4): the preflight boot form + net + serial wiring are all settled — see
        # the qemuparams / QB_SLIRP_OPT assignments below. This IS a NEW, separate boot this keyword
        # performs for the Robot suite (the preflight's QEMU instance is killed before this step runs;
        # see uat.yml's "Preflight" step killing $QPID, and the later "Build Seplos sim" / "Run Robot
        # e2e" steps being independent shell steps in the same job with no QEMU alive between them).
        #
        # RESOLVED (UAT run 1): broker-bind + serial node + DeviceAllow all confirmed working.
        # The SHIPPED sentient.toml binds broker to 127.0.0.1 (slirp hostfwd can't reach guest
        # loopback) and [serial] to /dev/ttyAMA0. uat.yml injects the E2E overrides via debugfs in
        # TWO stages, SPLIT deliberately (see uat.yml's "Inject e2e broker-bind" step's big comment):
        #   - BEFORE preflight: broker host=0.0.0.0 ONLY, keeping [serial] port=/dev/ttyAMA0 so the
        #     preflight boot degrades cleanly on serial (ttyAMA0 = console, held by serial-getty ->
        #     open fails -> HostSerial::disconnected() -> non-fatal DEGRADED path). Pointing preflight
        #     at a real-but-unfed UART is what broke run 1 (read-timeout -> backoff -> watchdog kill).
        #   - AFTER preflight, before THIS keyword's (the Robot suite's) boot: [serial] port is
        #     re-pointed at /dev/ttyAMA1 (CONFIRMED by run 1's E2E-DIAG: 9040000.pl011 -> ttyAMA1,
        #     crw-rw---- root dialout 204,65) + a systemd drop-in adds `DeviceAllow=/dev/ttyAMA1 rw`
        #     (the shipped unit's DevicePolicy=closed + DeviceAllow=/dev/ttyUSB0 would otherwise
        #     cgroup-block the open). This is safe for the Robot boot precisely BECAUSE this keyword's
        #     runqemu wires the 2nd -serial chardev socket AND the suite starts seplos-tcp-sim to FEED
        #     ttyAMA1 — so bin-rpi reads real frames instead of timing out. The Robot boot reads the
        #     PERSISTENT rootfs (this keyword's runqemu has NO `snapshot`), so it picks up the
        #     post-preflight override; preflight's own `snapshot` boot already ran + tore down.
        #
        # TODO(verify-in-ci): /dev/ttyAMA1 is confirmed for the current pinned poky; if a poky bump
        # renumbers it, the E2E-DIAG unit re-dumps the real node every run — correct
        # E2E_SERIAL_GUEST_DEV in uat.yml's two override steps from that output. Also note bin-rpi's
        # reconnect() self-EBUSY (tokio-serial TIOCEXCL on its own still-open fd): harmless on the
        # happy path (sim stays connected), but a real fix is needed in bin-rpi before the suite can
        # drive sim disconnect/reconnect scenarios (flagged in uat.yml's serial-override step).

        Args:
            image_dir: Deploy-images dir containing sentient-image-<machine>.rootfs.ext4 / .qemuboot.conf / Image
            machine: Yocto MACHINE (default qemuarm64 — ARM only)
            kas_work_dir: KAS_WORK_DIR the SAME preflight step used (so the same kas-managed
                BitBake/native-sysroot environment resolves `runqemu` on PATH) — required; e.g.
                `$GITHUB_WORKSPACE/yocto`
            mqtt_host_port: host-side port forwarded to the guest's MQTT broker (1883)
            seplos_host_port: host-side TCP port seplos-tcp-sim dials to reach the guest UART
            seplos_guest_serial: informational only (kept for parity with distro/qemu/README.md's
                naming; the actual guest device path is NOT selectable from the host side, it is
                whatever qemuarm64/the kernel enumerates the second `-serial` slot as)
            builder_user: non-root user that owns the kas/BitBake tree (default 'builder')
            boot_timeout: unused here (kept for signature symmetry with wait_for_console_line callers)
            kas_config: kas manifest to `kas shell` into (default distro/kas/sentient.yml, same
                manifest uat.yml's preflight step uses)
            repo_root: absolute path to the repo checkout (so the kas manifest path resolves
                regardless of the caller's cwd); defaults to this file's repo (distro/qemu/lib/../../..)

        Raises:
            RuntimeError: if the process fails to start

        # NOTE (bug fixed post-authoring): this keyword originally did
        # `source poky/oe-init-build-env build` before invoking `runqemu` directly via `runuser`.
        # That path does NOT exist under this repo's kas manifest (distro/kas/sentient.yml pins
        # bitbake + openembedded-core as DISCRETE repos, not the `poky` combo repo — see that
        # manifest's own header comment), so `source poky/oe-init-build-env` would fail with
        # "No such file or directory" on the very first CI run. uat.yml's own preflight step never
        # does this either — it goes through `kas shell "distro/kas/sentient.yml" -c "runqemu ..."`,
        # which lets kas itself set up the native-sysroot PATH (bitbake/runqemu/qemu-system-aarch64)
        # the same way `kas build` did. Fixed here to use the identical `kas shell` mechanism so
        # this keyword's runqemu invocation is proven by the SAME code path the preflight already
        # exercises successfully, not a second, divergent (and wrong) one.
        """
        if kas_work_dir is None:
            raise RuntimeError("kas_work_dir is required (must match the preflight step's KAS_WORK_DIR)")
        if not os.path.isdir(image_dir):
            raise RuntimeError(f"IMAGE_DIR not found: {image_dir}")

        self.kas_work_dir = kas_work_dir
        self.builder_user = builder_user

        if repo_root is None:
            # distro/qemu/lib/YoctoQemuKeywords.py -> repo root is 3 dirs up.
            repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))

        # qemuparams carries the Seplos 2nd-UART chardev AND BOTH -serial devices, in the ORDER we
        # need QEMU to assign the ttyAMA* slots. The MQTT hostfwd goes through QB_SLIRP_OPT (below),
        # NOT an explicit -netdev here: runqemu's `slirp` keyword already emits
        # `<QB_NETWORK_DEVICE> <QB_SLIRP_OPT>` (verified in oe-core scripts/runqemu setup_slirp,
        # ~L1145-1169), so adding our own `-netdev user,id=net0` in qemuparams made a DUPLICATE net0 →
        # runqemu boot failed with error 1 (run 2). QB_NETWORK_DEVICE (virtio-net-pci) is provided by
        # the machine conf, so we don't add a -device either.
        #
        # ===== SERIAL SLOT ORDER (run-4 fix) — verified against oe-core scripts/runqemu source =====
        # Run 4's captured QEMU cmdline showed `-serial chardev:seplossim` (our single qemuparams
        # serial) landing FIRST → it became ttyAMA0 (the CONSOLE slot, per the baked kernel cmdline
        # `console=ttyAMA0`), while runqemu's own `-serial null` (ttyAMA1) and `-serial mon:stdio`
        # (ttyAMA2) came after. Net effect: the boot console went to ttyAMA0 = our Seplos socket (so
        # runqemu's stdout never saw the 'METEOR' banner → 300 s Robot timeout), AND bin-rpi (reading
        # /dev/ttyAMA1) got runqemu's `-serial null` instead of the sim. Both wrong.
        #
        # runqemu assembles the cmdline in setup_final() as:
        #     QB_OPT_APPEND  qemu_opt_script  qemuparams  -snapshot  setup_serial()
        # i.e. QEMUPARAMS IS APPENDED *BEFORE* setup_serial() runs. setup_serial() counts existing
        # `-serial` occurrences via the regex `(^| )-serial ` and has an early-out:
        #     elif serial_num >= 2: return   # <-- adds NOTHING further
        # So if the cmdline ALREADY holds two `-serial` before setup_serial() runs, runqemu appends
        # none of its own `-serial null` / `-serial mon:stdio`. We exploit exactly that: put BOTH
        # serials in qemuparams, in slot order —
        #     -serial mon:stdio       -> ttyAMA0  (console on runqemu's stdout; matches console=ttyAMA0
        #                                           so the METEOR/poll-loop banners reach the keyword)
        #     -serial chardev:seplossim -> ttyAMA1 (the Seplos socket bin-rpi's [serial] port targets)
        # This TRIPS the `serial_num >= 2` return, so there is NO second mon:stdio and thus NO
        # "chardev stdio can only be used by one device" clash. (Verified: QB_SERIAL_OPT is NOT
        # consulted on the nographic/serialstdio path — it's only read on the `serialconsole` branch —
        # so setting it would have been a no-op; the count-based short-circuit is the real lever.)
        qemuparams = (
            f"-chardev socket,id=seplossim,host=127.0.0.1,port={seplos_host_port},server=on,wait=off "
            f"-serial mon:stdio "
            f"-serial chardev:seplossim"
        )
        # QB_SLIRP_OPT REPLACES runqemu's default slirp -netdev (which forwards :2222->22 etc. + tftp);
        # we only need the guest MQTT broker (1883) forwarded to the host for the paho subscriber.
        qb_slirp_opt = f"-netdev user,id=net0,hostfwd=tcp:127.0.0.1:{mqtt_host_port}-:1883"

        # Run runqemu via `kas shell` (matches uat.yml's preflight step EXACTLY) as the `builder`
        # user (owns the kas/BitBake tree; BitBake-adjacent tooling refuses root). kas shell drops
        # us into the same native-sysroot-populated environment `kas build` used, so `runqemu` /
        # `qemu-system-aarch64` resolve on PATH without any manual env-setup sourcing.
        # Match uat.yml's WORKING preflight invocation EXACTLY, plus our qemuparams: `runqemu <machine>
        # ext4 nographic slirp snapshot bootparams=...`. Do NOT pass DEPLOY_DIR_IMAGE as an arg —
        # inside `kas shell` runqemu already gets it from the bitbake env, and passing it ALSO
        # positionally made runqemu abort ("Can't handle two unknown args: qemuarm64 DEPLOY_DIR_IMAGE=",
        # run 3). The reconstructed image is in that same default deploy dir. `snapshot` = COW overlay;
        # reads still see the persistent rootfs (with the injected [serial]=ttyAMA1). bootparams forward
        # the journal to the console so the Robot 'METEOR'/poll-started waits can match.
        inner_shell_cmd = (
            "set -e\n"
            f"export QB_SLIRP_OPT={_sh_quote(qb_slirp_opt)}\n"
            f"runqemu {machine} ext4 nographic slirp snapshot "
            f"qemuparams={_sh_quote(qemuparams)} "
            f"bootparams={_sh_quote('systemd.journald.forward_to_console=1 systemd.log_level=info')}"
        )
        cmd = [
            "runuser", "-u", builder_user, "--",
            "env", f"HOME=/home/{builder_user}", f"KAS_WORK_DIR={kas_work_dir}",
        ]
        if machine:
            cmd.append(f"KAS_MACHINE={machine}")
        cmd += ["kas", "shell", kas_config, "-c", inner_shell_cmd]
        # kas chdir()s into KAS_WORK_DIR itself (same as .ci/scripts/run-kas.sh); the kas_config
        # path is relative to repo_root, so run from there.
        self._repo_root = repo_root

        try:
            self.start_monotonic = time.monotonic()
            self.qemu_process = subprocess.Popen(
                cmd,
                cwd=repo_root,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                stdin=subprocess.DEVNULL,
                text=True,
                bufsize=1,
            )
        except Exception as e:
            raise RuntimeError(f"Failed to start runqemu: {e}")

        self._stop_reader = False
        self.reader_thread = threading.Thread(target=self._reader_thread_func, args=(self.qemu_process,), daemon=True)
        self.reader_thread.start()

        return {"mqtt_host_port": mqtt_host_port, "seplos_host_port": seplos_host_port}

    def _reader_thread_func(self, process):
        try:
            while not self._stop_reader and process.poll() is None:
                line = process.stdout.readline()
                if not line:
                    time.sleep(0.05)
                    continue
                line_str = line.rstrip('\n\r')
                with self.console_lock:
                    self.console_buffer.append(line_str)
                    self.console_text.append(line_str)
        except Exception:
            pass
        finally:
            if process.stdout:
                try:
                    remaining = process.stdout.read()
                    if remaining:
                        for line in remaining.split('\n'):
                            line = line.rstrip('\n\r')
                            if line:
                                with self.console_lock:
                                    self.console_buffer.append(line)
                                    self.console_text.append(line)
                except Exception:
                    pass

    def wait_for_console_line(self, substring, timeout=180):
        """
        Block until a captured console line contains `substring`. Returns the matching line.
        Raises TimeoutError (with the console captured so far) on timeout.
        """
        start_time = time.monotonic()
        last_check_count = 0
        while time.monotonic() - start_time < timeout:
            with self.console_lock:
                for line in self.console_buffer[last_check_count:]:
                    if substring in line:
                        return line
                last_check_count = len(self.console_buffer)
            time.sleep(0.2)
        with self.console_lock:
            console_so_far = "\n".join(self.console_text)
        raise TimeoutError(
            f"Timeout waiting for console line containing '{substring}' (waited {timeout}s)\n\n"
            f"Console output so far:\n{console_so_far}"
        )

    def qemu_start_time(self):
        """Host-monotonic timestamp recorded just before runqemu was launched."""
        return self.start_monotonic

    def console_output(self):
        with self.console_lock:
            return "\n".join(self.console_text)

    def save_console_dump(self, path):
        with open(path, 'w') as f:
            f.write(self.console_output())

    def stop_yocto_qemu(self):
        """
        Terminate the runqemu wrapper and any qemu-system-* child it spawned. `runqemu` forks
        helper processes (a runqemu-flock guard, qemu-system-aarch64 itself) that a plain
        terminate() of the wrapper's own PID does not always reap — best-effort `pkill` for
        `qemu-system` as a backstop, same discipline as `distro/qemu/yocto-e2e.sh`'s own cleanup.
        """
        self._stop_reader = True
        if self.qemu_process is not None:
            try:
                self.qemu_process.terminate()
                try:
                    self.qemu_process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.qemu_process.kill()
                    self.qemu_process.wait(timeout=5)
            except Exception:
                pass
        if self.reader_thread and self.reader_thread.is_alive():
            self.reader_thread.join(timeout=2)
        try:
            subprocess.run(["pkill", "-f", "qemu-system"], timeout=5, capture_output=True)
        except Exception:
            pass
        output = self.console_output()
        self.qemu_process = None
        self.reader_thread = None
        return output


def _sh_quote(s):
    """Minimal POSIX shell single-quote escaping (no external dep on shlex.quote's import cost)."""
    return "'" + s.replace("'", "'\\''") + "'"
