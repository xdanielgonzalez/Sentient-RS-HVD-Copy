# sentient-builder — self-contained CI image for sentient-rs.
#
# Bakes every tool the pipeline installs per-run (see .github/workflows/_lint.yml, _test.yml,
# _supply-chain.yml, _quality.yml, release.yml and .github/actions/setup/action.yml) so CI jobs
# stop paying `cargo install` / `apt-get install` on every run. Built + pushed to Harbor by
# .github/workflows/builder-image.yml; consumed as `container:` by the reusable workflows.
#
# MULTI-STAGE: the ~13 cargo-install tools are compiled in a throwaway `tools` stage and only their
# BINARIES are copied into the final image — so the crate-source registry cache + build artifacts
# (1-3 GB) never land in the shipped layer. The final stage stays FROM rust:1.91-bookworm (matches
# [workspace.package] rust-version / justfile's `msrv`, so `cargo build` needs no toolchain
# resolution) and keeps the actual build environment (toolchain, cross linkers, cmake).
#
# Build:  docker build -f .ci/Dockerfile -t sentient-builder .   (context = repo root; needs
#         .cargo/config.toml for the kellnr mirror)
#
# BASE via Harbor: the rust base resolves through Harbor's Docker Hub pull-through cache
# (rust-mirror:443/docker-hub/library/rust:…) — official Docker Hub images live under `library/`. This
# keeps builds off the public Docker Hub rate limit and reuses the in-cluster cache. Override
# RUST_IMAGE for a local build off Docker Hub directly.

# =================================================================================================
# Stage 1 — fetch the CI tools as PREBUILT binaries via cargo-binstall. Nothing here survives except
# the binaries under /opt/tools/bin (copied into the final stage). binstall DOWNLOADS each tool's
# release binary instead of compiling it from source — the multi-minute `cargo install` compile
# becomes seconds (the same trick the old rust-runner image used). PINNED + --locked: a version bump
# is a deliberate edit, not a moving-target install. Grouped by the workflow that needs each tool so a
# bump for one lane doesn't invalidate every layer.
# =================================================================================================
ARG RUST_IMAGE=rust-mirror:443/docker-hub/library/rust:1.91-bookworm
FROM ${RUST_IMAGE} AS tools

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]
ENV CARGO_TERM_COLOR=always \
    DEBIAN_FRONTEND=noninteractive

# curl + ca-certificates fetch the cargo-binstall installer and the tool binaries. The rest
# (build-essential/pkg-config/cmake/libssl-dev/git) are the compile-FALLBACK safety net: binstall
# falls back to `cargo install` for any crate that has no prebuilt release, which then needs a C
# toolchain — kept so the build can never get stuck on a missing prebuilt.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential \
      pkg-config \
      cmake \
      libssl-dev \
      git \
      curl \
      ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Resolve tool crates through the internal kellnr mirror (not crates.io). Mirrors .cargo/config.toml.
# Used by binstall for crate metadata/version resolution + any compile fallback (binaries themselves
# come from each crate's GitHub release, direct).
COPY .cargo/config.toml $CARGO_HOME/config.toml

# cargo-binstall itself: install its PREBUILT binary via the official installer (which "uses
# cargo-binstall to install cargo-binstall" — no compile). Pulls from the cargo-binstall GitHub
# release; github egress is already required by the cosign step in stage 2.
RUN curl -L --proto '=https' --tlsv1.2 -sSf \
      https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
      | bash \
    && cargo binstall -V

# --root /opt/tools makes binstall mimic `cargo install --root`, so binaries land in /opt/tools/bin
# (the final stage copies exactly them, not the base's cargo/rustc). IMPORTANT: use --root, NOT
# --install-path — when a prebuilt is unavailable (e.g. QuickInstall times out) binstall falls back
# to `cargo install`, which does NOT accept --install-path and hard-fails; --root is a native
# cargo-install flag so the compile fallback (build deps kept above) works. --no-confirm for
# unattended CI; --locked pins to the crate's shipped lockfile.
# _test.yml e2e (nextest); _supply-chain.yml (audit/deny/vet/geiger).
RUN cargo binstall --no-confirm --locked --root /opt/tools \
      cargo-nextest@0.9.140 \
      cargo-audit@0.22.2 \
      cargo-deny@0.19.9 \
      cargo-vet@0.10.2 \
      cargo-geiger@0.13.0
# release.yml (auditable + SBOM + git-cliff release notes).
RUN cargo binstall --no-confirm --locked --root /opt/tools \
      cargo-auditable@0.7.5 \
      cargo-cyclonedx@0.5.9 \
      git-cliff@2.13.1
# _supply-chain.yml (zizmor); _quality.yml (tarpaulin, semver-checks); scheduled.yml (mutants).
RUN cargo binstall --no-confirm --locked --root /opt/tools \
      zizmor@1.26.1 \
      cargo-tarpaulin@0.37.0 \
      cargo-semver-checks@0.48.0 \
      cargo-mutants@27.1.0
# just (recipe runner) + sccache (RUSTC_WRAPPER cache).
RUN cargo binstall --no-confirm --locked --root /opt/tools \
      just@1.55.1 \
      sccache@0.16.0

