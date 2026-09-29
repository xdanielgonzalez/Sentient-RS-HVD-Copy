# cyclonedds-conformance — WP7a stock-tooling DDS conformance runner (story #321/#324).
#
# Layered FROM the WP5 probe image (.ci/cyclonedds.Dockerfile), which already ships the pip
# `cyclonedds` package (the `cyclonedds` CLI: ls/ps/typeof/subscribe/publish) + the local-type probes
# (subscribe_assert.py, dump_reference.py, tms_mirror_types.py) under /probe. This layer only adds the
# orchestration script + `ip` for netns diagnostics, and flips the entrypoint to it.
#
#   # 1) build the base first (WP5 image; repo root as context):
#   podman build -f .ci/cyclonedds.Dockerfile -t cyclonedds-probe .
#   # 2) build this runner (repo root as context):
#   podman build -f deploy/dds-conformance/tools.Dockerfile -t cyclonedds-conformance .
#
# Driven by compose.yaml (deploy/dds-conformance/) — not run standalone in normal use.
FROM localhost/cyclonedds-probe:latest

# iproute2 = `ip` (netns diagnostics in run_conformance.sh). coreutils `timeout`, bash, grep, awk are
# already in the python:3.12-slim base.
RUN apt-get update && apt-get install -y --no-install-recommends iproute2 \
    && rm -rf /var/lib/apt/lists/*

COPY --chmod=0755 deploy/dds-conformance/run_conformance.sh /probe/run_conformance.sh
COPY --chmod=0755 deploy/dds-conformance/local_delivery_probe.py /probe/local_delivery_probe.py

WORKDIR /probe
ENTRYPOINT ["/probe/run_conformance.sh"]
