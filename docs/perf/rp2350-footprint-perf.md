# RP2350 (Pico 2 / Cortex-M33) Firmware — Footprint & Performance

**Target:** `crates/bin/mcu-rp2350/ns` (non-secure world: W5500 device-hosted MQTT broker + Seplos
UART poll + DDS/RTPS/SPDP publisher). Chip: RP2350A (Cortex-M33, `thumbv8m.main-none-eabihf`).
**Date measured:** 2026-07-24. **Toolchain:** `rustc 1.92.0` (stable) / `nightly` for
`-Zemit-stack-sizes`. **Profile:** `release` (`lto = true`, `opt-level = "s"`, `panic = "abort"`).

> Doctrine: every number below is from a tool, with the exact command shown (Methodology §). No
> estimates except where explicitly labelled (host→MCU scaling, HIL-gated items).

> **⚠️ Update — the #1 lever has since been applied (PR #363, task #364).** The `LATEST_STORE`
> over-provisioning called out below was fixed by right-sizing the store to `LatestStore<8, 1024>`
> (topics 32→8, payload 8192→1024, behaviour-preserving const-generic default in lib-net). Measured
> result: **static RAM 341.8 KiB (76.3 %) → 90.6 KiB (20.2 %)** of the 448 KiB NS budget;
> `LATEST_STORE` 260.3 KiB → 36.6 KiB (−251 KiB). **All footprint numbers below describe the
> pre-fix baseline** unless noted; re-run the Methodology commands against `main` for post-fix
> figures.

---

## Executive summary

| Metric | Value | Budget | % of budget |
|---|---|---|---|
| **Flash footprint** (default `ns`) | **113.3 KiB** (115,980 B) | NS FLASH 3,968 KiB (`0x3E0000`) | **2.85 %** |
| **Static RAM** (`.data`+`.bss`, default) | **341.8 KiB** (349,960 B) | NS RAM 448 KiB (`0x70000`) | **76.3 %** |
| Core0 executor stack headroom | 170.2 KiB (RAM top − static end) | — | — |
| **Worst-case stack frame** | **26,088 B** (`broker_task::poll`) | fits in 170 KiB core0 stack | ✅ |
| **Per-reading CPU cost (host)** | **≈ 1.72 µs** (parse+decode+2×XCDR2) | 1 Hz poll cadence | negligible |

**Headline findings**
- **Flash is a non-issue** (2.85 % of the 3,968 KiB NS region). Even the `defmt` build is 3.9 %.
- **Static RAM is the constraint at 76.3 % of the 448 KiB NS budget** — and it is dominated by a
  **single static: `LATEST_STORE` = 260.3 KiB (266,504 B), i.e. 76 % of all static RAM**. This is
  a `LatestStore<32>` with 8,192-byte payload slots — grossly over-provisioned vs. the actual
  measured XCDR2 payloads (88–464 B) and the two topics actually published. **This is the #1
  reduction lever by a wide margin.**
- **CPU is not a bottleneck.** The whole per-reading pipeline is ≈1.7 µs on the host against a
  ~1 Hz Seplos poll — the device is I/O-bound (UART + Ethernet), not CPU-bound, by ~5 orders of
  magnitude of headroom even after host→MCU de-rating.
- On-target timing (poll period, pubs/sec, DDS cadence, `picotool` flash usage) is **HIL-gated** —
  see Performance §. The bench (`sentient-pi-bench`, aarch64 Pi 5) is reachable and has OpenOCD,
  but has no `picotool`, the board is SWD-only (not USB-enumerated), and SWD/GPIO needs sudo
  (none passwordless). Procedure documented for when the board is available.

---

## Footprint

### 1. Section sizes (default `ns`, no features)

`arm-none-eabi-size -A -x`:

| Section | Size (B) | Load addr | Notes |
|---|---:|---|---|
| `.vector_table` | 276 | `0x10000000`* | Cortex-M vector table |
| `.start_block` | 20 | `0x10000114` | RP2350 IMAGE_DEF |
| `.text` | 100,452 | `0x10000128` | code |
| `.rodata` | 2,940 | `0x100187a0` | constants |
| `.data` | 12,292 | `0x20000000`* | initialised RAM (LMA in flash) |
| `.bss` | 337,668 | `0x20003008`* | zero-init RAM |
| `.uninit` | 0 | — | — |

\* Addresses shown are from the built ELF (the current `ns` links at chip origin
`0x10000000`/`0x20000000`; the S/NS `memory.x` split relocates NS to `0x10020000`/`0x20010000` at
final image assembly — the *sizes* are unaffected, only the origins).

- **Flash used** = `.vector_table`+`.start_block`+`.text`+`.rodata`+`.data` = **115,980 B = 113.3 KiB**
  → **2.85 %** of the 3,968 KiB (`0x3E0000`) NS FLASH region.
