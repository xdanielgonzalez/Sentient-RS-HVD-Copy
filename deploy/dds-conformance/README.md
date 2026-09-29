# DDS/RTPS conformance harness (stock CycloneDDS CLI)

WP7a (story #321 / #324). A `docker-compose` harness that validates the MIL-STD-3071 DDS/RTPS
surface of the **real `bin-rpi` adapter** (built from this branch, `[dds]` enabled) against the
**official CycloneDDS Python CLI** (`cyclonedds ls / typeof / subscribe`), fed by the **real
`seplos-tcp-sim`**. It is the stock-tooling counterpart to the bespoke `deploy/dds-probe/` scripts,
and the substrate WP7's CI lane will drive.

## Topology — one shared network namespace (`network_mode: host`)

Three services, all in the **host network namespace**:

| service       | image                              | role |
|---------------|------------------------------------|------|
| `sim`         | `sentient-viz-sim:conformance`     | Seplos BMS simulator; TCP-serves telemetry on `127.0.0.1:5001` (viz `sim` target). |
| `adapter`     | `sentient-viz-app:conformance`     | real `bin-rpi` (this branch) + socat pty↔tcp bridge to the sim; `[dds]` on → DDS/RTPS beside MQTT. |
| `conformance` | `cyclonedds-conformance:local`     | stock CycloneDDS CLI runner (`run_conformance.sh`); its exit code is the verdict. |

**Why host netns (not a bridged per-container netns, not `service:adapter`):** SPDP discovery is
IPv6 **link-local multicast** on the custom group `ff02::1234`. That requires a real MULTICAST-flagged
interface shared by publisher and subscriber. The host netns exposes `eth0` (MULTICAST) to all three
services — the same substrate the WP0/WP4 spike used when it *did* deliver a sample to CycloneDDS.
`lo` has **no** MULTICAST flag (so an `lo`-pinned `service:` share can't carry SPDP), and rootless
podman's bridged per-container netns does not reliably forward IPv6 link-local multicast between
containers. Host netns eliminates that variable. Cost: services bind **host** ports (`sim` 5001,
adapter embedded broker **11883** — off the default 1883 to avoid collisions, RTPS 7400+).

The adapter advertises DDS unicast DATA locators at `eth0`'s **link-local** address
(`fe80::215:5dff:fee2:3c1d` on this dev machine). Override per host:

```bash
export DDS_IFACE=eth0
export DDS_UNICAST_ADDR="$(ip -6 -o addr show dev "$DDS_IFACE" scope link | awk '{print $4}' | cut -d/ -f1)"
```

## Container runtime

Rootless **podman** (4.9.3, netavark). Compose provider: **`podman-compose`**
(`/home/jstandish/.local/bin/podman-compose`). Note: `podman compose` on this box shells out to a
broken Docker-Desktop `docker-compose` — use `podman-compose` explicitly. A `docker`→`podman` shim
lives in the scratchpad `shimbin/` if a `docker` name is needed.

## Run

```bash
# 1) Images (from this branch). App + sim reuse the viz Dockerfile targets:
podman build -f deploy/docker/Dockerfile --ignorefile deploy/docker/Dockerfile.dockerignore \
  --target app -t sentient-viz-app:conformance .
podman build -f deploy/docker/Dockerfile --ignorefile deploy/docker/Dockerfile.dockerignore \
  --target sim -t sentient-viz-sim:conformance .
# The stock-CLI base (WP5 image) + this runner layer:
podman build -f .ci/cyclonedds.Dockerfile -t cyclonedds-probe .
podman build -f deploy/dds-conformance/tools.Dockerfile -t cyclonedds-conformance:local .

# 2) Run the harness (conformance exits -> whole stack tears down):
podman-compose -f deploy/dds-conformance/compose.yaml up --abort-on-container-exit

# 3) Verdict (exit code of the conformance container) + artifacts:
podman-compose -f deploy/dds-conformance/compose.yaml down
ls deploy/dds-conformance/artifacts/   # ls.txt, typeof_*.txt, subscribe_*.txt, assert_*.txt
```

## What each check proves (and against which clause)

| check | command | clause | asserts |
|-------|---------|--------|---------|
| (a) discovery + QoS | `cyclonedds ls -i 0 --qos` | **D.2** domain 0, **D.3** topic names, **D.5** QoS/USER_DATA | our participant + every published writer topic appear on domain 0 via SEDP (no XTypes needed). **HARD.** |
| (b) type propagation | `cyclonedds typeof <topic>` | **D.1** XTypes | reconstruct topic IDL over XTypes TypeInformation. **REPORT ONLY** — a new conformance datum. |
| (c) delivery | `cyclonedds subscribe <topic>` | **D.1** XCDR2 | stock CLI receives/decodes samples. XTypes-gated (type discovered "as in `typeof`"). **HARD.** |
| (d) delivery isolation | `subscribe_assert.py` (local type) | — | local-typed reader receipt; separates the XTypes gate (b/c) from the RTPS DATA-delivery gate. **REPORT ONLY.** |

`ls` in `cyclonedds` **11.0.1 has no `--json`** — the runner parses the text table by grepping the
known topic names (machine-checkable). `typeof`/`subscribe` both need XTypes TypeInformation from the
writer; the local-typed probe `(d)` does not, which is what makes it the isolation oracle.

## Known-open items this harness exercises

- **WP6 zero-DATA-delivery blocker**: full-stack bin-rpi discovered (SPDP/SEDP match) but **zero RTPS
  DATA submessages** reached CycloneDDS. Stock `subscribe` `(c)` is an independent confirmation; the
  local-typed `(d)` disambiguates: `(d)` receives ⇒ blocker is XTypes type propagation; `(d)` also
  empty ⇒ blocker is the dust_dds DATA path.
- **Rule-12 (XTypes sequence delimiter, DDSXTY14-56)**: CycloneDDS prefixes sequences of non-primitive
  elements with a 4-byte length delimiter; dust_dds 0.15.0 does not. Sequence-bearing topics
  (`DcMeasurementUpdate`) are in the `subscribe`/`assert` set so any decode divergence surfaces here.
