#!/usr/bin/env python3
"""MCU stack-frame-size audit + baseline/ratchet gate (CLAUDE.md "Size stacks/buffers by
MEASUREMENT, not guessing" + "Bounded Debugging").

Builds the `no_std` MCU crate set (the justfile `no_std_core` roster) with
`-Z emit-stack-sizes`, extracts every function's worst-case stack FRAME size (not the whole
call-stack depth — one function's own frame) from the emitted `.stack_sizes` ELF sections, and
gates on growth: a hot-path frame that quietly balloons is exactly the class of bug the Embedded
rules call out ("Size stacks/buffers by MEASUREMENT... growing a stack is the wrong lever - fix
the frame-size / data-model cause"). This script gives CI a MEASURED oracle instead of a guess.

Mechanism (verified against this toolchain — do not "simplify" by guessing a different flag set):
  1. `RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Z emit-stack-sizes -C lto=off -C opt-level=z"
     cargo +<TOOLCHAIN> build --locked <no_std_core -p flags> --no-default-features
     --target thumbv7em-none-eabihf`
     (RUSTC_BOOTSTRAP=1 unlocks the nightly `-Z emit-stack-sizes` flag on the PINNED stable
     TOOLCHAIN — reproducible frames, no floating nightly; see TOOLCHAIN. LTO OFF: LTO strips
     `.stack_sizes` from the final link; the section only survives in the per-compilation-unit
     `.o` objects archived inside each `deps/*.rlib`.)
  2. For each `target/thumbv7em-none-eabihf/debug/deps/*.rlib`: list members (`ar t`), extract
     every `*.o` member (`ar p`), and run the pinned TOOLCHAIN's `llvm-readobj --stack-sizes` on
     it (located via `rustc +<TOOLCHAIN> --print sysroot` + the `host:` line of `rustc
     +<TOOLCHAIN> -vV` — NOT whatever `llvm-readobj` happens to be on PATH, which may be a
     different LLVM version with an incompatible object-file reader).
  3. Parse the `StackSizes [ Entry { Functions: [...] Size: 0x... } ... ]` blocks, take the max
     frame per symbol (a symbol can appear in more than one object under generics/inlining
     variance). Demangle with `rustfilt` if it's on PATH; otherwise leave symbols mangled (noted
     in the baseline header — comparisons still work, they're just less readable).

Modes:
  (default)          human report: every measured frame, largest first, ceiling breaches flagged.
  --check            gate: exit 1 if any symbol's CURRENT frame exceeds its BASELINE value by
                     more than --tolerance bytes (default 0 — shrinking is always fine), OR any
                     NEW symbol (no baseline entry) exceeds --ceiling bytes. Symbols in the
                     baseline that are no longer observed are reported as stale (warning, not a
                     failure — one less function is not a regression). Existing baselined frames
                     are grandfathered at their current size (the ceiling is a backstop for NEW
                     frames only, not a re-litigation of history).
  --write-baseline   overwrite the baseline with the current measured frame set.

Baseline ratchet, same philosophy as `.ci/scripts/typeimpl_audit.py`: it should be possible to
land this gate with today's frames already baselined, then shrink the baseline as hot-path frames
get tightened — the gate blocks NEW growth, it doesn't demand an instant rewrite.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BASELINE = REPO / ".ci" / "scripts" / "stack_sizes_baseline.txt"

MCU_TARGET = "thumbv7em-none-eabihf"

# Toolchain pin. `-Z emit-stack-sizes` is a nightly flag, but RUSTC_BOOTSTRAP=1 unlocks it on the
# PINNED stable toolchain the CI image already ships (sentient-builder is single-toolchain 1.91) —
# so frame sizes are REPRODUCIBLE (codegen is fixed) instead of drifting with a floating `nightly`
# that CI would re-install daily and that would spuriously trip this release-blocking gate. The
# committed baseline MUST be generated with this same toolchain (locally: `rustup toolchain install
# 1.91 && rustup component add llvm-tools --toolchain 1.91 && rustup target add
# thumbv7em-none-eabihf --toolchain 1.91`). On an MSRV bump, move this pin and re-bless the
# baseline. Override for a one-off via `SS_TOOLCHAIN=…`.
TOOLCHAIN = os.environ.get("SS_TOOLCHAIN", "1.91")

# Frame-audit SCOPE: keep only symbols that reference one of OUR workspace crates — our own
# functions, plus generic/third-party monomorphizations over OUR types (e.g.
# `heapless::Vec<our DTC>::clone`, whose frame reflects our data model, not heapless'). Pure
# core/alloc/std and third-party-over-primitive frames are dropped: they're not ours to fix and,
# being toolchain internals, are the churniest across compiler bumps. This keeps the baseline
# focused on frames we author + can act on. Tokens are matched as a substring of the demangled name.
WORKSPACE_CRATE_TOKENS = (
    "lib_parse", "lib_canonical", "lib_store", "lib_tms", "lib_mapping", "lib_io",
    "spec_app", "impl_app", "lib_schema",
)

# Mirrors the justfile `no_std_core` roster exactly (see justfile `build-no-std` / comment there
# for why each crate is in this set) — keep these two lists in sync by hand; there is no single
# source of truth to derive this from without adding a build-time dependency on `just` itself.
NO_STD_CORE_CRATES = [
    "lib-parse", "lib-canonical", "lib-store", "lib-tms", "lib-mapping", "lib-io",
    "spec-app", "impl-app",
]

# Hard per-function ceiling (bytes), applied ONLY to NEW symbols (those with no baseline entry) —
# a backstop so a brand-new function can't land with a wild frame the ratchet has nothing to
# compare against. Symbols already in the baseline are governed purely by the growth ratchet
# (grandfathered at their current size, however large — e.g. the ~15 KB `to_active_diagnostics`
# frame), so this ceiling never re-litigates existing frames. Deliberately generous; override with
# --ceiling. Don't raise it just to silence a failure — a new frame this big deserves scrutiny.
DEFAULT_CEILING = 8192
DEFAULT_TOLERANCE = 0

ENTRY_RE = re.compile(r"Functions:\s*\[([^\]]*)\]\s*Size:\s*(0x[0-9A-Fa-f]+)")


def _run(cmd, **kw):
    """subprocess.run wrapper that fails with a clear message instead of a raw traceback when the
    tool itself is missing (nightly not installed, binutils `ar` absent, etc.)."""
    try:
        return subprocess.run(cmd, **kw)
    except FileNotFoundError as e:
        sys.exit(f"stack_sizes_audit: required tool not found: {cmd[0]} ({e})")


def crate_flags(crates):
    out = []
    for c in crates:
        out += ["-p", c]
    return out


def run_cargo_build(repo):
    env = os.environ.copy()
    env["RUSTFLAGS"] = "-Z emit-stack-sizes -C lto=off -C opt-level=z"
    # RUSTC_BOOTSTRAP=1 unlocks the nightly `-Z emit-stack-sizes` flag on the PINNED stable
    # TOOLCHAIN (see its comment) — reproducible frames, no floating-nightly churn.
    env["RUSTC_BOOTSTRAP"] = "1"
    cmd = (["cargo", f"+{TOOLCHAIN}", "build", "--locked"] + crate_flags(NO_STD_CORE_CRATES)
           + ["--no-default-features", "--target", MCU_TARGET])
    proc = _run(cmd, cwd=repo, env=env)
    if proc.returncode != 0:
        sys.exit(f"stack_sizes_audit: cargo build failed (exit {proc.returncode}) — see output above.")


def find_readobj():
    """Locate llvm-readobj from the pinned TOOLCHAIN's own sysroot (not PATH) so the object-file
    reader always matches the compiler that emitted the objects."""
    sysroot = _run(["rustc", f"+{TOOLCHAIN}", "--print", "sysroot"], capture_output=True, text=True)
    if sysroot.returncode != 0:
        sys.exit(f"stack_sizes_audit: `rustc +{TOOLCHAIN} --print sysroot` failed — is the {TOOLCHAIN} "
                  f"toolchain installed? (`rustup toolchain install {TOOLCHAIN}`)")
    sysroot_path = sysroot.stdout.strip()

    vv = _run(["rustc", f"+{TOOLCHAIN}", "-vV"], capture_output=True, text=True)
    if vv.returncode != 0:
        sys.exit(f"stack_sizes_audit: `rustc +{TOOLCHAIN} -vV` failed")
    host = None
    for line in vv.stdout.splitlines():
        if line.startswith("host:"):
            host = line.split(":", 1)[1].strip()
            break
    if not host:
        sys.exit(f"stack_sizes_audit: could not determine host triple from `rustc +{TOOLCHAIN} -vV`")

    readobj = os.path.join(sysroot_path, "lib", "rustlib", host, "bin", "llvm-readobj")
    if not os.path.exists(readobj):
        sys.exit(f"stack_sizes_audit: llvm-readobj not found at {readobj} — is the `llvm-tools` "
                  f"component installed for {TOOLCHAIN}? (`rustup component add llvm-tools "
                  f"--toolchain {TOOLCHAIN}`)")
    return readobj


def find_rlibs(repo):
    deps_dir = repo / "target" / MCU_TARGET / "debug" / "deps"
    if not deps_dir.is_dir():
        sys.exit(f"stack_sizes_audit: {deps_dir} not found — did the build run/succeed?")
    return sorted(deps_dir.glob("*.rlib"))


def rlib_object_members(rlib):
    proc = _run(["ar", "t", str(rlib)], capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"stack_sizes_audit: `ar t {rlib}` failed: {proc.stderr.strip()}")
    return [line.strip() for line in proc.stdout.splitlines() if line.strip().endswith(".o")]


def parse_stack_sizes(text):
    """Parse llvm-readobj --stack-sizes output:
        StackSizes [
          Entry {
            Functions: [<symbol>]
            Size: 0x1020
          }
          ...
        ]
    Returns {symbol: max_frame_bytes} for this one object (a symbol can legitimately repeat
    within an object under weak/inline linkage variance; take the max)."""
    frames = {}
    for m in ENTRY_RE.finditer(text):
        size = int(m.group(2), 16)
        for sym in m.group(1).split(","):
            sym = sym.strip()
            if sym:
                frames[sym] = max(frames.get(sym, 0), size)
    return frames


def collect_all_frames(repo, readobj):
    """Build the no_std crate set (via run_cargo_build, called by the caller first) has already
    happened; this walks every rlib's object members and returns {symbol: max_frame_bytes} across
    the whole no_std_core crate set."""
    frames = {}
    rlibs = find_rlibs(repo)
    if not rlibs:
        sys.exit(f"stack_sizes_audit: no *.rlib found under target/{MCU_TARGET}/debug/deps — "
                  f"build produced nothing.")
    with tempfile.TemporaryDirectory(prefix="stack-sizes-") as td:
        for rlib in rlibs:
            for idx, member in enumerate(rlib_object_members(rlib)):
                obj_path = os.path.join(td, f"{rlib.stem}.{idx}.o")
                with open(obj_path, "wb") as out:
                    extract = _run(["ar", "p", str(rlib), member], stdout=out, stderr=subprocess.PIPE)
                if extract.returncode != 0:
                    continue  # unreadable member — skip, not fatal
                ro = _run([readobj, "--stack-sizes", obj_path], capture_output=True, text=True)
                if ro.returncode != 0 or not ro.stdout:
                    continue  # no .stack_sizes section (e.g. no functions with a nonzero frame)
                for sym, size in parse_stack_sizes(ro.stdout).items():
                    frames[sym] = max(frames.get(sym, 0), size)
    return frames


def demangle_frames(frames):
    """Demangle symbol names with `rustfilt` (batched via stdin) if it's on PATH. Returns
    (frames_by_display_name, demangled_ok). On any failure (tool absent, output-count mismatch)
    falls back to the mangled names untouched — comparisons against a baseline written the same
    way still work, they're just less readable."""
    import shutil
    rustfilt = shutil.which("rustfilt")
    if not rustfilt or not frames:
        return dict(frames), False

    symbols = list(frames.keys())
    proc = _run([rustfilt], input="\n".join(symbols), capture_output=True, text=True)
    demangled = proc.stdout.splitlines() if proc.returncode == 0 else []
    if len(demangled) != len(symbols):
        return dict(frames), False  # mismatch — bail safe rather than misattribute a size

    out = {}
    for sym, dem in zip(symbols, demangled):
        dem = dem.strip() or sym
        # Two distinct mangled symbols can render identically once demangled (e.g. generic
        # monomorphizations that differ only by a hash disambiguator) — keep the max, the same
        # "record max frame per symbol" rule used everywhere else in this file.
        out[dem] = max(out.get(dem, 0), frames[sym])
    return out, True