- **Static RAM** = `.data`+`.bss` = **349,960 B = 341.8 KiB** → **76.3 %** of the 448 KiB (`0x70000`)
  NS RAM region (S world owns the first 64 KiB; the NS budget is the real ceiling, not the chip's
  full 520 KiB).
- Berkeley cross-check: `text 103176  data 12292  bss 337668  dec 453136`.

### 2. Feature-variant deltas

| Build | `.text` | `.data` | `.bss` | Flash Δ | RAM Δ |
|---|---:|---:|---:|---:|---:|
| **default** | 103,176 | 12,292 | 337,668 | — | — |
| `--features defmt` | 150,420 | 12,356 | 338,724 | **+46.1 KiB** | +1.1 KiB |
| `--features core1-parser` | 103,972 | 12,292 | 341,844 | +0.8 KiB | **+4.1 KiB** |

- `defmt` adds ~46 KiB of `.text` (RTT logging + formatting), trivial RAM. Strip it in production
  to reclaim flash (though flash is not constrained).
- `core1-parser` adds ~4 KiB RAM = the `CORE1_STACK` (4,128 B) spawned on the second core; default
  build does not spawn core1.

### 3. Static-RAM breakdown — the biggest statics (measured, `nm --size-sort`)

`arm-none-eabi-nm --print-size --size-sort` on the default ELF:

| Symbol | Bytes | KiB | What it is |
|---|---:|---:|---|
| **`LATEST_STORE`** | **266,504** | **260.3** | `LatestStore<32>`: 32 topics × (128 B topic + **8,192 B payload**) |
| `broker_task::POOL` | 17,048 | 16.6 | broker task async future arena (embassy task pool) |
| `MQTT_CHANNEL` | 16,696 | 16.3 | depth-2 channel of `MqttUnit` (each carries `[u8; MAX_PAYLOAD=2048]`) |
| `SOCKET_POOL` (`.data`) | 12,292 | 12.0 | 4× TCP rx/tx pairs (`4·(1024+2048)`) for broker sockets |
| `W5500_STATE` | 12,268 | 12.0 | `embassy_net_wiznet::State<4,4>` packet queues |
| `BROKER_CORE` | 7,364 | 7.2 | `BrokerCore<3>` (3 client slots) |
| `W5500_STACK_RESOURCES` | 3,976 | 3.9 | smoltcp `StackResources<7>` socket table |
| `run_task::POOL` | 3,176 | 3.1 | poll task async future arena |
| `PAYLOAD_SCRATCH` | 2,049 | 2.0 | `[u8; MAX_PAYLOAD]` render scratch |
| `DDS_TX_BUF`+`DDS_PKT_BUF` | 2,050 | 2.0 | DDS UDP tx + packet buffers |
| `__embassy_main::POOL` | 960 | 0.9 | main task future arena |

These ~11 statics account for essentially all 341.8 KiB. **`LATEST_STORE` alone is 76 %** of static
RAM. `LatestStore` (`crates/lib-net/src/embassy/pump/types/latest_store.rs`) is defined
`MAX_TOPICS=32`, `MAX_PAYLOAD_LEN=8192` — vs. measured XCDR2 payloads of 88 B
(`DcMeasurementUpdate`) / 464 B (`PowerHardwareUpdate`) and only **two** topics published.

### 4. `.text` by crate (`cargo bloat --crates`, guesswork-approx)

| Crate | % of `.text` | Size |
|---|---:|---:|
| smoltcp | 42.5 % | 41.5 KiB |
| embassy_executor | 28.3 % | 27.6 KiB |
| std¹ | 5.7 % | 5.5 KiB |
| lib_mapping | 4.7 % | 4.6 KiB |
| embassy_net_driver_channel | 3.6 % | 3.5 KiB |
| embassy_net | 2.8 % | 2.7 KiB |
| embassy_rp / lib_net / lib_io / lib_parse / … | rest | — |

¹ `std` entries are `core`/`compiler_builtins` (`memcpy`/`memmove`) attributed by cargo-bloat. The
IP net stack (smoltcp + embassy_net + driver_channel ≈ 48 KiB) and the async executor (27.6 KiB)
are the code-size backbone — both essential to the device-hosted-broker design.

---

## Performance

### Host micro-benchmarks (x86_64 — bounds algorithmic cost, NOT on-target cycles)

Measured with a `std::time::Instant` harness (500 k iters, 1 k warmup, `black_box`) over the real
14-cell live Seplos frame `LIVE_FRAME` (from `lib-parse/tests/seplos_vectors.rs`). **These are host
(x86_64) figures — the RP2350 Cortex-M33 at ≤150 MHz will be materially slower per op** (lower
clock, in-order M-profile), but they establish the algorithmic weight and relative cost.

