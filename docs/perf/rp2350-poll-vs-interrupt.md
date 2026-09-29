# RP2350 telemetry poll loop — hardware interrupt / wakeup vs. polling

**Question:** would the device's telemetry poll loop benefit from a hardware interrupt / wakeup
instead of "polling", for efficiency (power and/or CPU)?

**Target:** `crates/bin/mcu-rp2350/ns` (Cortex-M33, `thumbv8m.main-none-eabihf`), the device-hosted-broker
firmware. **Date:** 2026-07-25. **Method:** code + upstream-crate source read (no HIL this session;
µA-level claims are explicitly HIL-gated below).

> **Bottom line up front.** The premise "the loop wastes power/CPU by busy-polling" is **false**. The
> loop is **already effectively interrupt-driven**: it idles on an `embassy_time::Timer` that suspends
> the executor so the core sleeps on `WFE`, the Seplos UART read awaits an **IRQ-backed** buffered
> future, and the W5500 receive path is already gated on the chip's **hardware INT line**. There is no
> busy-spin to remove. The only remaining lever is a **deeper** sleep mode (RP2350 `DORMANT`), and the
> device's role as an **always-on MQTT broker + DDS publisher** precludes it. **Recommendation: change
> nothing for efficiency's sake.** Details and per-candidate verdicts follow.

---

## 1. Current wake architecture (cited)

The firmware runs one embassy thread-mode executor on core 0 with several always-resident tasks
(`run_task` = the Seplos poll loop, `broker_task`, the two W5500 runner tasks, `spdp_task`,
`dds_pub_task`, `watchdog_task`) plus a trivial `main` idle loop
(`crates/bin/mcu-rp2350/ns/src/main.rs:290`). Every one of them advances only by `.await`, so when none
is runnable the executor sleeps.

### 1a. The executor idle = `WFE` (Cortex-M sleep)

embassy-executor 0.10.0's Cortex-M thread executor idles by executing `WFE`:

```rust
// embassy-executor-0.10.0/src/platform/cortex_m.rs:100-108
pub fn run(&'static mut self, init: impl FnOnce(Spawner)) -> ! {
    init(self.inner.spawner());
    loop {
        unsafe {
            self.inner.poll();     // poll all ready tasks
            asm!("wfe");           // then sleep until an event/interrupt/SEV
        };
    }
}
```

Its own doc comment (`cortex_m.rs:58-67`) states the executor "uses the `WFE` ARM instruction to
sleep when it has no more work to do … allows for ultra low power consumption for chips where `WFE`
triggers low-power sleep." On the RP2350 Cortex-M33, `WFE` with `SLEEPDEEP` clear is the ordinary
core-sleep (core clock gated; XOSC/PLL/peripherals keep running) — confirmed by embassy-rp 0.10.0's
`aon_timer` module doc, which distinguishes "**interrupt wake (WFI/WFE)**" from "**DORMANT (deep
sleep)**" where "XOSC is powered down" (`embassy-rp-0.10.0/src/aon_timer/mod.rs:19-26`).

**So "polling" here is a misnomer: between iterations the core is asleep in `WFE`, woken by the timer
IRQ.** The scheduling is already interrupt-driven at the HAL level.

### 1b. The inter-poll delay is a suspending Timer, not a spin

The shared loop calls `platform.sleep(interval_ms)` once per iteration
(`crates/impl-app/src/lib.rs:101` on success, `:104`/`:230` on backoff). On the RP2350 that resolves
to:

```rust
// crates/bin/mcu-rp2350/ns/src/impl_embassy_clock.rs:23-31
async fn sleep_ms(&mut self, ms: u32) {
    crate::POLL_HEARTBEAT.fetch_add(1, Ordering::Relaxed);   // watchdog liveness bump
    Timer::after_millis(ms as u64).await;                    // suspends the executor
}
```