def in_scope(sym):
    """True if the (demangled) symbol references one of our workspace crates — see
    WORKSPACE_CRATE_TOKENS for why we scope to these."""
    return any(tok in sym for tok in WORKSPACE_CRATE_TOKENS)


def build_and_collect(repo):
    readobj = find_readobj()
    run_cargo_build(repo)
    raw = collect_all_frames(repo, readobj)
    frames, demangled_ok = demangle_frames(raw)
    frames = {sym: size for sym, size in frames.items() if in_scope(sym)}
    return frames, demangled_ok


def load_baseline(path):
    baseline = {}
    if not path.exists():
        return baseline
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.rstrip("\n")
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = line.split("\t", 1)
        if len(parts) != 2:
            continue
        bytes_str, sym = parts
        try:
            baseline[sym] = int(bytes_str)
        except ValueError:
            continue
    return baseline


def write_baseline(path, frames, demangled_ok):
    with open(path, "w", encoding="utf-8") as f:
        f.write("# MCU stack-frame-size baseline (-Z emit-stack-sizes; CLAUDE.md \"Size stacks/buffers "
                 "by MEASUREMENT, not guessing\").\n")
        f.write("# Regenerate: python3 .ci/scripts/stack_sizes_audit.py --write-baseline\n")
        f.write("# Format: <max-frame-bytes>\\t<symbol>  (one entry per function; shrinking is always\n")
        f.write("# tolerated by --check, growth beyond --tolerance bytes fails).\n")
        if not demangled_ok:
            f.write("# NOTE: rustfilt was not on PATH when this was generated — symbols below are "
                     "left mangled.\n")
        for sym in sorted(frames):
            f.write(f"{frames[sym]}\t{sym}\n")