# =================================================================================================
# Stage 2 — the shipped builder image: toolchain + cross linkers + the tool binaries (no build cache).
# =================================================================================================
ARG RUST_IMAGE
FROM ${RUST_IMAGE}

SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

LABEL org.opencontainers.image.title="sentient-builder" \
      org.opencontainers.image.description="Self-contained sentient-rs CI builder: cross linkers, no_std targets, and every cargo-install CI tool, pinned (multi-stage — tool build cache discarded)." \
      org.opencontainers.image.source="https://github.com/High-Velocity-Decisions/sentient-rs"

ENV CARGO_TERM_COLOR=always \
    RUST_BACKTRACE=1 \
    DEBIAN_FRONTEND=noninteractive

# apt: build essentials, cross linkers (aarch64 + armv7 release targets), the BARE-METAL Arm cross
# compiler (see below), cmake (native crate deps), minisign (release signing),
# git/curl/ca-certificates, xz-utils (tar.xz), libssl (tool runtime dep).
#
# gcc-arm-none-eabi is NOT gcc-arm-linux-gnueabihf under another name: the latter targets Arm *Linux
# userspace* and cannot emit the freestanding Cortex-M33 objects the RP2350 TrustZone Secure-world
# image needs. `crates/bin/mcu-rp2350`'s S-world build compiles its CMSE veneer through `cc`, which wants
# arm-none-eabi-gcc on PATH — so it belongs in the image rather than in a per-job `apt-get install`
# (_mcu-build.yml's `apt_packages` matrix field, which becomes an empty no-op for rp2350 once this
# image is rebuilt + republished via builder-image.yml and the digest pin is bumped).
#
# libnewlib-arm-none-eabi is REQUIRED alongside it, not optional: apt lists it merely as a
# *recommended* package of gcc-arm-none-eabi, and this RUN uses --no-install-recommends, so without
# naming it explicitly the compiler installs with NO freestanding C headers. veneer.c includes
# <string.h>, so the S-world build dies with "fatal error: string.h: No such file or directory".
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential \
      gcc-aarch64-linux-gnu \
      gcc-arm-linux-gnueabihf \
      gcc-arm-none-eabi \
      libnewlib-arm-none-eabi \
      libc6-dev-arm64-cross \
      libc6-dev-armhf-cross \
      pkg-config \
      cmake \
      minisign \
      git \
      curl \
      ca-certificates \
      xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Kellnr mirror baked into $CARGO_HOME so every CI `cargo` resolves through the internal sparse
# mirror. Mirrors .cargo/config.toml verbatim — if that file changes, rebuild this image.
COPY .cargo/config.toml $CARGO_HOME/config.toml

# rustup components + targets baked so per-job `rustup component/target add` are no-ops: lint
# (rustfmt+clippy), release cross targets (aarch64/armv7 Linux), the SHIPPING Pi target
# (aarch64-unknown-linux-musl — the static bin-rpi that `just build-musl` produces for both Pi 4/5
# and UAT), and the two no_std MCU core targets `just build-no-std` builds (thumbv7em-none-eabihf,
# the Cortex-M4F reference chip; thumbv8m.main-none-eabihf, the RP2350A/Cortex-M33 real chip —
# EPIC #350, story #352: MCU runtime pivot Zephyr -> Embassy — replacing the former Zephyr matrix
# targets, thumbv7m-none-eabi + aarch64-unknown-none, baked here only for the now-retired
# sentient-zephyr-builder image). Baking the musl target is not just speed: it removes a
# static.rust-lang.org rust-std download from every bin-rpi / UAT build, which was timing out on
# the self-hosted runners' flaky first-attempt egress.
# The self-hosted runners' internet egress intermittently times out on large downloads (rustup pulls
# each ~30 MB rust-std from static.rust-lang.org). rustup's own 3x per-file retry isn't enough across
# six targets, so wrap the whole `target add` in an outer retry: it's idempotent (already-installed
# targets are skipped instantly), so each attempt only re-downloads whatever is still missing.
RUN rustup component add rustfmt clippy \
    && n=0; until rustup target add \
         aarch64-unknown-linux-gnu \
         aarch64-unknown-linux-musl \
         armv7-unknown-linux-gnueabihf \
         thumbv7em-none-eabihf \
         thumbv8m.main-none-eabihf; do \
         n=$((n+1)); \
         [ "$n" -ge 8 ] && { echo "rustup target add: giving up after $n attempts"; exit 1; }; \
         echo "rustup target add failed (attempt $n/8) — retrying in 15s"; sleep 15; \
       done

# The compiled CI tools — binaries only (the 1-3 GB registry/build cache stayed in the `tools` stage).
COPY --from=tools /opt/tools/bin/ /usr/local/bin/

# cosign: pinned release binary, sha256-verified (not a cargo crate).
ARG COSIGN_VERSION=3.1.1
ARG COSIGN_SHA256=ae1ecd212663f3693ad9edf8b1a183900c9a52d3155ba6e354237f9a0f6463fc
RUN curl -fsSL -o /usr/local/bin/cosign \
      "https://github.com/sigstore/cosign/releases/download/v${COSIGN_VERSION}/cosign-linux-amd64" \
    && echo "${COSIGN_SHA256}  /usr/local/bin/cosign" | sha256sum -c - \
    && chmod +x /usr/local/bin/cosign \
    && cosign version