`Timer::after_millis(..).await` registers a deadline with the embassy-time driver (a hardware alarm)
and yields `Pending`; the executor then hits the `WFE` above and the core sleeps until the alarm IRQ
fires. embassy-time is tickless — there is **no** periodic tick wake between scheduled deadlines. The
configured Seplos interval is **2000 ms** (`main.rs:213`, `run_config()`), i.e. ~0.5 Hz on the MCU
(the footprint note's "~1 Hz" is the generic cadence figure).

### 1c. The Seplos UART read is IRQ-backed, with a timer safety-net

The transport is `BufferedUart` — interrupt-driven, static ring buffers, bound to
`UART0_IRQ => BufferedInterruptHandler<UART0>` (`main.rs:124-127`, `boot_seplos.rs:81-89`). A `read()`
awaits bytes the UART RX **interrupt** has deposited in the ring buffer; while the wire is idle the
future is `Pending` (core asleep). The adapter races that read against a 500 ms deadline so a silent /
half-framed device can't wedge the task forever:

```rust
// crates/bin/mcu-rp2350/ns/src/impl_buffered_uart_adapter.rs:52-55
let read_fut = Read07::read(&mut self.uart, buf);
let timeout_fut = Timer::after(Duration::from_millis(READ_TIMEOUT_MS)); // 500 ms
match select(read_fut, timeout_fut).await { … }
```

Each received byte resets the deadline (the `read_until`/`read_exact` loop calls `read()` per byte),
so the timer only fires on a genuine stall. This is **event-driven with a bounded timeout**, not a
poll of the wire.

### 1d. The W5500 receive path is already on the hardware INT line

embassy-net-wiznet 0.3.0's `Runner::run()` gates all RX draining on the W5500's INT pin going low:

```rust
// embassy-net-wiznet-0.3.0/src/lib.rs:63-95
pub async fn run(mut self) -> ! {
    let mut tick = Ticker::every(Duration::from_millis(500));
    loop {
        match select3(
            async { self.int.wait_for_low().await.ok(); rx_chan.rx_buf().await }, // RX ← HW INT
            tx_chan.tx_buf(),                                                       // TX ← event
            tick.next(),                                                            // link poll 500 ms
        ).await { … }
    }
}
```

`self.int` is the GPIO input wired to the W5500 INT pad — GP21, `Input::new(int_pin, Pull::Up)`,
active-low (`boot_w5500.rs:92,105`). This is not incidental: `boot_w5500.rs:86-91` records that when
INT and RESET were swapped, `wait_for_low()` never asserted, RX never drained, and DHCP never
completed — i.e. **the whole receive leg is driven by this hardware interrupt**, and it is correctly
wired. The only periodic wake in this task is a cheap 500 ms link-state check (`is_link_up`) and
`tx_buf()` is event-driven on outbound frames.

### 1e. No busy-spin anywhere in the idle path

The broker pump (`lib-net/src/embassy/pump/async_broker_pump.rs`) `select3`s over `accept_source()`,
`rx.recv()`, and a read future that returns `core::future::pending()` when no client slot is connected
(`:105-114`) — a fixed regression (with a dedicated test at `:348-420`) precisely so the pump
**suspends** instead of spinning at 100 % CPU when idle. `accept()` on `:1883` suspends on the smoltcp
stack (woken by W5500 RX → INT). The one residual poll is a **50 ms** retry in `accept_source` used
only when the socket-buffer pool is momentarily exhausted (`main.rs:472`) — a rare transient, not the
steady state.

**Summary of steady-state wakes while the BMS is quiet:** the Seplos 2 s timer, the W5500 runner's
500 ms link ticker, and the 2 s watchdog check — plus any real network/UART IRQ. Everything else is
asleep in `WFE`.

---

## 2. Per-candidate verdicts

| Candidate | Verdict | Why |
|---|---|---|
| **Seplos request cadence** (replace the 2 s timer with an interrupt) | **already-optimal — no event source exists** | Seplos is **master-initiated request/response**: the device is silent until the MCU sends a command, so there is **nothing to interrupt on**. The cadence is inherently a driver timer. That timer already suspends the executor → `WFE` sleep between polls (§1a/1b). You cannot make "ask every 2 s" event-driven; there is no unsolicited RX to wake on. |
| **W5500 INT pin** (drive RX off the hardware interrupt vs. SPI polling) | **already-optimal — already implemented** | embassy-net-wiznet already gates RX on `int.wait_for_low()` (§1d), wired to GP21. There is no SPI busy-poll of the chip to convert; the win this candidate imagines is **already banked**. (A latent bug — INT/RESET swapped — was fixed on HW 2026-07-23.) |
| **UART read deadline** (make the 500 ms read timeout event-driven) | **already-optimal** | The read already awaits the UART **RX interrupt**; the 500 ms `Timer` is only a stall guard, not a polling interval (§1c). It costs one alarm registration per read and fires only on a genuine stall. Nothing to convert. |
| **Deeper sleep between polls** (RP2350 `DORMANT` / stop mode, wake on timer/GPIO) | **not-worth-it — precluded by the device's role** | See §3. |

---

## 3. Deep sleep (`DORMANT`) — the only real lever, and why it loses

RP2350 does offer a much deeper mode than `WFE`: `DORMANT`, in which "XOSC is powered down" and wake
must come from an LPOSC-clocked AON-timer alarm or a GPIO event
(`embassy-rp-0.10.0/src/aon_timer/mod.rs:19-40`). In principle the device could `DORMANT` for most of
each ~2 s Seplos gap and wake on a timer. In practice the device's **job forbids it**:

- **It hosts an always-on MQTT broker on `:1883`.** MQTT clients connect and expect prompt responses;
  the broker must be reachable continuously. `DORMANT` powers down XOSC → the W5500 SPI link, smoltcp
  stack, and TCP accept path are dead while dormant. Clients would see the broker disappear for ~2 s
  windows.
- **It is a DDS/RTPS + SPDP participant** (`spdp_task`, `dds_pub_task`) that must answer discovery and
  keep the **DHCP lease** and **ARP/NDP** state live. Sleeping through those breaks discovery and
  risks losing the lease.
- **Wake latency and complexity.** Coming out of `DORMANT` re-locks XOSC/PLLs (tens of µs–ms) and
  needs the AON timer on LPOSC; embassy's default executor does **not** enter `DORMANT` (it does
  `WFE`), so this would be a bespoke power state machine fighting the always-on network tasks that
  keep the core busy anyway.

