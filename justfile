# sentient-rs task runner — single source of truth for build/test/lint/security/release.
#
# Both developers and CI call these recipes, so the pipeline can never drift from what runs locally.
# Dev path: WSL/Linux (`just test`). CI path: rust:latest containers call the same recipes.
#
# Install just: `cargo install just --locked`  OR  `curl -sSf https://just.systems/install.sh | bash`
# List recipes: `just --list`

set shell := ["bash", "-euo", "pipefail", "-c"]

# ---- Config -----------------------------------------------------------------------------------
mcu_target       := "thumbv7em-none-eabihf"          # Cortex-M4F reference MCU
mcu_target_v8m   := "thumbv8m.main-none-eabihf"       # RP2350A = Cortex-M33 (crates/bin/mcu-rp2350, EPIC #350)
# ---- HIL bench (RP2350 over SWD) — see the `MCU HIL bench` recipe group below ------------------
# The bench Pi (its own SWD wires to two Pico 2s) is the access rail; openocd runs ON the bench, the
# defmt decode runs locally. Two Picos: the DEVICE (bin/mcu-rp2350 + W5500, hosts the broker) and the
# Seplos SIMULATOR (hil/pico-seplos-emu). SWD pin roles are the owner's wiring (2026-07-23).
bench_host       := "john@sentient-pi-bench"
device_swclk     := "25"                              # DEVICE (Feather RP2350 + W5500): SWCLK=GPIO25, SWDIO=GPIO24 (verified DPIDR 0x4c013477)
device_swdio     := "24"
sim_swclk        := "21"                              # SIMULATOR (Pico2 W, Seplos sim): SWCLK=GPIO21, SWDIO=GPIO20
sim_swdio        := "20"
rtt_port         := "9090"                            # openocd RTT TCP server (bench), tunneled to localhost
device_hostname  := "meteor-pico"                     # DHCP hostname (Option 12) baked into the DEVICE image at flash time
rp2350_ns_elf    := "crates/bin/mcu-rp2350/target/thumbv8m.main-none-eabihf/release/ns"
seplos_sim_elf   := "hil/pico-seplos-emu/target/thumbv8m.main-none-eabihf/release/pico-seplos-emu"
msrv             := "1.91"                            # matches [workspace.package] rust-version (smoltcp 0.13 needs 1.91)
# no_std core crates (built for the MCU with --no-default-features). lib-tms's core (the generated
# MIL-STD-3071 types + typedefs + TmsTopicId) is ROM-only no_std/no-alloc; its `TmsMessage` decode
# factory is host-only (`std` feature) and NOT built here. spec-app (ADR 0009): the app contract —
# the Platform port + RunConfig/Health + preflight — unconditionally no_std, proven here alongside
# impl-app (the policy crate that re-exports it) and bin-mcu-platform (the Embassy MCU platform
# impl, EPIC #350, replacing the retired bin-zephyr-platform).
no_std_core      := "-p lib-parse -p lib-canonical -p lib-store -p lib-tms -p lib-mapping -p lib-io -p lib-profile -p spec-app -p impl-app -p bin-mcu-platform"
# owned net path also builds no_std for the MCU: lib-net's channel + smoltcp-folded link + the M1
# embassy-net spike (adapter/config core, never its `embassy-host` feature), enabled via
# `--features lib-net/link,lib-net/embassy` in build-no-std below.
no_std_net       := "-p lib-net"

# Default: show the recipe list.
default:
    @just --list

# ---- Stage 1: format + lint (the CI gate) -----------------------------------------------------

fmt:
    cargo fmt --all --check

# Auto-format in place (dev convenience; CI uses `fmt`).
fmt-fix:
    cargo fmt --all

clippy:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Type/impl file-separation smell test (CLAUDE.md "Code Organization"). Baseline/ratchet gate:
# fails only on NEW violations vs .ci/scripts/typeimpl_baseline.txt — existing ones are worked down by
# the reorg, shrinking the baseline. Regenerate after splitting files: `just smell-baseline`.
smell:
    python3 .ci/scripts/typeimpl_audit.py --check

# Regenerate the smell-test baseline (drops now-clean entries after a split lands).
smell-baseline:
    python3 .ci/scripts/typeimpl_audit.py --write-baseline

# Crate-graph architecture audit: roster completeness (SRP forcing function against one-off crates)
# + no orphan lib islands. Dependency-DIRECTION / layering is enforced separately by cargo-deny
# `[bans]` + `wrappers` (deny.toml, supply-chain gate) — a maintained tool, not hand-rolled here.
crate-graph:
    python3 .ci/scripts/crate_graph_audit.py --check

# Regenerate the committed workspace crate DAG (docs/architecture/crates.{dot,svg}); needs graphviz.
depgraph:
    python3 .ci/scripts/crate_graph_audit.py --dot > docs/architecture/crates.dot
    dot -Tsvg docs/architecture/crates.dot -o docs/architecture/crates.svg
    # Layer-level Mermaid view (clean stack, one node per layer) — embedded in README.md.
    python3 .ci/scripts/crate_graph_audit.py --mermaid > docs/architecture/crates.mmd

# Drift check: this recipe's own `sec-matrix` tier list vs `.github/workflows/_security.yml`'s
# `TIERS` bash array — the same 9 security tiers, declared twice (the workflow can't just call
# `just sec-matrix` and keep its all-failures-visible reporting; see that file's own comment), with
# nothing else keeping them in sync. Fails loudly on drift instead of silently losing CI coverage.
sec-matrix-drift:
    python3 .ci/scripts/sec_matrix_audit.py --check

# Full stage-1 gate.
lint: fmt clippy smell crate-graph tms-codegen-check sec-matrix-drift

# ---- TMS IDL codegen (host-only tool, EXCLUDED from the firmware workspace) --------------------
#
# `tools/tms-idl-codegen` parses `docs/compliance/tms/mil-std-3071_data_model.idl` (nom) into TWO
# `@generated` files (dual-target, the split that dissolves the layering cycle):
# `crates/lib-tms/src/generated.rs` (the pure TMS model — types + IDL typedefs + TmsTopic/TmsTopicId
# + the TmsMessage factory) and `crates/lib-mapping/src/wire/generated_encoders.rs` (the bridge — the
# flat WireFlat canonical->wire encoders + FAULT_TABLE) + a coverage report (quote/prettyplease).
# It is its own workspace, so nom/quote never enter the firmware graph / cargo-vet surface. The
# byte-identity proofs live in-workspace: the wire encoders' at `crates/lib-mapping/tests`, the
# generated types' frozen-spelling proof at `crates/lib-tms/tests`.

# Regenerate the concrete type catalogue + coverage report from the IDL.
tms-codegen:
    cargo run -q --manifest-path tools/tms-idl-codegen/Cargo.toml -- emit

# Fail if the committed generated file is stale vs the IDL.
tms-codegen-check:
    cargo run -q --manifest-path tools/tms-idl-codegen/Cargo.toml -- check

# Build + test the codegen tool (parser + resolver unit tests), out of workspace. The generated
# catalogue's byte-identity proofs are in-workspace (`just test` runs them).
tms-codegen-test:
    cargo test --manifest-path tools/tms-idl-codegen/Cargo.toml

# ---- Stage 2: tests + no_std builds -----------------------------------------------------------