def cmd_write_baseline(repo):
    frames, demangled_ok = build_and_collect(repo)
    write_baseline(BASELINE, frames, demangled_ok)
    print(f"wrote {len(frames)} frame entries to {BASELINE}"
          + ("" if demangled_ok else " (rustfilt not found — symbols left mangled)"))
    return 0


def cmd_check(repo, ceiling, tolerance):
    frames, demangled_ok = build_and_collect(repo)
    baseline = load_baseline(BASELINE)

    regressions = []  # (sym, cur, base)
    for sym, cur_size in frames.items():
        base_size = baseline.get(sym)
        if base_size is not None and cur_size > base_size + tolerance:
            regressions.append((sym, cur_size, base_size))

    # Ceiling applies to NEW symbols only (no baseline entry). Baselined symbols are governed by
    # the growth ratchet above and grandfathered at their current size, however large.
    ceiling_violations = [(sym, size) for sym, size in frames.items()
                          if sym not in baseline and size > ceiling]

    stale = sorted(set(baseline) - set(frames))
    for sym in stale:
        print(f"note: {sym!r} is now absent (stale) — drop it from {BASELINE}")
    if not demangled_ok:
        print("note: rustfilt not found on PATH — symbols compared in mangled form.")

    fail = False
    if regressions:
        fail = True
        print(f"\nSTACK-SIZE AUDIT FAILED: {len(regressions)} symbol(s) grew beyond baseline "
              f"(tolerance={tolerance} byte(s)):")
        for sym, cur_size, base_size in sorted(regressions, key=lambda r: -(r[1] - r[2])):
            print(f"  {sym}: {base_size} -> {cur_size}  (+{cur_size - base_size})")
    if ceiling_violations:
        fail = True
        print(f"\nSTACK-SIZE AUDIT FAILED: {len(ceiling_violations)} NEW symbol(s) exceed the "
              f"{ceiling}-byte per-function ceiling (baselined frames are grandfathered):")
        for sym, size in sorted(ceiling_violations, key=lambda r: -r[1]):
            print(f"  {sym}: {size} bytes")

    if fail:
        print("\nFix: reduce the frame — avoid large stack locals on the hot path (move to "
              "static/reused storage), split divergent-cost match arms into #[inline(never)] fns "
              "(CLAUDE.md \"Zero-copy / stack discipline\"). If the growth is deliberate and "
              "measured-safe, regenerate the baseline: "
              "`python3 .ci/scripts/stack_sizes_audit.py --write-baseline`.")
        return 1

    print(f"stack-size audit OK — {len(frames)} symbol(s) measured, none regressed beyond baseline "
          f"(tolerance={tolerance} byte(s)), none exceed the {ceiling}-byte ceiling "
          f"({len(stale)} stale baseline entry/entries).")
    return 0


