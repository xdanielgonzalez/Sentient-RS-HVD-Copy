# sentient-yocto-builder — Yocto/BitBake (kas) toolchain image for the release.yml Yocto image job.
#
# Separate from sentient-builder (.ci/Dockerfile): BitBake's host-package footprint plus the sstate/
# downloads caches this job needs are large and are used by exactly one job (the Yocto `.wic` image
# build in release.yml). Keeping the main builder lean means every _lint/_test/_supply-chain/
# _quality job doesn't pull a multi-GB image it never uses. (The retired sentient-zephyr-builder
# image — .ci/zephyr.Dockerfile, EPIC #350 story #352 — used to share this same rationale for its
# own toolchain footprint; this image's own reasoning stands on its own now.)
#
# FROM sentient-builder (not a bare rust:1.91 / ubuntu base) so the musl-static Rust target this
# job's upstream artifact (bin-rpi aarch64-unknown-linux-musl) was built with, and the kellnr
# mirror config, are already present in the image lineage — though this job itself does NOT invoke
# cargo (it consumes a prebuilt bin-rpi artifact per the plan's "prebuilt binary recipe" decision;
# no Rust toolchain runs inside BitBake).
#
# Build:  docker build -f .ci/yocto.Dockerfile -t sentient-yocto-builder \
#           --build-arg BASE_IMAGE=<locally-tagged-or-pushed sentient-builder> .
#         Default BASE_IMAGE below points at the Harbor tag builder-image.yml publishes; a local
#         build without registry access can build .ci/Dockerfile locally first (`docker build -f
#         .ci/Dockerfile -t rust-mirror:443/sentient/sentient-builder:latest .`) so this FROM resolves
#         from the local image cache instead of pulling.
#
# TODO(verify-at-build): this Dockerfile is authored from the Yocto wrynose docs' "Required
# Packages for the Build Host" list (docs.yoctoproject.org, wrynose manual) and kas's own install
# docs, but has never been built here (greenfield, no local Yocto toolchain to validate against).
# Confirm the full package list + kas version resolve cleanly on a bookworm base at build time.

ARG BASE_IMAGE=rust-mirror:443/sentient/sentient-builder@sha256:e30a010df3c8a398a50ff3d61e374bdbea05a12819fb39fa86e2d54b9b038e12
FROM ${BASE_IMAGE}

# Fail a RUN if any stage of a pipe fails (e.g. `curl ... | sha256sum -c -`), not just the last one.
SHELL ["/bin/bash", "-euo", "pipefail", "-c"]

LABEL org.opencontainers.image.title="sentient-yocto-builder" \
      org.opencontainers.image.description="sentient-builder + kas/BitBake host deps for the release.yml Yocto (wrynose) Pi image build." \
      org.opencontainers.image.source="https://github.com/High-Velocity-Decisions/sentient-rs"

ENV DEBIAN_FRONTEND=noninteractive