# Workspace test suite, minus two crates that would otherwise double-run (each also has its own
# dedicated CI lane):
#   - test-e2e: the real-pty/real-broker cross-crate suite, run nextest-serialized via
#     `just test-e2e` — a second pass here would rerun the same pty/socket tests outside that
#     serialization, exactly the contention `.config/nextest.toml`'s `seplos-e2e` group exists to
#     prevent.
#   - test-acceptance: `harness = false` on its `cucumber` test binary (see
#     crates/test-acceptance/Cargo.toml's `[[test]]`) only opts it out of the libtest harness, NOT
#     out of `cargo test`/nextest — there is no `test = false` — so an unscoped run would silently
#     re-execute the whole Gherkin suite alongside the dedicated `just test-acceptance` lane.
# `cargo nextest run`, NOT plain `cargo test` (verify-at-build, story "test speed" investigation):
# measured on this workspace (~98 test binaries across ~32 crates), nextest's cross-BINARY
# parallelism beats plain `cargo test`'s sequential-per-binary execution by ~30-45% wall-clock even
# fully warm (e.g. 21-30s vs 30-40s for this exact scope on a 32-core box) — most individual suites
# here are sub-second, so the win is from overlapping ~98 process launches, not from any single
# slow suite. `.config/nextest.toml`'s `[profile.default]` (45s/3 slow-timeout, JUnit) applies here
# too; its `lib-io::host_timeout`/`lib-mqtt::broker_health` overrides were ALREADY written
# anticipating this ("remain in this group for correctness under a broader `cargo nextest run
# --workspace`" — see that file) — this recipe is what finally exercises that. `--all-targets`
# mirrors the old `cargo test` target selection (lib/bins/tests; no benches/examples here); nextest
# cannot run doctests regardless of this flag — `just test-doc` is the ONLY place they run (this
# was already true before this change, not new).
# Second line: lib-net's `packed` feature (the postcard/serde IPC codec for the boundary messages)
# is non-default and default-feature workspace test runs never build it, so its round-trip/codec
# tests (see lib-net's `#[cfg(feature = "packed")]` tests) would otherwise never compile or run
# anywhere in CI.
#
# TRAP: plain `cargo test --workspace` (no excludes) is NOT just slower here — it can be far worse
# than the raw "wrong lane" cost above suggests. Measured directly (this same checkout, otherwise
# fully warm): `cargo test --workspace --locked --all-targets` ran 6m57s and then STOPPED on
# `dds_interop_e2e`'s documented ~30% flake (`.config/nextest.toml`'s own comment) — plain
# `cargo test` fail-fasts on the first failing binary and has none of nextest's `retries = 3`, so
# one known-flaky e2e test aborts the entire run before test-acceptance, doctests, or the rest of
# test-e2e ever get a turn. Separately, four test-e2e files (capture_e2e, dds_interop_e2e,
# sigterm_drain_e2e, spec_conformance_e2e — grep `ensure_bin_rpi_built`) each shell out to their own
# `cargo build -p bin-rpi` from `std::env::temp_dir()` (deliberate — see their own doc comments: it
# dodges this workspace's `.cargo/config.toml` kellnr registry replacement for sandboxes where that
# mirror isn't reachable). On a normal local machine where the kellnr mirror IS reachable but only
# configured at the project level (not baked into `$CARGO_HOME/config.toml` the way
# `.ci/Dockerfile` does for CI's containers), that cwd-outside-the-tree subprocess resolves a
# DIFFERENT Cargo source than the rest of the build, so it is NOT the "no-op rebuild-check" those
# tests' doc comments describe — it force-recompiles the entire dependency graph from scratch, once
# per call site, unprotected by CI's sccache. This is why the previously-measured 1082-1390s for
# `cargo test --workspace` varies run to run: where the flake/recompile lands differs each time. If
# you want these 4 tests' internal builds to stay warm locally too, mirror
# `.cargo/config.toml` into your own `$CARGO_HOME/config.toml` (exactly what `.ci/Dockerfile` does)
# — optional, and does not change behavior in a sandbox that can't reach the mirror in the first
# place, so it doesn't undo what those tests' cwd choice is FOR.
# COMPILE-ONLY warm-up for the shared sccache. ci.yml fans out `test`/`security`/`quality`/
# `dds-interop`/`supply-chain` in PARALLEL, all gated on `needs: [lint]`, so on a cold or
# dependency-changed cache those five lanes each compile the SAME graph at the same moment —
# concurrent identical compiles all miss (nothing has been written yet) and every lane pays the
# full cost. Running this once in the `lint` lane, which they all wait on anyway, converts that
# into one compile plus four sets of cache hits.
#
# The flag list MUST stay byte-identical to `test`'s first line below (plus `--no-run`): sccache
# keys on the exact rustc invocation, so a single differing flag yields different cache keys and
# this becomes pure wasted work rather than a warm-up. Change one, change both.
warm-cache:
    cargo nextest run --workspace --locked --all-targets --exclude test-e2e --exclude test-acceptance --no-run

test:
    cargo nextest run --workspace --locked --all-targets --exclude test-e2e --exclude test-acceptance
    cargo nextest run -p lib-net --locked --all-targets --features packed
    # lib-tms's `std` feature enables the host-only serde `TmsMessage::{decode,encode}` factory
    # (serde_json/alloc). The default-feature workspace run above never builds it, so its factory
    # decode/encode round-trip tests (gated `#[cfg(feature = "std")]`) would otherwise never run.
    cargo nextest run -p lib-tms --locked --all-targets --features std

# Human-readable cucumber-rs (Gherkin) spec-alignment suite: Seplos -> Canonical -> TMS field
# correctness, in-process, reusing the real test-kit/lib-parse/lib-mapping code.
# `harness = false` (see test-acceptance/Cargo.toml), so it is NOT nextest-runnable and is
# deliberately its own recipe/CI step rather than folded into `test` or `test-e2e`.
test-acceptance:
    cargo test -p test-acceptance --locked --test cucumber

# Integration/e2e lane: the cross-crate e2e tests (moved into test-e2e in #99 — real pty,
# real broker), run under nextest for process-per-test isolation. `.config/nextest.toml` puts every
# pty/broker test — across test-e2e AND the crate-local tests that stayed in lib-io/
# lib-mqtt — into one `seplos-e2e` group serialized against itself, so they never contend for
# a pty or a listening socket at the same time. Requires `cargo install cargo-nextest --locked`.
test-e2e:
    cargo nextest run -p test-e2e --locked -E 'not binary(dds_interop_e2e)'

# Host L2 driver (pnet/AF_PACKET) + folded smoltcp stack — lib-net's `link-host` (pnet) + `tls`
# (both imply `link` + `std`), the former lib-link `--features host` (default was std+tls) surface.
test-host-link:
    cargo test -p lib-net --features link-host,tls

# Stacked-Borrows oracle for the firmware broker-pump static-mut aliasing pattern (see
# crates/lib-net/tests/broker_pump_aliasing_miri.rs's doc comment for the exact shape it
# reproduces); nightly-only. `--features link` compiles lib-net's `link` module (BrokerCore).
# Not part of `ci` — weekly in scheduled.yml, on demand here.
miri:
    rustup toolchain install nightly --component miri
    cargo +nightly miri test -p lib-net --locked --features link --test broker_pump_aliasing_miri

# no_std core + owned net path build for the MCU (proves the embedded contract compiles).
build-no-std:
    rustup target add {{mcu_target}}
    cargo build --locked {{no_std_core}} --no-default-features --target {{mcu_target}}
    # lib-net's owned-net MCU modules (channel + link + the embassy-net spike).
    # `--features lib-net/link,lib-net/embassy` links the (dependent-less) embassy spike so the
    # MCU contract stays exercised.
    cargo build --locked {{no_std_net}} --no-default-features --features lib-net/link,lib-net/embassy --target {{mcu_target}}
    # lib-net's `packed` (postcard/serde) IPC codec (former lib-channel `packed`) is no_std-clean on
    # the MCU target too — verified locally. Built as its own line (not folded into `no_std_net`
    # above) since it's a non-default feature of the codec, not the always-on owned-net path.
    cargo build --locked -p lib-net --no-default-features --features packed --target {{mcu_target}}
    # Same no_std_core roster (now including bin-mcu-platform, the Embassy MCU platform impl — EPIC
    # #350) for the REAL MCU target (RP2350A / Cortex-M33), not just the thumbv7em reference chip
    # above — proves bin-mcu-platform's embassy-version-free generics actually build for the chip
    # bin/mcu-rp2350 (its own excluded workspace) instantiates them on.
    rustup target add {{mcu_target_v8m}}
    cargo build --locked {{no_std_core}} --no-default-features --target {{mcu_target_v8m}}