| Operation | ns/op (host) | Payload out |
|---|---:|---:|
| `ResponseFrame::parse` (CRC + structure) | 303.9 | — |
| parse + `decode_reading` → `CanonicalReading` | 319.7 | — |
| `Seplos::parse_reading` (full vendor path) | 337.9 | — |
| `render_tms` JSON `DcMeasurementUpdate` | 80.8 | 229 B |
| `render_tms_cdr` XCDR2 `DcMeasurementUpdate` | 115.2 | 88 B |
| `render_tms_cdr` XCDR2 `PowerHardwareUpdate` (14 cells) | 1,230.3 | 464 B |
| **FULL: parse+decode + both XCDR2 renders** | **1,719.2** | — |

- **Per-reading host CPU ≈ 1.72 µs.** The 14-cell `PowerHardwareUpdate` XCDR2 render (~1.23 µs)
  dominates; parse+decode is ~0.34 µs.
- Against a ~1 Hz Seplos poll, even a conservative 10–30× host→MCU de-rating (→ tens of µs) leaves
  CPU utilisation for the telemetry pipeline at well under 0.01 %. **The device is I/O-bound
  (UART baud + Ethernet), not CPU-bound.** CPU cost is not a design risk.

### On-target timing — HIL-GATED (not measured this session)

The following require the bench hardware and are **not fabricated**:

- poll-loop iteration period, publish rate (pubs/sec), DDS/SPDP sample cadence
- flash occupancy via `picotool info`
- true executor-stack high-water (vs. the 26 KiB worst *frame* measured statically below)

**Why not measured now:** `sentient-pi-bench` (aarch64 Pi 5) is reachable and has OpenOCD, **but**
(a) no `picotool` installed, (b) the Pico 2 is SWD-attached, not USB-enumerated (`lsusb` shows no
`2e8a` device), and (c) linuxgpiod SWD + GPIO needs root and the account has no passwordless sudo.

**Procedure to capture on-target (per `justfile` `mcu-rp2350-rtt` + `docs/testing/mcu-dds-spdp-hil.md`):**
1. `cd crates/bin/mcu-rp2350 && cargo build --release -p ns --features defmt`
2. `arm-none-eabi-nm …/release/ns | grep _SEGGER_RTT` → RTT control-block address
3. OpenOCD over linuxgpiod SWD (`swclk=GPIO11`, `swdio=GPIO10`), `rtt setup <addr> 1024`, `rtt
   server start 9090 0`
4. `defmt-print -e …/release/ns tcp --host localhost --port 9090`
5. **Counters to add for timing:** a `defmt::info!` timestamp delta bracketing the poll loop
   (period), a monotonically-incrementing publish counter logged each emit (pubs/sec), and
   `picotool info -a` once flashed (flash occupancy). RTT timestamps give sub-ms cadence resolution.

### Stack frames (measured, `-Zemit-stack-sizes` + `llvm-readobj --stack-sizes`)

Largest per-function frames (final ELF + per-CU objects in `deps/*.rlib`, max across CUs):

| Frame (B) | Function |
|---:|---|
| **26,088** | `TaskStorage<broker_task…>::poll` |
| 24,784 | `TaskStorage<__embassy_main…>::poll` |
| 13,720 | `boot_w5500::spawn_net_and_broker` (one-shot init) |
| 8,368 | `ChannelPublisher<…>` |
| 6,104 | `TaskStorage<run_task…>::poll` |
| 4,104 | `embassy_net::new::<Device<1514>, 7>` |
| 2,208 | `TaskStorage<dds_pub_task…>::poll` |
| 1,240 | `smoltcp Interface::poll` |
| 1,096 | `TaskStorage<spdp_task…>::poll` |
| 320 | `lib_parse … ResponseFrame::parse` |

- The dominant frames are the **async task `poll` state machines**, which run on **core0's
  executor/main stack**. Worst is `broker_task::poll` at **26,088 B (25.5 KiB)**.
- Core0 stack headroom = NS RAM top (`0x20080000`) − static end (`0x2005570c`) = **174,324 B ≈
  170 KiB**. The 25.5 KiB worst frame + its call chain fits comfortably — **no overflow risk
  today** — but these frames scale with the payload/topic buffers, so they are the thing to watch
  if `LATEST_STORE`/`MAX_PAYLOAD`/channel depth grow (cf. the `zephyr-main-stack-future` hazard:
  large by-value locals inflate a single poll frame).