# ---------------------------------------------------------------------------------------------
# apt: Yocto wrynose (5.0 LTS) "Required Packages for the Build Host" (Debian/Ubuntu list, per
# the official Yocto Project Reference Manual / Quick Build guide for the wrynose release), plus
# `sudo` (needed below to drop root -> the `builder` user cleanly) and `locales` (to generate
# en_US.UTF-8 — BitBake refuses to run under a non-UTF-8 / "C" locale).
#
# TODO(verify-at-build): cross-check this list against the wrynose manual's Debian/Ubuntu column
# at build time — package names/availability can drift between Debian bookworm (this image's base)
# and the Ubuntu LTS the Yocto docs are written against (e.g. bookworm's default python3 is 3.11,
# which wrynose's BitBake supports; some optional doc-only packages are intentionally omitted
# here since this image never runs `bitbake -c build doc`).
# ---------------------------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
      gawk \
      wget \
      git \
      diffstat \
      unzip \
      texinfo \
      gcc \
      build-essential \
      chrpath \
      socat \
      cpio \
      python3 \
      python3-pip \
      python3-pexpect \
      xz-utils \
      debianutils \
      iputils-ping \
      python3-git \
      python3-jinja2 \
      python3-subunit \
      zstd \
      liblz4-tool \
      file \
      locales \
      libacl1 \
      sudo \
    && rm -rf /var/lib/apt/lists/* \
    # Debian's locale-gen IGNORES a locale argument (unlike Ubuntu's) — it only generates locales
    # uncommented in /etc/locale.gen. `locale-gen en_US.UTF-8` was therefore a no-op and BitBake,
    # which hard-requires en_US.UTF-8, aborted. Add the entry, then generate.
    && echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen \
    && locale-gen

ENV LANG=en_US.UTF-8 \
    LANGUAGE=en_US:en \
    LC_ALL=en_US.UTF-8

# ---------------------------------------------------------------------------------------------
# kas: the pinned-manifest layer fetcher/builder driver (distro/kas/sentient.yml). Installed via
# pip (kas is a Python tool, not a system package on bookworm) into the system site — this image
# is single-purpose (one job), so no dedicated venv is needed (kas has no NON-default-Python-minor
# constraint on bookworm's 3.11).
#
# TODO(verify-at-build): pin an exact kas version once chosen (kas-container / kas classic both
# read distro/kas/sentient.yml identically for our purposes; plain `kas` here, no container-in-
# container, since we're already inside a container on the ARC runner). `--break-system-packages`
# is required on bookworm's PEP 668-protected system Python; revisit if this image later gets a
# dedicated venv instead.
# ---------------------------------------------------------------------------------------------
ARG KAS_VERSION=4.4
RUN pip3 install --no-cache-dir --break-system-packages "kas==${KAS_VERSION}"

# ---------------------------------------------------------------------------------------------
# Robot Framework — drives the functional QEMU e2e (distro/qemu/*.robot): boots the built
# sentient-image under `runqemu` and asserts Seplos -> TMS -> MQTT, emitting a report.html/
# output.xml TEST-CASE REPORT (the CI artifact). (The retired Zephyr lane's Renode boot-smoke,
# _emulation.yml, used to be the MCU analogue of this — EPIC #350, story #352, retired it.)
#   - paho-mqtt v2 is the API MqttKeywords targets ("paho-mqtt v2" per its own docstring); it
#     subscribes to the guest broker over the runqemu slirp hostfwd.
# (qemu-system-aarch64/-x86_64 for the boot come from the Yocto build's own native sysroot via
# `runqemu`, not from this image; the Seplos sim is the Rust `seplos-tcp-sim`, cargo-built at job time
# from the sentient-builder base's toolchain.)
# ---------------------------------------------------------------------------------------------
RUN pip3 install --no-cache-dir --break-system-packages \
      "robotframework==6.1" \
      "robotframework-retryfailed==0.2.0" \
      "paho-mqtt>=2.0,<3"

# ---------------------------------------------------------------------------------------------
# MinIO client (mc) — the write side of the sstate/downloads MIRROR. After each build, _yocto.yml
# `mc mirror`s $SSTATE_DIR + $DL_DIR up to the MinIO `cache` bucket (authenticated) so the mirror
# ACCUMULATES our hardened-distro sstate across runs, and `mc anonymous set download` (idempotent)
# opens the sstate/ + downloads/ prefixes for the anonymous HTTP GETs BitBake's wget mirror does
# on the read side (SSTATE_MIRRORS in distro/kas/sentient.yml). Single static binary, no deps; the
# version is frozen at this image's digest (which the workflows pin), so `latest` here is reproducible.
# ---------------------------------------------------------------------------------------------
RUN wget -q https://dl.min.io/client/mc/release/linux-amd64/mc -O /usr/local/bin/mc \
    && chmod +x /usr/local/bin/mc \
    && mc --version

# ---------------------------------------------------------------------------------------------
# BitBake refuses to run as root (a hard, deliberate upstream restriction — it aborts immediately
# with "do not use the BitBake root user" if uid 0 invokes it). Create a non-root `builder` user
# that owns the build/cache trees this job writes to (DL_DIR/SSTATE_DIR below), matching how a
# real Yocto build host is set up. `sudo` is kept only so CI debugging steps can escalate if truly
# needed (e.g. apt-get in a one-off diagnostic run) — the job itself never needs it.
# ---------------------------------------------------------------------------------------------
RUN useradd --create-home --shell /bin/bash builder \
    && usermod -aG sudo builder \
    && echo "builder ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/builder \
    && chmod 0440 /etc/sudoers.d/builder

# ---------------------------------------------------------------------------------------------
# Shared Yocto caches (downloads + shared-state). Mounted/restored as a CI cache volume by the
# release.yml Yocto job (Yocto builds are heavy — sstate reuse across runs is not optional, it's
# the difference between a multi-hour and a ~10-minute rebuild on an unchanged layer set). Owned
# by `builder` so BitBake (running as that user) can write to them without a permission fixup step.
# TODO(verify-at-build): confirm these paths match distro/kas/sentient.yml's local_conf_header
# DL_DIR/SSTATE_DIR values exactly — they must agree for the cache to actually be hit.
# ---------------------------------------------------------------------------------------------
ENV YOCTO_DL_DIR=/opt/yocto-cache/downloads \
    YOCTO_SSTATE_DIR=/opt/yocto-cache/sstate-cache
RUN mkdir -p "${YOCTO_DL_DIR}" "${YOCTO_SSTATE_DIR}" \
    && chown -R builder:builder /opt/yocto-cache

# NOTE: the image deliberately DEFAULTS TO ROOT (no trailing `USER builder`). Under a GitHub Actions
# `container:` on the ARC k8s runner, the runner writes its file-command state under
# /__w/_temp/_runner_file_commands/ (root-owned) — a non-root default USER makes actions/checkout
# itself fail with EACCES before any step runs, and ARC's k8s containerMode ignores a `--user`
# override. So the container runs as root for the runner, and the Yocto jobs (_yocto.yml,
# release.yml) drop to the `builder` user (created above) via `runuser` ONLY for the `kas build`
# invocation, which is the one thing BitBake refuses to run as root. builder still owns the caches.
ENV PATH="/home/builder/.local/bin:${PATH}"