# Build the RP2350 (Cortex-M33) MCU shipping binary (EPIC #350; S1 #352 -> S4 #355 S/NS split) —
# its own excluded workspace (bin/mcu-rp2350), own Cargo.lock, own embassy 0.10-era pins.
# `.cargo/config.toml` there pins the target (thumbv8m.main-none-eabihf), so no `--target` flag is
# needed on any line below.
#
# Order matters: `abi` (the frozen NS<->S veneer ABI — error codes, custody codes, the
# `sv_err_to_security_error` mapping) has no in-workspace deps, so it builds first; `s` (secure
# world) depends on `abi` and links the CMSE veneers (`sv_session_establish`/`sv_seal`/`sv_open`/
# `sv_profile`), so it builds second. `ns` (non-secure world) builds five ways: its plain default
# feature set (the Stage-1 shape, no S-world dependency at all — still the only member that MUST
# build clean with zero TrustZone toggles, and — story S8 #359 — the one that also brings up the
# W5500/embassy-net/DHCPv4 stack unconditionally, see `crates/bin/mcu-rp2350/ns/src/main.rs`); `--features
# defmt` (RTT-only logging, incl. the #359 net-lifecycle lines — still no S-world dependency);
# `--features "defmt,core1-parser"` together (both are independent, orthogonal toggles — this is
# the combination #359's gate requires, not just each alone); `--features core1-parser` alone as a
# compile-only proof (no artifact dependency, so always run); and `--features veneers`, which links
# against the import lib `s`'s build emits at `crates/bin/mcu-rp2350/target/veneer-implib/` via
# `SENTIENT_VENEER_IMPLIB` — guarded below so this recipe stays green before that artifact exists
# (e.g. mid-integration, or if `s` hasn't built the veneer-exporting profile yet).
#
# NOTE: the package names are `rp2350-abi`/`rp2350-s` (see each crate's own `Cargo.toml`), NOT the
# `abi`/`s` directory names `-p` had been (wrongly) given here — `cargo build -p abi`/`-p s` do not
# match any package in this workspace and fail outright; fixed below alongside this story's own
# additions since it's the same recipe block. Also corrected: `SENTIENT_VENEER_IMPLIB` must name the
# artifact FILE (`ns/build.rs`'s own doc comment: `.../target/veneer-implib/libveneer_implib.o`),
# not just its parent directory — the directory-only value below previously fed straight to the
# linker as a bare path arg, which fails with "Is a directory" (never actually reached before this
# story's `-p abi`/`-p s` fix above, since those typos aborted the recipe on its very first line).
# Also corrected (delight-features WP6): the guard below checked `bin/mcu-rp2350/target/veneer-implib`
# — stale since #385 moved this crate from `bin/mcu-rp2350` to `crates/bin/mcu-rp2350`. `just` recipes run
# from the repo root, so that path never existed, `ls -A` was always empty, and the `veneers` build
# silently took the skip branch on every run — the lane was never actually exercised. Fixed to
# `crates/bin/mcu-rp2350/target/veneer-implib`, matching `s/build.rs`'s own
# `manifest_dir.join("..").join("target").join("veneer-implib")` (manifest_dir =
# `crates/bin/mcu-rp2350/s`). Same stale-prefix bug also lurked in the doc-only `bin/mcu-rp2350/target/...`
# example paths inside the `mcu-rp2350-uf2` and `mcu-rp2350-rtt` heredocs below — fixed there too.
mcu-rp2350:
    cd crates/bin/mcu-rp2350 && cargo build --release -p rp2350-abi
    cd crates/bin/mcu-rp2350 && cargo build --release -p rp2350-s
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features defmt
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features "defmt,core1-parser"
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features core1-parser
    # Delight-feature matrix (WP3/WP4/WP5): the shipping default leaves power/neopixel/ui/mesh OFF,
    # so nothing here gated them until now — a break in the INA3221 rail leg, NeoPixel status LED,
    # OLED render layer, or Meshtastic codec could merge unseen. Two "all" rows because `ui`
    # (display) and `mesh` are a MUTUAL-EXCLUSION `compile_error!` in src/main.rs (both want
    # GP24/GP25) — one build can't hold both, so the display-side and mesh-side "everything on"
    # images are exercised separately. Each also carries `defmt` (the RTT log sink these bench
    # features are observed through). Default features stay the deterministic published artifact —
    # _mcu-build.yml rebuilds plain `ns` after this recipe, so these extra images never leak into it.
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features "defmt,power,neopixel,ui"
    cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features "defmt,power,neopixel,mesh"
    if [ -n "$(ls -A crates/bin/mcu-rp2350/target/veneer-implib 2>/dev/null || true)" ]; then \
        cd crates/bin/mcu-rp2350 && SENTIENT_VENEER_IMPLIB="$(pwd)/target/veneer-implib/libveneer_implib.o" cargo build --release -p ns --features veneers; \
    else \
        echo "mcu-rp2350: skipping 'ns --features veneers' — crates/bin/mcu-rp2350/target/veneer-implib not produced yet (build 's' first, or this is a pre-veneer checkout)"; \
    fi

# Host-testable slice of the excluded crates/bin/mcu-rp2350 workspace (test speed/coverage
# investigation, story "run tests in parallel"): NOTHING runs `cargo test` in this workspace today
# — `_mcu-build.yml`'s `mcu` job (uat.yml/production.yml) only BUILDS it, and this workspace's own
# `.cargo/config.toml` pins `build.target = thumbv8m.main-none-eabihf` unconditionally, so a bare
# `cargo test` here fails outright (`can't find crate for test`, then "#[panic_handler] function
# required" — no std, no libtest harness on a bare-metal target). `rp2350-abi` (the frozen NS<->S
# ABI — error codes, custody codes; see its own Cargo.toml doc comment, which already calls it
# "host-testable") has ZERO embassy/cortex-m deps, so overriding just its target to the HOST triple
# works and its 5 unit tests pass (verified). `ns` (embassy-executor, inline `asm!("sev")`) does
# NOT: even `cargo test -p ns --target x86_64-unknown-linux-gnu` fails to compile — cortex-m asm has
# no x86_64 encoding — so its lib/ui-screen tests need real hardware/HIL or an emulator (out of
# scope here; see the `mcu-hil-*` recipes for the HIL path). This closes the `rp2350-abi` half of
# the gap; the `ns` half is honestly still open.
mcu-rp2350-test:
    cd crates/bin/mcu-rp2350 && cargo test -p rp2350-abi --locked --target x86_64-unknown-linux-gnu