def cmd_report(repo, ceiling):
    frames, demangled_ok = build_and_collect(repo)
    rows = sorted(frames.items(), key=lambda kv: -kv[1])
    print(f"MCU stack-frame sizes ({len(rows)} symbol(s), target={MCU_TARGET}):")
    for sym, size in rows:
        flag = "  [OVER CEILING]" if size > ceiling else ""
        print(f"  {size:6d}  {sym}{flag}")
    over = sum(1 for _, size in rows if size > ceiling)
    print(f"\nSUMMARY: {len(rows)} symbol(s), largest={rows[0][1] if rows else 0} bytes, "
          f"{over} over the {ceiling}-byte ceiling.")
    if not demangled_ok:
        print("note: rustfilt not found on PATH — symbols left mangled.")
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true",
                     help="gate: exit 1 on a NEW/worse regression vs baseline or a ceiling breach")
    ap.add_argument("--write-baseline", action="store_true",
                     help="overwrite the baseline with the current measured frame set")
    ap.add_argument("--ceiling", type=int, default=DEFAULT_CEILING,
                     help=f"hard per-function frame ceiling in bytes, regardless of baseline "
                          f"(default {DEFAULT_CEILING})")
    ap.add_argument("--tolerance", type=int, default=DEFAULT_TOLERANCE,
                     help=f"bytes of slack allowed over baseline before --check fails "
                          f"(default {DEFAULT_TOLERANCE})")
    args = ap.parse_args()

    if args.write_baseline:
        return cmd_write_baseline(REPO)
    if args.check:
        return cmd_check(REPO, args.ceiling, args.tolerance)
    return cmd_report(REPO, args.ceiling)


if __name__ == "__main__":
    sys.exit(main())