Net: for a mains-or-bus-powered, always-networked broker/telemetry node, `DORMANT` is the wrong
trade. It would only pay off for a **disconnected, battery, duty-cycled** sensor — which this device
is not.

---

## 4. Expected efficiency delta (qualitative; what HIL would be needed to quantify)

- **CPU:** zero improvement available. The footprint note already measured ≈1.72 µs/reading against a
  ~1 Hz poll — ~5 orders of magnitude of headroom; the device is I/O-bound (UART baud + Ethernet), and
  the idle path already suspends. No redesign moves this needle.
- **Power (WFE, today):** between wakes the core is already in Cortex-M sleep. Marginal further savings
  from a redesign are **small** because the W5500 + net stack are the dominant always-on consumers and
  must stay powered for the broker role. The frequent-ish wakes (500 ms wiznet link ticker, 2 s
  watchdog/Seplos) are cheap `WFE` exits, each a few instructions.
- **Power (DORMANT, hypothetical):** could cut core+XOSC draw during the sleep window, but only by
  breaking the broker/DDS/DHCP liveness (§3) — so it is not a usable win here.
- **What would need HIL to quantify (cannot be measured without the bench):** actual board current in
  `WFE` idle vs. active µA, the duty ratio of awake-vs-`WFE` per 2 s cycle, and the W5500's own draw —
  i.e. whether the MCU core is even a meaningful fraction of total board power (it likely is **not**,
  next to the W5500 PHY). Capture via a current probe on the bench (`sentient-pi-bench`) alongside the
  RTT timing counters already proposed in `docs/perf/rp2350-footprint-perf.md` (Performance §). Until
  then, no µA figure should be asserted.

---

## 5. Recommendation

**Do nothing for efficiency.** The telemetry loop is already interrupt/event-driven where an event
source exists (UART RX IRQ, W5500 INT) and already sleeps the core (`Timer` → `WFE`) where the work is
genuinely a timed master-poll (Seplos, which has no event to wake on). There is no busy-spin left to
remove — the one historical busy-spin (broker pump, zero clients) was fixed and is regression-tested.
The only deeper lever, `DORMANT`, is precluded by the always-on broker/DDS/DHCP role.

If power ever *does* become a real requirement (e.g. a battery variant), the honest path is not
"interrupt-drive the poll" (already done) but a **product-level** decision to duty-cycle the whole
node — accepting that the device stops being an always-reachable broker while asleep — and that should
be driven by a HIL current-budget measurement, not a code refactor.

---

### Sources cited
- `embassy-executor-0.10.0/src/platform/cortex_m.rs:58-67,100-108` — thread executor idles on `WFE`.
- `embassy-rp-0.10.0/src/aon_timer/mod.rs:19-40` — WFI/WFE (interrupt wake) vs. DORMANT (XOSC down,
  LPOSC/AON-timer wake).
- `embassy-net-wiznet-0.3.0/src/lib.rs:63-95` — RX gated on `int.wait_for_low()`; 500 ms link ticker.
- `crates/impl-app/src/lib.rs:101,104` — the loop's single `platform.sleep(..)` per iteration.
- `crates/bin/mcu-rp2350/ns/src/impl_embassy_clock.rs:23-31` — `sleep_ms` = `Timer::after_millis`.
- `crates/bin/mcu-rp2350/ns/src/impl_buffered_uart_adapter.rs:52-55` — IRQ read raced with a 500 ms guard.
- `crates/bin/mcu-rp2350/ns/src/boot_w5500.rs:86-91,92,105` — W5500 INT = GP21 (active-low), the RX
  hardware-interrupt wiring.
- `crates/lib-net/src/embassy/pump/async_broker_pump.rs:105-114,348-420` — pump suspends (no
  busy-spin); zero-client regression test.
- `docs/perf/rp2350-footprint-perf.md` — CPU ≈1.72 µs/reading, I/O-bound; on-target timing HIL-gated.
