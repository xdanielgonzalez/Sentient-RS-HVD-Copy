# syntax=docker/dockerfile:1
# Multi-stage build for the MQTT visualization/simulation stack (see deploy/README.md).
# Targets:
#   app — static musl bin-rpi + socat pty bridge (the Seplos->TMS->MQTT adapter)
#   sim — static musl seplos-tcp-sim (the Seplos BMS battery simulator)
#
#   docker build -f deploy/docker/Dockerfile --target app -t sentient-viz-app:local .
#   docker build -f deploy/docker/Dockerfile --target sim -t sentient-viz-sim:local .

FROM rust:1.91-bookworm AS builder
ARG TARGETARCH
WORKDIR /src

# Map the Docker platform arch to a musl triple. Both are NATIVE builds inside the
# arch-matching base image, so plain gcc handles C objects — no cross toolchain.
RUN case "$TARGETARCH" in \
      amd64) echo x86_64-unknown-linux-musl  > /rust-target ;; \
      arm64) echo aarch64-unknown-linux-musl > /rust-target ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac && rustup target add "$(cat /rust-target)"

# DELIBERATELY not copying .cargo/ — it pins the internal kellnr mirror
# (rust-mirror:8000), unreachable from a laptop docker build. Cargo.lock is
# source-replacement-agnostic, so crates resolve identically from crates.io.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# Host-buildable workspace member; its firmware sibling hil/pico-seplos-emu is excluded
# (like the MCU-only bin/mcu-rp2350 workspace, which the app image never builds).
COPY hil/pico-seplos-emu-core/ hil/pico-seplos-emu-core/
# The vendored, rule-(12)-patched dust_dds fork consumed via [patch.crates-io] path override in the
# workspace Cargo.toml (see vendor/dust_dds/PATCHES.md). It IS a build input — cargo resolves the
# path dep here, so it must be in the build context.
COPY vendor/ vendor/

ENV CC_x86_64_unknown_linux_musl=gcc  AR_x86_64_unknown_linux_musl=ar \
    CC_aarch64_unknown_linux_musl=gcc AR_aarch64_unknown_linux_musl=ar \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
    RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic -C link-self-contained=yes"

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    T="$(cat /rust-target)" && \
    cargo build --locked --release --target "$T" -p bin-rpi && \
    cargo build --locked --release --target "$T" -p test-kit --features tcp-sim --bin seplos-tcp-sim && \
    install -D "target/$T/release/bin-rpi" /out/bin-rpi && \
    install -D "target/$T/release/seplos-tcp-sim" /out/seplos-tcp-sim

FROM alpine:3.20 AS app
# socat: pty<->TCP bridge to the simulator; tini: PID-1 signal handling.
RUN apk add --no-cache socat tini
COPY --from=builder /out/bin-rpi /usr/local/bin/bin-rpi
COPY --chmod=0755 deploy/docker/app-entrypoint.sh /usr/local/bin/app-entrypoint.sh
ENV SIM_HOST=sim SIM_PORT=5001 PTY_LINK=/tmp/ttySIM RUST_LOG=info
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/app-entrypoint.sh"]

FROM alpine:3.20 AS sim
RUN apk add --no-cache tini
COPY --from=builder /out/seplos-tcp-sim /usr/local/bin/seplos-tcp-sim
COPY --chmod=0755 deploy/docker/sim-entrypoint.sh /usr/local/bin/sim-entrypoint.sh
ENV SIM_PORT=5001 SIM_PACE=19200
ENTRYPOINT ["/sbin/tini", "--", "/usr/local/bin/sim-entrypoint.sh"]