- Note: the 25.5 KiB is a *transient stack frame*, distinct from the 16.6 KiB `broker_task::POOL`
  *arena* (the future's resident size in `.bss`) — both are counted; the arena is in the static
  RAM table above, the frame is a runtime stack cost.

---

## Risks & headroom

| Item | Status | Lever |
|---|---|---|
| **Static RAM 76.3 % of NS budget** | ⚠️ closest to a wall | shrink `LatestStore` (below) |
| `LATEST_STORE` = 260 KiB (76 % of RAM) | ⚠️ over-provisioned | `MAX_TOPICS 32→~4-8`, `MAX_PAYLOAD_LEN 8192→~512-1024` |
| Flash 2.85 % | ✅ vast headroom | (strip `defmt` → −46 KiB if ever needed) |
| Worst stack frame 25.5 KiB / 170 KiB | ✅ ~6.6× headroom | keep payload buffers bounded |
| Core1 stack 4 KiB (`core1-parser`) | ✅ small | measure if core1 work grows |
| CPU ≈1.7 µs/reading @ 1 Hz | ✅ ~5 orders of magnitude spare | — |

**Top 3 footprint consumers:** (1) `LATEST_STORE` 260 KiB; (2) async task arenas + MQTT channel
(broker 16.6 + channel 16.3 + run 3.1 + main 0.9 ≈ 37 KiB — `MqttUnit` embeds a 2,048-byte
payload); (3) networking buffers (`SOCKET_POOL` 12 + `W5500_STATE` 12 + `StackResources` 3.9 ≈
28 KiB).

**Top 3 reduction levers:**
1. **Right-size `LatestStore`** (`crates/lib-net/src/embassy/pump/types/latest_store.rs`):
   `MAX_PAYLOAD_LEN 8192→1024` and `MAX_TOPICS 32→8` gives 8×1024 ≈ **9.2 KiB, saving ≈251 KiB**
   (static RAM 76 %→ ~21 %). Measured payloads are ≤464 B and only 2 topics publish — even
   4×512 B is generous. **Single biggest win in the whole image.**
2. **Trim the MQTT channel / `MqttUnit` payload** (16.3 KiB): apply the canonical-in-memory /
   render-on-drain doctrine so the channel carries a compact `CanonicalReading` rather than a
   pre-rendered `[u8; 2048]`; or reduce `MAX_PAYLOAD` (2048) toward the measured max, and/or
   `MQTT_CHANNEL_DEPTH`.
3. **Networking buffer tuning** (≈28 KiB): `SOCKET_POOL` (`POOL_TX_LEN=2048`) and `W5500_STATE<4,4>`
   queue depths are sized for headroom; if `MAX_CLIENTS` or queue depth can drop, so do these. Flash
   levers (defmt strip) exist but are moot — flash is not constrained.

---

## Methodology (reproducible commands)

```bash
# Build (excluded workspace; .cargo/config pins thumbv8m.main-none-eabihf)
cd crates/bin/mcu-rp2350
cargo build --release -p ns                              # default (warning-clean)
cargo build --release -p ns --features defmt             # +RTT logging
cargo build --release -p ns --features core1-parser      # +core1
# (or `just mcu-rp2350` for abi + s + ns × variants)

ELF=target/thumbv8m.main-none-eabihf/release/ns

# Section sizes / footprint
arm-none-eabi-size -A -x $ELF
arm-none-eabi-size    $ELF

# Largest static-RAM symbols (.bss/.data)
arm-none-eabi-nm --print-size --size-sort --radix=d $ELF | tail -30

# Code size by crate
cargo bloat --release -p ns --crates -n 20

# Stack frames (nightly for -Zemit-stack-sizes; release lto=true keeps a subset in the final
# ELF, full set survives in per-CU deps/*.rlib objects)
rustup target add thumbv8m.main-none-eabihf --toolchain nightly
RUSTFLAGS="-Zemit-stack-sizes" cargo +nightly build --release -p ns
llvm-readobj --stack-sizes $ELF                          # + iterate deps/*.rlib .o members
#   (parser: scratchpad/parse_stack.py — final ELF + `ar p rlib *.o` | llvm-readobj, max per symbol)

# Host micro-bench: temporary tests/zz_perf_bench.rs (removed after run) over LIVE_FRAME,
# std::time::Instant, 500k iters:
cargo test --release -p lib-mapping --test zz_perf_bench -- --nocapture

# HIL (requires bench hardware; not run this session): see justfile `mcu-rp2350-rtt`
```

Memory-region facts from `crates/bin/mcu-rp2350/ns/memory.x` (NS FLASH `0x3E0000`=3,968 KiB @
`0x10020000`; NS RAM `0x70000`=448 KiB @ `0x20010000`) and `s/memory.x` (S owns first 128 KiB flash
/ 64 KiB RAM). Chip totals: 4 MiB QSPI flash, 520 KiB SRAM.
