<p class="eyebrow">Deployment · Build, Test, Run</p>

# Build, test & run guide

Everything you need to build, test, lint, and run `sentient-rs`. The `justfile` at the repo root is
the single source of truth for build/test/lint/security/release — both developers and CI call the
same recipes, so the pipeline can never drift from what runs locally. For on-device install/wiring,
see [`deployment.md`](./README.md); for the module-by-module map, see
[`architecture.md`](../architecture.md) and the [API reference](../reference/README.md).

## Prerequisites

- Rust toolchain matching the MSRV **1.91** (`[workspace.package] rust-version`).
- [`just`](https://just.systems): `cargo install just --locked`.
- `just` recipes shell out with `bash -euo pipefail`; on Windows use WSL/Git Bash.
- List every recipe: `just --list` (or just `just`, the default recipe).

### Internal crate mirror (`.cargo/config.toml`)

`.cargo/config.toml` points cargo at the High-Velocity-Decisions internal **kellnr** mirror:
`crates.io` is served from `sparse+http://rust-mirror:8000/api/v1/cratesio/`, rustup from
`http://rust-mirror:8080`, and git deps fetch via the git CLI. Those hosts resolve on the HVD
network (CI configures the same in its runner containers). Off-network, override the source or point
cargo back at upstream crates.io.

## Stage 1 — format + lint (the CI gate)

```sh
just fmt          # cargo fmt --all --check
just fmt-fix      # cargo fmt --all  (dev convenience)
just clippy       # cargo clippy --workspace --all-targets --locked -- -D warnings
just lint         # fmt + clippy
```

Production lint posture (`[workspace.lints.clippy]`): `unwrap_used`, `expect_used`, and `panic` are
**denied** in logic; test code opts out with scoped `#[allow]`s.

## Stage 2 — tests + no_std builds

```sh
just test             # cargo nextest run --workspace --locked (excludes test-e2e/test-acceptance,
                       # each has its own dedicated lane below — see the justfile's own comment)
just test-doc          # cargo test --doc --workspace --locked (nextest can't run doctests)
just test-host-link   # cargo test -p lib-net --features link-host,tls  (pnet/AF_PACKET + folded smoltcp)
```

### no_std / embedded build

The protocol/mapping core, the Embassy MCU platform adapter, and the owned net path build with
`--no-default-features` for two Cortex-M targets: the Cortex-M4F *reference* target
`thumbv7em-none-eabihf`, and the *real* shipping MCU target `thumbv8m.main-none-eabihf` (RP2350A /
Cortex-M33). This proves the embedded contract compiles — including on the chip
`crates/bin/mcu-rp2350` actually instantiates it on:

```sh
just build-no-std
# equivalent to:
rustup target add thumbv7em-none-eabihf
cargo build --locked -p lib-parse -p lib-canonical -p lib-store -p lib-tms -p lib-mapping \
  -p lib-io -p lib-profile -p spec-app -p impl-app -p bin-mcu-platform \
  --no-default-features --target thumbv7em-none-eabihf
cargo build --locked -p lib-net --no-default-features \
  --features lib-net/link,lib-net/embassy --target thumbv7em-none-eabihf
cargo build --locked -p lib-net --no-default-features --features packed \
  --target thumbv7em-none-eabihf
# ...then the same core roster again, for the real MCU chip:
rustup target add thumbv8m.main-none-eabihf
cargo build --locked -p lib-parse -p lib-canonical -p lib-store -p lib-tms -p lib-mapping \
  -p lib-io -p lib-profile -p spec-app -p impl-app -p bin-mcu-platform \
  --no-default-features --target thumbv8m.main-none-eabihf
```

## Security feature/tier matrix

`lib-security` composes two compile-time toggles — **FIPS** × **TrustZone** — into a tier. The
matrix recipes clippy + test each buildable tier (see [`security.md`](../security/README.md) for the posture
and `lib-security` in the [API reference](../reference/README.md) for the code surface):

```sh
just sec-tier std                      # one tier, e.g. software / fips,std / trustzone,std / fips,trustzone,std
just sec-no-std                        # no_std core tier: build-only for the MCU (tests need std)
just sec-matrix                        # all buildable tiers (std, fips+std, trustzone+std, fips+trustzone+std, no_std)
```

The **wolfcrypt** backend is excluded from `sec-matrix` — it needs a system wolfSSL 5.9.1+ library
at link time and is a separate build ledger (like the RP2350 MCU firmware below), exercised only by
`.github/workflows/wolfcrypt.yml`. See [`wolfcrypt-backend.md`](../security/wolfcrypt-backend.md).

## MSRV, supply chain, coverage, fuzzing, SBOM

```sh
just msrv          # cargo +1.91 build --locked --workspace  (needs: rustup toolchain install 1.91)

# Supply-chain gate (Stage 1 in CI):
just audit         # cargo audit  (fails on real vulns; unmaintained/yanked stay warnings)
just deny          # cargo deny check
just vet           # cargo vet --locked
just zizmor        # static analysis of the GitHub Actions workflows themselves (--min-severity=low)
just supply-chain  # audit + deny + vet

# Coverage / mutation / semver:
just coverage      # cargo tarpaulin --workspace --exclude bin-rpi --features std --fail-under 75 (Html+Xml)
just mutants       # cargo mutants on lib-security + lib-parse (slow; CI runs on a schedule)
just semver        # cargo semver-checks

# Fuzzing (nightly + fuzz/ targets):
just fuzz                              # default target fuzz_parse_telemetry, 300 s
just fuzz fuzz_parse_telemetry 600     # explicit target + seconds

# SBOM (release):
just sbom          # cargo cyclonedx -f json --all
```

## Release build (the RPi binary)

The release pipeline builds the `profiling` profile (optimized **with** debug symbols) per target,
then strips a copy for the shippable binary:

```sh
just build-release aarch64-unknown-linux-gnu     # cargo build --profile profiling --target <t> -p bin-rpi
just build-auditable aarch64-unknown-linux-gnu   # same, embedding the dependency list into the binary
```

For a plain optimized build and the cross-compile targets (aarch64 / armv7), see
[`deployment.md`](./README.md) §3.

## Aggregate gates

```sh
just ci        # lint + test + build-no-std + sec-matrix   (what a PR must pass locally)
just ci-full   # ci + supply-chain + coverage + semver     (incl. heavier hardening checks)
```

## Running the RPi binary

The binary loads TOML config (overridable by `SENTIENT_`-prefixed env vars, nested with `__`), then
runs the poll loop. Minimal local run:

```sh
SENTIENT_CONFIG=config/sentient.toml \
SENTIENT_SERIAL__PORT=/dev/ttyUSB0 \
cargo run -p bin-rpi
```

Config schema (`config/sentient.toml`: `[device]`, `[serial]`/`[poll]` or the multi-port `[[bus]]`
array-of-tables — each entry optionally setting `protocol = "seplos"|"voltronic"` and `device_id` —
`[broker]`, `[publish]`, plus the top-level `direction` fallback key and per-port `direction`/
`gpio_*`/`settle_*_us`/`tx_drain` RS-485 driver-enable fields, and the opt-in-by-presence
`[capture]` raw-wire-capture section) and env-override rules are documented in full in
[`deployment.md`](./README.md) §4.

### systemd unit + binary verification

Production runs under `distro/deploy/bin-rpi.service` — a hardened unit with `Type=notify` +
`WatchdogSec=30` (the binary pets the watchdog only while healthy) and
`ExecStartPre=verify-binary.sh`, which checks the binary's X.509 CMS signature (via `openssl cms
-verify` against the CA) and refuses to start on a mismatch. Install and enable:

```sh
sudo install -m0644 distro/deploy/bin-rpi.service /etc/systemd/system/bin-rpi.service
sudo install -m0755 distro/deploy/verify-binary.sh     /usr/local/bin/verify-binary.sh
sudo systemctl daemon-reload && sudo systemctl enable --now bin-rpi
sudo journalctl -u bin-rpi -f
```

Full install (user, mosquitto relay, secrets, runbook) is in [`deployment.md`](./README.md).

## MCU firmware build (`crates/bin/mcu-rp2350`, its own workspace)

The MCU ships **Embassy on the RP2350** (Cortex-M33), not Zephyr — `bin/zephyr` and the
`bin-zephyr-platform` adapter were deleted by the Zephyr→Embassy pivot, so the `west` flow that used
to live here is gone (see [ADR 0010](../decisions/0010-mcu-runtime-embassy.md)). The firmware is
`crates/bin/mcu-rp2350`, an **excluded** workspace with its own `Cargo.lock` and its own embassy pins;
its `.cargo/config.toml` pins `thumbv8m.main-none-eabihf`, so no `--target` flag is needed:

```sh
just mcu-rp2350          # rp2350-abi, then rp2350-s (secure world), then `ns` in its feature combos
just mcu-rp2350-uf2      # documentation-only: the picotool combine + demo-sign steps for a UF2
just mcu-hil-setup       # one-time bench check (openocd over SWD); then mcu-flash-device / mcu-rtt
```

The platform adapter itself (`bin-mcu-platform`) is a normal workspace member and is covered by
`just build-no-std` above; only the binary lives outside the workspace, so CI builds it explicitly
and path-dep drift breaks loudly.

**Honest gaps.** There is **no RP2350 emulation** — neither Renode nor QEMU models the chip — so the
merge gate is build/link only; on-target confirmation is HIL smoke on the bench Pico, not a gate
(ADR 0010, and [`rp2350-footprint-perf.md`](../perf/rp2350-footprint-perf.md)). The UF2 signing in
`mcu-rp2350-uf2` is a load-time-integrity demo signature, **not** a verified boot chain — the Gate-0
bootloader is deferred. A dedicated MCU build/flash deployment chapter (the successor to the
archived Zephyr one) has not been written yet.

## Where CI/release config lives

CI and release are GitHub Actions workflows under `.github/workflows/` (do not edit as part of docs
work). They call the same `just` recipes above:

| Workflow | Purpose |
|---|---|
| `ci.yml` | Top-level CI orchestration. |
| `_lint.yml` | `just lint` (fmt + clippy). |
| `_test.yml` | `just test` + no_std builds + the security tier matrix. |
| `_supply-chain.yml` | `just supply-chain` (audit + deny + vet) + `zizmor`. |
| `_quality.yml` | Coverage / mutation / semver checks. |
| `production.yml` | Tag-gated release: per-target `build-release` / `build-auditable`, SBOM, X.509 CMS + cosign signing (binaries **and** `.wic`). The MCU Renode boot-smoke job was removed with the Zephyr tree (ADR 0010) — there is no RP2350 emulation to replace it with. |
| `uat.yml` | Post-merge UAT: build `bin-rpi`, slot it into the golden pre-release image, boot under `runqemu`, preflight the agent. |
| `image.yml` | Path-filtered (`distro/**`) golden Yocto image build. |
| `wolfcrypt.yml` | The separate wolfCrypt build ledger (needs system wolfSSL). |
| `scheduled.yml` | Scheduled heavier checks (e.g. mutation testing). |
