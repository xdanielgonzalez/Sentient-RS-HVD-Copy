# cyclonedds-probe — containerized CycloneDDS participant used as the interop AUTHORITY for the
# DDS/RTPS transport (story #321 / #323): joins MIL-STD-3071 domain 0 over RTPS/UDP (IPv6-capable),
# subscribes named TMS topics, asserts sample receipt/type-decode/USER_DATA/QoS semantics
# (subscribe_assert.py), and independently derives the XCDR2 wire bytes CycloneDDS itself would put
# on the wire for the golden-fixture topics (dump_reference.py) — the empirical oracle for the
# `@optional`-in-APPENDABLE encoding arbitration (`lib_tms::cdr::emit_optional`/`emit_optional_seq`;
# see `crates/lib-tms/src/cdr/mod.rs` module doc item 3).
#
# Pattern mirrors `.ci/docs.Dockerfile` / `.ci/Dockerfile`: base image resolves through Harbor's
# Docker Hub pull-through cache (rust-mirror:443/docker-hub/library/*) so builds stay off the public rate
# limit; `pip install` itself goes straight to PyPI (no proxy configured for it in this repo).
#
# Build (repo root as context): podman build -f .ci/cyclonedds.Dockerfile -t cyclonedds-probe .
# Run subscribe_assert:  podman run --rm --network host cyclonedds-probe subscribe_assert.py --help
# Run dump_reference:    podman run --rm cyclonedds-probe dump_reference.py --help
#
# `--network host` is required for real interop runs (subscribe_assert.py needs to see the SPDP
# multicast group on the shared netns with the publisher — see test-e2e's dds_interop_e2e.rs, WP7);
# dump_reference.py needs no network at all (pure in-process (de)serialization).
FROM rust-mirror:443/docker-hub/library/python:3.12-slim

# eclipse-cyclonedds's Python binding (PyPI `cyclonedds`) ships a bundled/vendored native `ddsc` C
# library — no separate system package needed. Pinned: a version bump here is a deliberate,
# re-verified edit (the arbitration verdict below cites this exact version).
RUN pip install --no-cache-dir cyclonedds==11.0.1

WORKDIR /probe
COPY deploy/dds-probe/subscribe_assert.py deploy/dds-probe/dump_reference.py deploy/dds-probe/tms_mirror_types.py ./

ENTRYPOINT ["python3"]
CMD ["subscribe_assert.py", "--help"]
