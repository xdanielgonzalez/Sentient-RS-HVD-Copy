# Pi hotpath benchmark — real-hardware footprint

Measured **speed, memory, CPU utilization, and thermal/power footprint** of the telemetry hot path
(`impl_app::run_cycle` → decode → canonical → flat-wire render → publish) on the actual shipping
silicon. Produced by `.ci/scripts/pi_bench_harness.sh` running the `hotpath_e2e` bench; raw data is
committed under [`docs/bench-data/`](bench-data/) (a 89 s / 150 000-cycle sustained soak).

## Setup

| | |
|---|---|
| Board | Raspberry Pi Zero 2 W — **Cortex-A53** (ARMv8-A, in-order, 4 cores), 1 GHz, 512 MB |
| Binary | `bin-rpi` `hotpath_e2e`, `aarch64-unknown-linux-musl`, **`profiling` profile (`opt-level=3`)** — the RPi deliverable build |
| Governor | `ondemand` (pins to 1 GHz under load) |
| Workload | `run_cycle` × 150 000 (≈ 89 s sustained), **1 687 cycles/s** |

The A53 has NEON (`asimd`) + hardware CRC32 but **no crypto extension** (`aes`/`sha` absent), so
there is no ARM-accelerated TLS on this part; NEON is what the `opt-level=3` build exploits.

## Speed

Per-function latency (µs), sustained over the full soak:

| Function | avg | p50 | p95 | p99 |
|---|---|---|---|---|
| **run_cycle** (full poll→map→render→publish) | 592 | 576 | 643 | 816 |
| emit_reading (decode→canonical→render) | 14.6 | 13.6 | 17.6 | 35.4 |
| publish_rendered_tapped (flat-wire render + publish) | 12.9 | 12.4 | 15.5 | 23.7 |

Against the committed **x86 dev-box baseline** (`.ci/scripts/bench_baseline.json`):

| Function | x86 avg | Pi p50 | Pi ÷ x86 |
|---|---|---|---|
| run_cycle | 134.8 µs | 576 µs | 4.3× |
| emit_reading | 9.7 µs | 13.6 µs | **1.39×** |
| publish_rendered_tapped | 9.4 µs | 12.4 µs | **1.32×** |

The **render/serialize hot path runs at ~1.3–1.4× of the x86 dev box** — the tight, branch-/alloc-light
flat-wire code nearly matches x86 on a 1 GHz in-order A53 once NEON is on. The full `run_cycle` gap
(4.3×) is dominated by the surrounding async/tokio + rumqttd machinery, **not** the serializer.

## Memory

| | |
|---|---|
| Peak RSS (VmHWM) | **5 MB** — flat across 150 000 cycles (no leak) |
| Heap churn | 2.6 MB total; ~57 B/cycle in `run_cycle`, 274 B/call in publish |

The device-model / render-on-output design keeps the resident set tiny: the broker stores no payload
bytes and the hot path renders into reused buffers.

## CPU utilization (does not starve other processes)

System-wide, all four cores, sampled from `/proc/stat` throughout the soak:

| | |
|---|---|
| Overall | **~32 % average**, 58 % peak (100 % = all 4 cores saturated) |
| Per-core avg | cpu0 ≈ 6 % (harness sampler), cpu1–3 ≈ 34 / 45 / 43 % (the bench's tokio + router threads) |
| **Cores busy** | **~1.3 of 4 on average → ~2.7 cores free for other work** |

The hot path is effectively one core's worth of work spread across the runtime's threads. On the
4-core Pi that leaves the majority of the machine idle — telemetry never monopolizes the CPU, so
co-resident processes (the broker, the mosquitto bridge, systemd, SSH) are never starved.

## Sustained thermal / power

Over the 89 s soak the SoC temperature climbed and leveled off far below the throttle point; the core
held full clock the whole time. Full per-sample trace:
[`pi-zero2-soak90.samples.csv`](bench-data/pi-zero2-soak90.samples.csv).

| t | freq | temp | cpu |
|---|---|---|---|
| 0 s | 1000 MHz | 39.7 °C | 30 % |
| 20 s | 1000 MHz | 44.0 °C | 32 % |
| 40 s | 1000 MHz | 44.5 °C | 34 % |
| 60 s | 1000 MHz | 45.6 °C | 29 % |
| 89 s | 1000 MHz | 46.2 °C | — |

- **No throttling** (`throttled=0x0` throughout), **no under-voltage**, core voltage steady 1.256 V.
- Temperature levels off at **~46 °C** — the 80 °C throttle point is never in play. The A53 sustains
  full 1 GHz on this workload indefinitely; the bottleneck is neither thermal nor power.
- **Real watts are not captured** — the Pi Zero 2 W has no internal current sensor. The harness logs
  power *proxies* (freq/temp/throttle) into a `watts`-ready CSV column. For true power, add an
  INA219/INA260 on the 5 V rail (I²C: `dtparam=i2c_arm=on` + reboot) and feed its reading into that
  column.

Headroom framing: production polls at `interval_ms = 1000`; a 576 µs cycle is **~0.06 %** of that
budget, on ~1.3 of 4 cores. The shipping silicon is nowhere near stressed.

## `opt-level` impact (why the deliverable uses `opt-level=3`)

The workspace `release` profile is `opt-level = "z"` (size — correct for the 64 KB MCU). The Pi ships
aarch64 with room to spare, so `profiling` overrides to `opt-level = 3`, restoring inlining + NEON
autovectorization of the render path:

| Function (p50) | `opt-level="z"` | `opt-level=3` | Δ |
|---|---|---|---|
| emit_reading | 20.2 µs | 13.6 µs | **−33 %** |
| publish_rendered_tapped | 17.0 µs | 12.4 µs | **−27 %** |

Kept generic (no `-C target-cpu`) so one aarch64-musl binary stays portable across the shipping cores
(A72/Pi 4, A76/Pi 5); A53-specific tuning added only ~10 % more and would under-serve those.

## Reproduce

Cross-build the bench for the deliverable profile on an x86 host, then run the harness on the Pi:

```sh
# x86 host — build the aarch64-musl bench (opt-3 profiling profile)
CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc \
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=rust-lld \
RUSTFLAGS="-C opt-level=3 -C target-feature=+crt-static -C relocation-model=pic -C link-self-contained=yes" \
  cargo build --release --target aarch64-unknown-linux-musl \
    -p bin-rpi --example hotpath_e2e --features hotpath,hotpath-cpu,hotpath-alloc
scp target/aarch64-unknown-linux-musl/release/examples/hotpath_e2e pi:/tmp/hotpath

# Pi — sustained soak (speed + memory + cpu + power-proxies -> CSV + summary)
./pi_bench_harness.sh /tmp/hotpath soak 500000    # <binary> <label> <iters>
```