# Documentation-only recipe: the manual steps to turn the built `s` + `ns` ELFs (see `mcu-rp2350`
# above) into a flashable, demo-signed UF2. This is NOT the deferred Gate-0 secure bootloader
# (#351) — there is no verified boot chain here, just picotool's IMAGE_DEF signing applied to a
# manually-combined image, for the IP-protection/load-time-integrity demo claim only (see the repo
# root CLAUDE.md "THIS IS A DEMO SCAFFOLD" banner). Never requires picotool to be installed to run
# `just --list` or CI — guarded with `command -v`.
mcu-rp2350-uf2:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v picotool >/dev/null 2>&1; then
        echo "mcu-rp2350-uf2: picotool not found on PATH — this recipe is documentation-only, printing the manual steps:"
    fi
    cat <<'STEPS'
    Manual UF2 packaging + demo signing (RP2350A, A4 stepping only) — NOT a verified/Gate-0 boot chain:

      1. Build both worlds (release profile, so IMAGE_DEF is emitted by embassy-rp's macro):
           just mcu-rp2350

      2. Combine the secure (`s`) and non-secure (`ns`) ELFs into one flashable image. picotool's
         `uf2 convert` / `partition` tooling expects the secure ELF to own the IMAGE_DEF metadata
         block; consult `picotool help uf2` for the current combine flags on your picotool version
         (RP2350 multi-image / TrustZone image layout is picotool-version-sensitive — verify
         against installed `picotool version`, don't assume flags across versions):
           picotool uf2 convert crates/bin/mcu-rp2350/target/thumbv8m.main-none-eabihf/release/s  \
               --other-elf crates/bin/mcu-rp2350/target/thumbv8m.main-none-eabihf/release/ns \
               -o crates/bin/mcu-rp2350/target/rp2350-demo.uf2

      3. Sign the IMAGE_DEF block with a demo secp256k1 keypair (picotool's block-signing path,
         NOT a fused/OTP secure-boot root of trust — this is a load-time-integrity /
         IP-protection demo signature only):
           picotool otp keygen --scheme secp256k1 -o demo-signing-key.pem   # ONE-TIME, dev only
           picotool sign crates/bin/mcu-rp2350/target/rp2350-demo.uf2 --key demo-signing-key.pem \
               -o crates/bin/mcu-rp2350/target/rp2350-demo.signed.uf2

      4. Flash via BOOTSEL mass-storage drag-drop, or `picotool load -f rp2350-demo.signed.uf2`.

      Explicitly NOT covered here (out of Stage-4 scope, tracked separately):
        - Gate-0 secure bootloader / fused root-of-trust verification chain (#351, deferred).
        - Chunked payloads over SV_MAX_PAYLOAD=4096 (TODO #355).
    STEPS

# Documentation-only recipe (story S8 #359): the manual bench flow to attach OpenOCD's RTT bridge
# over the SWD linuxgpiod adapter and stream defmt logs for `ns --features defmt` (link/DHCP
# lifecycle events — see `crates/bin/mcu-rp2350/ns/src/main.rs`'s module doc). The board has no console, so this
# IS the only way to observe it. Every OpenOCD/`nm` command below is cited against upstream docs,
# not guessed (see the inline citations) — verify against your installed `openocd --version` before
# relying on exact flag names, same posture as `mcu-rp2350-uf2`'s picotool caveat. Never requires
# openocd to be installed to run `just --list` or CI — guarded with `command -v`.
mcu-rp2350-rtt:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v openocd >/dev/null 2>&1; then
        echo "mcu-rp2350-rtt: openocd not found on PATH — this recipe is documentation-only, printing the manual steps:"
    fi
    cat <<'STEPS'
    Manual RTT attach flow for the bench Pico 2 ("RP2350 test bench") — NOT executed by this recipe:

      0. Build the defmt-enabled NS image first (produces the ELF `nm`/OpenOCD both need):
           just mcu-rp2350
         (or, minimally: `cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features defmt`)

      1. Find the `_SEGGER_RTT` control-block symbol's address in the built ELF — `rtt setup` below
         needs this exact address (OpenOCD CAN auto-search instead if you omit it, but a known
         address is faster/more reliable over a slow SWD link):
           arm-none-eabi-nm crates/bin/mcu-rp2350/target/thumbv8m.main-none-eabihf/release/ns | grep _SEGGER_RTT
         (e.g. `20010008 D _SEGGER_RTT` — the leading hex field is the address `rtt setup` wants.)

      2. Attach OpenOCD over the bench's linuxgpiod SWD adapter (swclk=GPIO11, swdio=GPIO10) and
         start the RTT bridge. Commands cited from the OpenOCD manual's "adapter gpio" (generic
         linuxgpiod GPIO mapping) and "RTT support" sections — `rtt setup <address> <size> [ID]`,
         `rtt start`, `rtt server start <port> <channel> [message]`
         (https://openocd.org/doc/html/index.html; confirm against `tcl/target/rp2350.cfg` shipped
         by your installed OpenOCD, which this repo has not modified):
           openocd \
             -c "adapter driver linuxgpiod" \
             -c "adapter gpio swclk -chip 0 11" \
             -c "adapter gpio swdio -chip 0 10" \
             -c "transport select swd" \
             -c "source [find target/rp2350.cfg]" \
             -c "init" \
             -c "reset halt" \
             -c "rtt setup 0x20010008 1024 \"SEGGER RTT\"" \
             -c "rtt start" \
             -c "rtt server start 9090 0" \
             -c "resume"
         (Replace `0x20010008` with whatever step 1's `nm` actually printed for THIS build — the
         address moves whenever the image's static layout changes.)

      3. In a second terminal, decode the RTT byte stream OpenOCD is now serving on TCP port 9090
         into readable defmt log lines, via `defmt-print`'s own documented `tcp` subcommand
         (https://github.com/knurling-rs/defmt — `print/README.md` + `print/src/main.rs`'s `Command`
         enum: `defmt-print -e <elf> tcp --host <host> --port <port>`):
           cargo install defmt-print   # one-time
           defmt-print -e crates/bin/mcu-rp2350/target/thumbv8m.main-none-eabihf/release/ns \
             tcp --host localhost --port 9090

         Expect to see, over the wire as the bench Pico 2's W5500 links and DHCPs: "w5500: link
         up", "dhcp: acquired <ip> gw <gw>", and (on cable pull / lease loss) "w5500: link down" /
         "dhcp: lost" — see `crates/bin/mcu-rp2350/ns/src/main.rs`'s module doc for exactly when each fires.
    STEPS

# ==== MCU HIL bench (RP2350 over SWD) — executable ==============================================
# EXECUTABLE counterparts to the documentation recipes above. These drive the bench Pi
# ({{bench_host}}) which holds the SWD wires to both Pico 2s (see the `bench_*`/`*_swclk` vars in
# Config). openocd runs ON the bench over ssh; the defmt decode runs locally. No pins/hosts to
# remember — they live in the Config block. All ssh; nothing here mutates the local tree.

# One-time host setup: install the local defmt decoder + verify the bench toolchain is reachable.
mcu-hil-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v defmt-print >/dev/null 2>&1 || { echo "installing defmt-print…"; cargo install defmt-print; }
    command -v arm-none-eabi-nm >/dev/null 2>&1 || echo "WARN: arm-none-eabi binutils not found locally (needed for ELF inspection)"
    echo "checking bench {{bench_host}}…"
    ssh -o ConnectTimeout=10 -o BatchMode=yes {{bench_host}} 'command -v openocd >/dev/null || { echo "ERROR: openocd missing on bench"; exit 1; }; echo "bench OK: $(hostname), openocd $(openocd --version 2>&1 | head -1 | cut -d" " -f1-4)"'
    echo "mcu-hil-setup: ready. Flash: 'just mcu-flash-device' / 'just mcu-flash-sim'. Logs: 'just mcu-rtt'."

# Flash the DEVICE (Pico2 + W5500, hosts the broker) with the defmt NS image over SWD (GPIO{{device_swclk}}/{{device_swdio}}).
# DEVICE_HOSTNAME is baked into the image at build time (DHCP Option 12); override: `just device_hostname=foo mcu-flash-device`.
mcu-flash-device:
    cd crates/bin/mcu-rp2350 && DEVICE_HOSTNAME="{{device_hostname}}" cargo build --release -p ns --features defmt
    scp -o BatchMode=yes {{rp2350_ns_elf}} {{bench_host}}:/home/john/ns-device.elf
    ssh -o BatchMode=yes {{bench_host}} 'openocd \
        -c "adapter driver linuxgpiod" \
        -c "adapter gpio swclk -chip 0 {{device_swclk}}" -c "adapter gpio swdio -chip 0 {{device_swdio}}" \
        -c "transport select swd" -c "source [find target/rp2350.cfg]" \
        -c "init" -c "reset halt" -c "program /home/john/ns-device.elf verify" -c "reset run" -c "shutdown"'
    @echo "device flashed. Watch its DHCP IP with: just mcu-rtt"

# Flash the SIMULATOR (Pico2 W, Seplos pack emulator) over SWD (GPIO{{sim_swclk}}/{{sim_swdio}}).
mcu-flash-sim:
    cd hil/pico-seplos-emu && cargo build --release
    scp -o BatchMode=yes {{seplos_sim_elf}} {{bench_host}}:/home/john/seplos-sim.elf
    ssh -o BatchMode=yes {{bench_host}} 'openocd \
        -c "adapter driver linuxgpiod" \
        -c "adapter gpio swclk -chip 0 {{sim_swclk}}" -c "adapter gpio swdio -chip 0 {{sim_swdio}}" \
        -c "transport select swd" -c "source [find target/rp2350.cfg]" \
        -c "init" -c "reset halt" -c "program /home/john/seplos-sim.elf verify" -c "reset run" -c "shutdown"'
    @echo "simulator flashed + running."

# Stream + decode the DEVICE's defmt/RTT log locally (its DHCP IP, broker-up, client events).
# openocd RTT server runs on the bench (searches RAM for the SEGGER RTT block — no fixed address);
# the port is tunneled to localhost and decoded with the local ELF. Ctrl-C stops; cleans up the bench.
mcu-rtt:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v defmt-print >/dev/null 2>&1 || { echo "run 'just mcu-hil-setup' first"; exit 1; }
    [ -f "{{rp2350_ns_elf}}" ] || { echo "no NS ELF — run 'just mcu-flash-device' first"; exit 1; }
    cleanup() { ssh -o BatchMode=yes {{bench_host}} 'pkill -f "rtt server" 2>/dev/null || true'; }
    trap cleanup EXIT
    echo "starting openocd RTT server on {{bench_host}} (device SWD GPIO{{device_swclk}}/{{device_swdio}})…"
    ssh -f -o ExitOnForwardFailure=yes -L {{rtt_port}}:localhost:{{rtt_port}} {{bench_host}} 'openocd \
        -c "adapter driver linuxgpiod" \
        -c "adapter gpio swclk -chip 0 {{device_swclk}}" -c "adapter gpio swdio -chip 0 {{device_swdio}}" \
        -c "transport select swd" -c "source [find target/rp2350.cfg]" \
        -c "init" -c "rtt setup 0x20000000 0x80000 \"SEGGER RTT\"" -c "rtt start" \
        -c "rtt server start {{rtt_port}} 0" -c "resume"'
    sleep 2
    echo "decoding defmt (Ctrl-C to stop) — expect 'dhcp: acquired <ip>' then 'broker up: <ip>:1883':"
    defmt-print -e "{{rp2350_ns_elf}}" tcp --host localhost --port {{rtt_port}}

# Probe an SWD target for liveness (default = the device; pass pins to override, e.g. `just mcu-swd-probe 21 20`).
mcu-swd-probe swclk=device_swclk swdio=device_swdio:
    ssh -o BatchMode=yes {{bench_host}} 'openocd \
        -c "adapter driver linuxgpiod" \
        -c "adapter gpio swclk -chip 0 {{swclk}}" -c "adapter gpio swdio -chip 0 {{swdio}}" \
        -c "transport select swd" -c "source [find target/rp2350.cfg]" \
        -c "init" -c "targets" -c "shutdown" 2>&1' | grep -E "DPIDR|Examination succeed|Error connecting" || echo "no target on GPIO{{swclk}}/{{swdio}}"

# Live High/Low monitor of bench GPIO lines while you probe power/reset/SWD by hand — no meter.
# STOP openocd first (it holds SWCLK/SWDIO; a busy line shows ERR). SPEC = OFFSET[:LABEL].
# Bring-up tell: with the Feather powered + debug JST-SH seated, SWDIO reads H (target pull-up);
# SWDIO stuck L with power on ⇒ the same fault mcu-swd-probe reports as "cannot read IDR".
# Override the pins, e.g.: `just mcu-gpio-watch pins="11:SWCLK 10:SWDIO 9:RUN"`.
mcu-gpio-watch pins="24:DEV_SWCLK 25:DEV_SWDIO 21:SIM_SWCLK 20:SIM_SWDIO":
    scp -o BatchMode=yes hil/gpio-watch.sh {{bench_host}}:/tmp/gpio-watch.sh
    ssh -t -o BatchMode=yes {{bench_host}} 'bash /tmp/gpio-watch.sh -c gpiochip0 -i 0.2 {{pins}}'

# ---- lib-security: FIPS × TrustZone × secure-element tier matrix --------------------------

# One tier: clippy + test under an explicit feature set (e.g. `just sec-tier fips,std`).
sec-tier features:
    cargo clippy --locked -p lib-security --all-targets --no-default-features \
        --features "{{features}}" -- -D warnings
    cargo test --locked -p lib-security --no-default-features --features "{{features}}"

# no_std core tier: build-only for the MCU (tests need std to run).
sec-no-std:
    rustup target add {{mcu_target}}
    cargo build --locked -p lib-security --no-default-features --target {{mcu_target}}

# All buildable security tiers (wolfcrypt excluded — needs system wolfSSL 5.9.0+, separate ledger).
# Mirrors `.github/workflows/_security.yml`'s `security-matrix` job EXACTLY (9 rows: the 2x2x2
# fips x trustzone x secure-element grid, all with `std`, + the no_std core build). Flagged by
# Stage-3 Agent A: this recipe previously only covered the original 4 tiers (std,software /
# fips,std / trustzone,std / fips,trustzone,std) — CI grew the other 4 (secure-element x trustzone
# x fips combinations) when `secure-element` custody landed, and this recipe drifted out of sync.
# Row order below matches the CI matrix's `include:` list top-to-bottom for easy diffing.
sec-matrix: (sec-tier "std,software") (sec-tier "fips,std") (sec-tier "trustzone,std") (sec-tier "fips,trustzone,std") (sec-tier "secure-element,std") (sec-tier "fips,secure-element,std") (sec-tier "secure-element,trustzone,std") (sec-tier "fips,secure-element,trustzone,std") sec-no-std

# ---- MSRV -------------------------------------------------------------------------------------

# Build against the declared MSRV (needs a {{msrv}} toolchain: `rustup toolchain install {{msrv}}`).
msrv:
    cargo +{{msrv}} build --locked --workspace

# ---- Supply chain (new — pipeline hardening) --------------------------------------------------

# Fail on actual vulnerabilities (unmaintained/yanked stay warnings — transitive, uncontrollable).
# Accepted-vuln ignores + rationale live in .cargo/audit.toml.
audit:
    cargo audit

deny:
    cargo deny check

vet:
    cargo vet --locked

# Report `unsafe` usage across the workspace + dependency tree (advisory — NOT a gate; cargo-geiger
# can be slow/noisy). Surfaces the unsafe surface to review. Install once:
# `cargo install cargo-geiger --locked`. See docs/rust-hardening.md.
geiger:
    cargo geiger --workspace --all-features

# Static analysis of the GitHub Actions workflows themselves. Gate at low+ severity (informational
# style nits don't fail CI); deliberately-accepted rules are ignored in .github/zizmor.yml.
zizmor:
    zizmor --min-severity=low .github/workflows/

# Full supply-chain gate (Stage 1 in CI).
supply-chain: audit deny vet

# ---- Security-gate evidence (machine-readable reports the pipeline UPLOADS) --------------------
#
# The PASS/FAIL gate stays `just supply-chain` (audit/deny/vet). These recipes ADDITIONALLY capture
# the same tools' findings as machine-readable artifacts the pipeline uploads alongside the SBOM +
# signatures — non-fatal by design (they record the posture for audit, they do NOT re-gate). Used by
# _supply-chain.yml (PR gate, `if: always()` so a red gate still uploads the report that explains it)
# and production.yml (release evidence bundle). See docs/rust-hardening.md.

# cargo-audit (RustSec advisory DB) + cargo-deny (advisories/bans/licenses/sources) as JSON into
# `outdir` (default target/security-reports). Never fails — the gate already ran; this is evidence.
security-report outdir="target/security-reports":
    mkdir -p {{outdir}}
    cargo audit --json > {{outdir}}/cargo-audit.json 2> {{outdir}}/cargo-audit.log || true
    cargo deny --format json check > {{outdir}}/cargo-deny.jsonl 2>&1 || true
    @echo "security-gate reports -> {{outdir}} (cargo-audit.json, cargo-deny.jsonl)"

# cargo-geiger unsafe-surface report (advisory) as JSON + GitHub-flavored markdown into `outdir`.
# cargo-geiger is NOT baked into sentient-builder — CI installs it per-run (like scheduled.yml's
# fuzz/miri); locally `cargo install cargo-geiger --locked`. Never fails (advisory, not a gate).
geiger-report outdir="target/security-reports":
    mkdir -p {{outdir}}
    cargo geiger --workspace --all-features --output-format Json > {{outdir}}/cargo-geiger.json 2> {{outdir}}/cargo-geiger.log || true
    cargo geiger --workspace --all-features --output-format GitHubMarkdown > {{outdir}}/cargo-geiger.md 2>> {{outdir}}/cargo-geiger.log || true
    @echo "unsafe-surface report -> {{outdir}} (cargo-geiger.json, cargo-geiger.md)"

# ---- Coverage / mutation / semver (new) -------------------------------------------------------

# Line+branch coverage on the std surface (tarpaulin can't measure no_std). Gate at 75%
# (canonical.rs/app/host.rs additions raised the baseline 66.56% -> 76.92%; ratchet up further as
# lib-io/host.rs's Rs485 path, lib-parse/seplos/error.rs + serial_number.rs, and the
# lib-net/lib-mqtt/lib-security remainders gain tests). test-e2e/test-kit
# are excluded like bin-rpi (#99) — test-support/e2e-only crates, not production logic to hold
# to a coverage bar. lib-net's embassy-net spike (`lib_net::embassy`) is only linkable with its
# `embassy-host` feature (embassy-time/std driver + critical-section/std impls); the workspace
# coverage build leaves that module off, so it needs no exclude — its real coverage comes from its
# host-feature integration tests, which tarpaulin can't build.
# vendor/dust_dds + vendor/dust_dds_derive are excluded: a `[patch.crates-io]` PATH override
# (see root Cargo.toml) that pulls the ~54k-line vendored fork into the same target/ build tarpaulin
# instruments; it's third-party code we carry for a single XCDR2 NEXTINT fix (PATCHES.md), not
# production logic of this workspace to hold to the 75% bar.
# generated_cdr.rs/generated_qos.rs (lib-tms) + generated_cdr_encoders.rs (lib-mapping) are the same
# @generated-class XCDR2 chain/QoS/CDR-encoder bindings as generated.rs/generated_fields.rs/
# generated_encoders.rs above — emitted by tools/tms-idl-codegen, verified by the byte-oracle tests
# and `just tms-codegen-check`'s drift gate, not hand-written logic to hold to the 75% bar.
coverage:
    cargo tarpaulin --workspace --exclude bin-rpi --exclude test-e2e --exclude test-kit \
        --exclude test-acceptance \
        --exclude-files 'crates/lib-tms/src/generated.rs' \
        --exclude-files 'crates/lib-tms/src/generated_fields.rs' \
        --exclude-files 'crates/lib-tms/src/generated_cdr.rs' \
        --exclude-files 'crates/lib-tms/src/generated_qos.rs' \
        --exclude-files 'crates/lib-mapping/src/wire/generated_encoders.rs' \
        --exclude-files 'crates/lib-mapping/src/wire/generated_cdr_encoders.rs' \
        --exclude-files 'vendor/*' \
        --features std --locked --out Html --out Xml --fail-under 75

# Mutation testing on the safety-critical crates (slow — CI runs this on a schedule).
mutants:
    cargo mutants --workspace -p lib-security -p lib-parse --locked

semver:
    cargo semver-checks

# ---- Fuzzing (new) ----------------------------------------------------------------------------

# Time-boxed fuzz of the Seplos frame parser (nightly + cargo-fuzz required). The fuzz/ crate lives
# under test-e2e/fuzz/ (#99 — grouped with the other test artifacts); cargo-fuzz auto-discovers
# it as a `fuzz` subdirectory of the current directory, so this recipe cd's there first.
fuzz target="fuzz_parse_telemetry" secs="300":
    cd crates/test-e2e && cargo +nightly fuzz run {{target}} -- -max_total_time={{secs}}

# ---- SBOM (new — release) ---------------------------------------------------------------------

# CycloneDX SBOM (JSON) for the whole workspace.
sbom:
    cargo cyclonedx -f json --all

# ---- Release build ----------------------------------------------------------------------------

# Optimized+symbols build of the RPi binary for one target (release.yml calls this per target).
build-release target:
    cargo build --locked --profile profiling --target {{target}} -p bin-rpi

# Auditable release build (embeds the dependency list into the binary for in-situ scanning).
build-auditable target:
    cargo auditable build --locked --profile profiling --target {{target}} -p bin-rpi

# ---- musl group: the fully-static aarch64 bin-rpi artifact Yocto packages -----------------------
#
# Cross-build bin-rpi as a FULLY-STATIC aarch64 musl binary — ABI-independent of whatever glibc the
# Yocto rootfs ships (our glibc release binary is pinned to Debian-bookworm 2.36; dropping that into a
# differently-glibc'd image can break at load — musl-static sidesteps it). Consumed by the
# sentient-agent recipe (distro/meta-sentient/.../sentient-agent_0.1.0.bb) + release.yml's musl job.
#
# These steps are the ones VERIFIED to work locally 2026-07-12 (no docker, no musl cross toolchain —
# just the aarch64 GNU cross-gcc already on the host / in .ci/Dockerfile):
#   - `ring` (pulled transitively by rustls) needs a C compiler for the target; the aarch64 GNU
#     cross-gcc compiles its (libc-agnostic) C objects fine — hence CC_/AR_ below.
#   - the static link is done by `rust-lld` + rustc's self-contained musl crt — no system musl-gcc.
#
# ASLR CAVEAT (recorded — see distro/HARDENING.md + issues #150/#154). ROOT CAUSE (verified via
# `rustc -Zunstable-options --print target-spec-json`, rustc 1.92): the `aarch64-unknown-linux-musl`
# target spec LACKS `static-position-independent-executables` (whereas `x86_64-unknown-linux-musl` has
# it) — so rustc CANNOT emit static-PIE for aarch64-musl. A plain build AND `-C relocation-model=pic`
# both yield `Type: EXEC` (static NON-PIE → no ASLR). This is a target CAPABILITY gap, not a missing
# flag — no RUSTFLAGS fixes it today. Restoring ASLR on aarch64 means either (a) a glibc-DYNAMIC build
# (PIE by default — but then build it against the IMAGE's own glibc, e.g. via the Yocto SDK, to avoid
# the version skew that motivated musl-static), or (b) upstream aarch64-musl static-PIE support landing.
# `verify-musl` enforces "fully static" (hard fail) + reports the missing ASLR as a TRACKED accepted
# risk (not silent). The relocation-model=pic flag is retained so the artifact flips to DYN
# automatically once/if rustc gains aarch64-musl static-PIE support.

# Build the static musl artifact.
[group('musl')]
build-musl target="aarch64-unknown-linux-musl":
    rustup target add {{target}}
    CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc \
    AR_aarch64_unknown_linux_musl=aarch64-linux-gnu-ar \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic -C link-self-contained=yes" \
    cargo auditable build --locked --profile profiling --target {{target}} -p bin-rpi

# ASLR + static gate for the musl artifact. "Fully static" is a HARD requirement (fails the build);
# static-PIE/ASLR is currently unmet on aarch64-musl (see the caveat above) and is surfaced as an
# accepted risk until #150/#154 resolve it (or a rustc bump makes it DYN — then this prints OK).
[group('musl')]
verify-musl target="aarch64-unknown-linux-musl":
    #!/usr/bin/env bash
    set -euo pipefail
    bin="target/{{target}}/profiling/bin-rpi"
    test -f "$bin" || { echo "FAIL: {{target}} bin-rpi not found (run build-musl first): $bin"; exit 1; }
    # Fully-static check (deterministic + cross-arch safe): a static ELF has no PT_INTERP and no
    # NEEDED shared-lib entries. `ldd` is unreliable on a cross-arch binary, so use readelf.
    if readelf -l "$bin" 2>/dev/null | grep -q 'INTERP' || readelf -d "$bin" 2>/dev/null | grep -q 'NEEDED'; then
      echo "FAIL: $bin is dynamically linked (has PT_INTERP/NEEDED) — must be fully static"; exit 1
    fi
    # ASLR check: static-PIE reports Type: DYN. Currently EXEC on aarch64-musl (see build-musl caveat).
    if readelf -h "$bin" | grep -qE 'Type:[[:space:]]*DYN'; then
      echo "OK: fully static + static-PIE (Type: DYN) — ASLR present"
    else
      echo "WARN: fully static but NON-PIE (Type: EXEC) — NO ASLR. Accepted risk, tracked in distro/HARDENING.md (#150/#154)."
    fi

# Build then gate the musl artifact.
[group('musl')]
musl: build-musl verify-musl

# ---- Bench + MCU stack-size gates (advisory) ---------------------------------------------------
#
# hotpath (crate `hotpath` v0.21, optional dep, Pi-only) instruments the bin-rpi `hotpath_e2e`
# example; `bench`/`bench-json` run it, `bench-check`/`bench-baseline` feed its JSON report through
# `.ci/scripts/bench_metrics_audit.py` — an ADVISORY trend gate (default exit 0; see that script's
# docstring for the --strict/--fail-ratio knobs). Set HOTPATH_ITERS to tune the example's iteration
# count (see crates/bin/rpi/examples/hotpath_e2e.rs); unset uses the example's own default.

# Human-readable hotpath table on stdout. HOTPATH_ITERS tunes iteration count.
[group('bench')]
bench:
    cargo run --release --manifest-path crates/bin/rpi/Cargo.toml --example hotpath_e2e \
        --features hotpath,hotpath-cpu,hotpath-alloc

# Same run, JSON report to target/hotpath/bench.json (consumed by bench-check/bench-baseline).
[group('bench')]
bench-json:
    mkdir -p target/hotpath
    HOTPATH_OUTPUT_FORMAT=json HOTPATH_OUTPUT_PATH=target/hotpath/bench.json \
        cargo run --release --manifest-path crates/bin/rpi/Cargo.toml --example hotpath_e2e \
        --features hotpath,hotpath-cpu,hotpath-alloc

# Advisory bench-metrics gate: run, then compare against .ci/scripts/bench_baseline.json (never
# fails the build here — pass --strict to bench_metrics_audit.py directly to gate on it).
[group('bench')]
bench-check: bench-json
    python3 .ci/scripts/bench_metrics_audit.py --check

# Regenerate the bench baseline from the current run.
[group('bench')]
bench-baseline: bench-json
    python3 .ci/scripts/bench_metrics_audit.py --write-baseline

# MCU stack-size gate (Track B): -Z emit-stack-sizes over the no_std core vs a committed baseline.
# Builds internally — see .ci/scripts/stack_sizes_audit.py.
[group('stack-sizes')]
stack-sizes:
    python3 .ci/scripts/stack_sizes_audit.py --check

# Regenerate the stack-size baseline from the current build.
[group('stack-sizes')]
stack-sizes-baseline:
    python3 .ci/scripts/stack_sizes_audit.py --write-baseline

# ---- Aggregate gates --------------------------------------------------------------------------

# Everything a PR must pass locally before pushing (mirrors the CI gate). `test` now excludes
# test-e2e/test-acceptance (see `test`'s own comment above), so both are listed explicitly here
# too — otherwise this local aggregate would silently lose the coverage the old single
# `cargo test --workspace` line used to provide.
ci: lint test test-e2e test-acceptance build-no-std sec-matrix

# Everything incl. the heavier hardening checks.
ci-full: ci supply-chain coverage semver

# ---- Docs -------------------------------------------------------------------------------------

# Regenerate a docs/*.md report as a .docx (for Google Drive / Word import). Native Word tables +
# heading styles. Defaults to the Scope-E security report (with its embedded trust-model diagram);
# pass a path to build any report, e.g. `just docs-docx docs/security/arm-trustzone-deep-dive.md`.
# Edit the .md, then run the recipe. The Scope-E diagram PNG is tracked at docs/assets/trust-model.png
# (no browser needed to rebuild the docx).
docs-docx src="docs/security/scope-e-tpm-signing-report.md":
    #!/usr/bin/env bash
    set -euo pipefail
    # `py -3` (Windows launcher) avoids the WindowsApps python stub; python3/python on Linux/macOS.
    if command -v py >/dev/null 2>&1; then PY="py -3"; \
    elif command -v python3 >/dev/null 2>&1; then PY="python3"; \
    else PY="python"; fi
    $PY -m pip install --quiet --disable-pip-version-check pypandoc-binary
    $PY .ci/scripts/build-report-docx.py "{{src}}"

# Workspace API docs (rustdoc). Two passes: the workspace with default features (host-green — NEVER
# --all-features, lib-net's embassy MCU deps break host builds), then lib-net again with its `host`
# umbrella feature so its published docs show the real RPi publish surface (mesh/locate modules)
# instead of the empty default-feature crate. The second pass overwrites lib-net's target/doc pages.
# target/doc is cleaned first: cargo never prunes it, so renamed/deleted crates' pages (e.g. the
# pre-#289 lib_net_embassy) would otherwise linger and get grafted into the site by `docs` below.
docs-api:
    rm -rf target/doc
    cargo doc --workspace --no-deps --locked
    cargo doc -p lib-net --no-deps --locked --features host

# Full documentation site (mdbook + rustdoc). Requires the pinned mdbook toolchain (see
# .github/workflows/docs.yml): `cargo binstall -y mdbook mdbook-mermaid`. The html backend is the
# ONLY mdbook backend (book.toml's linkcheck NOTE), so mdbook renders directly into build-dir —
# the site root is book/ (NOT book/html/). rustdoc is built separately (docs-api) and grafted
# under book/api/ after the mdbook build, matching docs/api-reference.md's raw-HTML links.
docs: docs-api
    mdbook build
    rm -rf book/api
    cp -r target/doc book/api
    @echo "docs site -> book/ (rustdoc grafted at book/api/)"

# Live-reload preview of the book while editing docs/ (no rustdoc graft — `just docs` for the
# full site including book/api/).
docs-serve:
    mdbook serve --open

# Doctest-only gate (wired into _test.yml alongside `test`): every rustdoc example in the
# workspace compiles and runs. `--doc` runs JUST the doctests — a fast, focused lane that names
# a doc-example regression directly instead of burying it in the full workspace test run.
# This is now the ONLY place doctests run (`test` passes --all-targets, which excludes them), so
# the two feature-variant passes from `test` are mirrored here: doc examples gated behind
# lib-net's `packed` or lib-tms's `std` would otherwise never execute anywhere.
test-doc:
    cargo test --doc --workspace --locked
    cargo test --doc -p lib-net --locked --features packed
    cargo test --doc -p lib-tms --locked --features std

# Full docs gate: build the whole site AND prove every doc example runs.
docs-check: docs test-doc

# Local build of the tailnet docs-site image (.ci/docs.Dockerfile — nginx + the rendered book/;
# needs `just docs` first and a local Docker daemon). CI's publish job (docs.yml) builds/pushes
# the same image to Harbor and rolls out the k3s deployment; runbook: .ci/k8s/docs/README.md.
docs-image tag="sentient-docs:local":
    docker build -f .ci/docs.Dockerfile -t {{tag}} .

# ---- Yocto (distro/kas/sentient.yml, .ci/yocto.Dockerfile) --------------------------------------
#
# Local dev entry points into the same kas manifest release.yml's Yocto job drives, so the
# pipeline never drifts from what a developer runs on a workstation. Requires `kas` + BitBake's
# host deps — i.e. run these from inside
# sentient-yocto-builder (`docker run -it sentient-yocto-builder bash`, or `kas shell`), not bare
# on a workstation, unless that workstation already has the full Yocto host-package set installed.
#
# `machine` selects the kas config-layer override (see distro/kas/sentient-rpi4.yml); the bare
# sentient.yml defaults to MACHINE=raspberrypi5. Pass e.g. `just yocto-build rpi4` for the Pi 4
# (64-bit) target, or `qemuarm64`/`qemux86-64` for the emulation-proxy machines the release e2e
# uses (TODO(verify-at-build): add sentient-qemuarm64.yml / sentient-qemux86-64.yml kas config-
# layer files, mirroring sentient-rpi4.yml, once the e2e harness — owned by another workstream,
# distro/qemu/yocto-e2e.sh — needs them; not added here since this task doesn't own that script).

# Fetch + configure the pinned layer set and build every recipe kas resolves for MACHINE (no
# explicit image target — see distro/kas/sentient.yml's own TODO on kas's `target:` key). Mostly
# useful as a fetch/sync step ahead of `yocto-image`.
yocto-build machine="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{machine}}" ] && [ "{{machine}}" != "raspberrypi5" ]; then
      kas build distro/kas/sentient.yml:distro/kas/sentient-{{machine}}.yml
    else
      kas build distro/kas/sentient.yml
    fi

# Build the sentient-image target explicitly for MACHINE (the actual release artifact:
# .wic/.wic.bz2 + the create-spdx SBOM). TODO(verify-at-build): `sentient-image` is the recipe
# name distro/meta-sentient/recipes-sentient/sentient-image/sentient-image.bb is expected to
# define (owned by another workstream) — confirm the PN matches once that recipe lands.
yocto-image machine="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{machine}}" ] && [ "{{machine}}" != "raspberrypi5" ]; then
      KAS_TARGET="sentient-image" kas build distro/kas/sentient.yml:distro/kas/sentient-{{machine}}.yml --target sentient-image
    else
      kas build distro/kas/sentient.yml --target sentient-image
    fi

# Functional multi-platform QEMU e2e for the Yocto image (boot under runqemu on the emulation-proxy
# MACHINEs, drive the guest UART with SeplosSim, assert TMS payloads over a paho MQTT client via
# slirp hostfwd — see the plan's Phase 2 "Functional multi-platform QEMU e2e" section). The harness
# script itself (distro/qemu/yocto-e2e.sh) is authored by another workstream — this recipe only
# wires it in, with an `n` loop-count argument so local == CI.
# TODO(verify-at-build): confirm distro/qemu/yocto-e2e.sh's actual CLI surface (subcommands like
# up/down/build/run, or a single run-with-flags form) once that script lands; this recipe assumes
# a `run {{n}}` convention since the plan explicitly models it that way.
yocto-e2e-run n="1":
    ./distro/qemu/yocto-e2e.sh run {{n}}

# ---- Builder images ---------------------------------------------------------------------------

# Build + push the sentient-builder + sentient-yocto-builder images to
# ghcr.io (the pipeline pulls them back through the Harbor proxy rust-mirror:443/ghcr/…). ghcr.io
# has valid TLS — no cert setup. Creds USED, never stored: set a GitHub username + PAT
# (write:packages) in the environment:
#   GHCR_USERNAME=you GHCR_TOKEN=ghp_… just publish-images
#   GHCR_USERNAME=you GHCR_TOKEN=ghp_… just publish-images high-velocity-decisions ghcr.io v0.2.1
# PRIMARY path is now .github/workflows/builder-image.yml (CI-built on arc-runner-set — no local
# Docker daemon needed); this recipe/script is the local/offline fallback.
publish-images namespace="high-velocity-decisions" registry="ghcr.io" tag="latest":
    ./.ci/scripts/publish-builder-image.sh {{namespace}} {{registry}} {{tag}}

# ---- TMS compliance attestation (OSCAL + conftest, EXCLUDED from the firmware workspace) --------
#
# Generates the MIL-STD-3071 TMS conformance attestation from docs/compliance/tms/tms-coverage.json
# (the tms-idl-codegen descriptor factory's generated type/topic catalogue — `just tms-codegen`)
# against compliance/oscal/catalog.json's Required/Conditional topic controls, gated by the
# conftest/OPA policies in compliance/policy/. Entirely out-of-workspace: tools/tms-attest is a
# stdlib-only Python script, not a Cargo crate — never touches the Rust dependency graph /
# cargo-vet surface. Install conftest as a pinned release binary (curl + sha256 verify — see
# .github/workflows/compliance.yml for the exact command); trestle (pinned pip) is only needed to
# re-author/validate compliance/oscal/*.json, not to run these gates.

# Generate compliance/assessment-results.json + compliance/tms-compliance-report.md (runs conftest
# as part of generation). Non-fatal by design — the report is written either way so you can see
# WHAT failed; `just tms-attest-check` is the actual pass/fail CI gate.
tms-attest:
    python3 tools/tms-attest/generate_report.py || true
    @echo "wrote compliance/assessment-results.json + compliance/tms-compliance-report.md"

# CI gate: conftest's own exit code against the real coverage file, nothing else. Fails loudly,
# listing every Required/Conditional topic (MIL-STD-3071 Table B.2 / A.8.7) missing a type binding,
# plus any provenance or catalogue-integrity regression (compliance/policy/*.rego).
tms-attest-check:
    conftest test --policy compliance/policy --data compliance/policy/data docs/compliance/tms/tms-coverage.json

# Self-test: prove the Rego policies actually DENY on a known-broken input — a regression guard on
# the policies THEMSELVES (a Rego bug that silently stopped denying would otherwise go unnoticed
# since the real coverage file is currently green). Expects conftest to FAIL against the fixture;
# this recipe inverts that so a green `just tms-policy-selftest` means "the policies correctly
# caught the injected breakage".
tms-policy-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    if conftest test --policy compliance/policy --data compliance/policy/data compliance/policy/testdata/tms-coverage-broken-example.json; then
      echo "FAIL: conftest passed against the intentionally-broken fixture — the Rego policies stopped catching real gaps" >&2
      exit 1
    fi
    echo "OK: conftest correctly failed against the intentionally-broken fixture (see compliance/policy/testdata/tms-coverage-broken-example.json)"

# ---- MQTT visualization / simulation stack (deploy/ — docs/deployment/grafana-viz.md) ----------

# Engine selection: `just viz_builder=podman viz-images`, `just viz_compose="docker compose" viz-up`
# (or export VIZ_BUILDER / VIZ_COMPOSE).
viz_builder := env_var_or_default("VIZ_BUILDER", "docker")
viz_compose := env_var_or_default("VIZ_COMPOSE", "podman-compose")

# Build both stack images from the multi-target Dockerfile (needs BuildKit; amd64 + arm64 hosts).
# podman gets --ignorefile because only BuildKit auto-discovers Dockerfile.dockerignore.
[group('viz')]
viz-images tag="local":
    {{ viz_builder }} build -f deploy/docker/Dockerfile {{ if viz_builder == "podman" { "--ignorefile deploy/docker/Dockerfile.dockerignore" } else { "" } }} --target app -t sentient-viz-app:{{ tag }} .
    {{ viz_builder }} build -f deploy/docker/Dockerfile {{ if viz_builder == "podman" { "--ignorefile deploy/docker/Dockerfile.dockerignore" } else { "" } }} --target sim -t sentient-viz-sim:{{ tag }} .

# Lint + render the Helm chart (no cluster needed).
[group('viz')]
viz-lint:
    helm lint deploy/charts/sentient-viz
    helm template sentient-viz deploy/charts/sentient-viz > /dev/null

# Install/upgrade into the current kube-context (kind/k3s/minikube — load images first, see docs).
[group('viz')]
viz-install namespace="sentient-viz":
    helm upgrade --install sentient-viz deploy/charts/sentient-viz -n {{namespace}} --create-namespace

[group('viz')]
viz-uninstall namespace="sentient-viz":
    helm uninstall sentient-viz -n {{namespace}}

# Bring the compose quickstart up (no cluster needed; build images first: just viz-images).
[group('viz')]
viz-up:
    {{ viz_compose }} -p sentient-viz -f deploy/compose/compose.yaml up -d

[group('viz')]
viz-down:
    {{ viz_compose }} -p sentient-viz -f deploy/compose/compose.yaml down

# Send a control line to the sim's FIFO (compose route; the k8s equivalent is in the docs).
[group('viz')]
viz-ctl line="faults high_voltage_protection":
    {{ viz_compose }} -p sentient-viz -f deploy/compose/compose.yaml exec sim sh -c 'echo "{{ line }}" > /ctl/sim.ctl'
