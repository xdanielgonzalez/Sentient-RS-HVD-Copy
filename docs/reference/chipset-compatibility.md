<p class="eyebrow">Reference · Silicon & Portability</p>

# Chipset compatibility & portability

This chapter is for the integrator who is going to lay out their own board. It answers three
questions in order: **what does this firmware actually require of a microcontroller**, **which
parts can satisfy it today**, and **what does it cost to move**. The current reference target is
the **RP2350A** (Cortex-M33, `thumbv8m.main-none-eabihf`) on an Adafruit Feather RP2350. That is a
bench and bring-up choice, not a shipping recommendation — and the last section of this chapter
says plainly why, and where our knowledge runs out.

The short version: the firmware is **mostly** portable by construction, the chip-coupled surface is
small and enumerable, and the practical limit on your choice is not our code — it is that the Rust
async embedded ecosystem has exactly **three** production-grade vendor HALs.

---

## 1. What the firmware demands of a chip

Everything below is read out of the shipping source tree, not from a datasheet wish list. File and
line references are to `crates/bin/mcu-rp2350/` unless noted.

### Peripheral budget

| Peripheral | Count | Configuration | Where it is bound |
|---|---|---|---|
| UART (buffered, interrupt-driven) | 2 | UART0 Seplos BMS at **19200 8N1**; UART1 Heltec mesh radio at **38400 8N1** | `ns/src/boot_seplos.rs:74`, `ns/src/boot_mesh.rs:51` |
| GPIO (RS-485 driver-enable) | 1 | Half-duplex DE/RE strobe on the Seplos leg | `ns/src/boot_seplos.rs:71` (`PIN_29`) |
| SPI (master, DMA) | 1 | **16 MHz**, W5500 Ethernet controller, 2 DMA channels | `ns/src/boot_w5500.rs:105` |
| GPIO (W5500 control) | 3 | CS, INT (edge-wait capable), RST | `ns/src/boot_w5500.rs:88,95,96` |
| I2C (master, async) | 1–2 | **100 kHz**; INA3221 rail telemetry (I2C0) and/or SSD1306 OLED (I2C1) | `ns/src/boot_i2c.rs:69`, `ns/src/boot_ui.rs:75` |
| DMA | 2–3 | CH0/CH1 for SPI; CH2 only for the optional status LED | `ns/src/main.rs:207` |
| Watchdog | 1 | Gated on a poll-loop heartbeat counter | `ns/src/main.rs:54` |
| Second core | optional | `core1-parser` moves the parser leg to CORE1 | `ns/src/core1.rs` |

**GPIO count.** The mandatory baseline is **9 pins** — UART0 TX/RX + DE (3), SPI CLK/MOSI/MISO (3),
W5500 CS/INT/RST (3). Optional features add: `mesh` +2, `power` +2, `ui` +5 (I2C pair plus three
buttons), `neopixel` +1. `mesh` and `ui` are **mutually exclusive by `compile_error!`** because both
claim GP24/GP25 on this board (`ns/Cargo.toml:138-140`) — a genuine pin-budget collision, not an
arbitrary restriction. A fully loaded build therefore lands between **14 and 17 GPIO**, and a board
you design yourself can resolve that collision by simply not reusing pins.

### The networking leg is external, and that is the point

The device does not use an on-chip Ethernet MAC. **Layer 2 comes from a WIZnet W5500 over SPI**,
driven by `embassy-net-wiznet`. That crate's entire driver surface is generic:

```rust
pub struct Runner<'d, C: Chip, SPI: SpiDevice, INT: Wait, RST: OutputPin>
```

Not one chip-specific type appears in it (`embassy-net-wiznet-0.3.0/src/lib.rs:53`). Our own
wrapper in `crates/lib-net/src/embassy/wiznet/` states the same contract in its module doc: *"It
names NO `embassy-rp` type — the concrete `embassy_rp::spi::Spi`/`Output`/`Input` (or any other
MCU's HAL) is supplied at the call site."* The consequence for a port is large: **the entire
Ethernet leg travels to any HAL that can produce an async `SpiDevice`, a `Wait` input, and an
`OutputPin`.** All three are `embedded-hal-async` traits that every serious HAL implements.

### IPv6 dual-stack is a compliance requirement, not a preference

MIL-STD-3071 mandates RTPS/UDP over **IPv6**. The `embassy-net` feature set is pinned accordingly —
`proto-ipv6`, `multicast`, `slaac`, alongside `dhcpv4` and `dhcpv4-hostname`
(`crates/bin/mcu-rp2350/Cargo.toml:142-147`). The manifest comment records the mechanism precisely:
`slaac` is what actually assigns the interface its `fe80::` link-local address; `proto-ipv6` alone
adds no address at all. **IPv6 acquisition is SLAAC; IPv4 comes from DHCPv4; there is no DHCPv6
dependency anywhere in the stack.** SPDP discovery multicasts to `ff02::1234` on domain 0.

For a chip evaluation this matters because it removes a requirement people expect to find: you do
**not** need vendor IPv6 middleware. smoltcp, underneath `embassy-net`, supplies it, and it is
chip-independent.

### TrustZone-M is a hard requirement

The security architecture is an ARMv8-M Secure/Non-secure split: a secure-world image
(`crates/bin/mcu-rp2350/s/`) owning the first 128 KiB of flash and 64 KiB of RAM, programming the SAU,
and `bxns`-jumping to the non-secure application. This is the cryptographic separation boundary and
the intended path toward a FIPS 140-3 story. **A candidate chip without TrustZone-M is not a
candidate.** In practice that means Cortex-M33/M23/M55/M85, not Cortex-M4/M7.

Two honesty notes, both of which the source already carries:

- **Intent and shipping configuration disagree today.** The `veneers` feature that wires the
  secure-world crypto leg is **off by default** (`ns/Cargo.toml:120`, `default = []`). The default
  build links no `sv_*` veneer and performs no cross-world call. TrustZone-M is designed in and
  partially implemented; it is not switched on in the default image. Treat it as a requirement on
  your silicon choice, and as an open work item in ours.
- **The SAU does not constrain DMA.** `s/src/sau.rs` says so at length: the SAU governs CPU core
  accesses only, and a bus-level access-control mechanism (RP2350 `ACCESSCTRL`, or the equivalent on
  another part — ST's GTZC, Nordic's SPU) is a separate, not-yet-wired requirement. When you
  evaluate a candidate, evaluate its **bus-master** access control, not just its core security
  extension.

### Measured footprint — and why the number is stale

The last full measurement (`docs/perf/rp2350-footprint-perf.md`, **dated 2026-07-24**) recorded:

| Metric | Measured | Notes |
|---|---|---|
| Flash (`.text`+`.rodata`+`.data`+vectors) | **113.3 KiB** (115,980 B) | 2.85 % of the then-current 3,968 KiB NS flash region |
| Static RAM (`.data`+`.bss`) | **90.6 KiB** (20.2 %) | after the `LatestStore<8, 1024>` right-sizing in PR #363 |
| Worst-case stack frame | 26,088 B (`broker_task::poll`) | |
| `--features defmt` delta | +46.1 KiB flash | RTT logging and formatting |

**That measurement predates the current tree and should not be treated as the number for your
budget.** It was taken before the board moved from a Pico 2 (4 MB flash) to the Feather RP2350
(8 MB) — which is why the doc still quotes a 3,968 KiB NS flash region where `ns/memory.x` now
reads `0x7E0000` (8,064 KiB) — and before the `mesh`, `power`, `ui`, and `neopixel` legs were added.
**A re-measurement is owed and has not been done.** We will not extrapolate a figure and present it
as measured; the honest statement is that flash was never close to the constraint, RAM was, and the
current RAM figure is unknown until the Methodology commands in that document are re-run against
`main`.

What you can safely conclude for a chip shortlist: **flash demand is on the order of 128–256 KiB**
and **RAM demand is dominated by tunable buffer statics**, not by code. The largest single consumer
was one store that shrank by 251 KiB when it was right-sized with a const generic. That is a
configuration lever, not a floor.

---

## 2. The Embassy support reality

Embassy is the async runtime this firmware is built on, and it is also the **cross-target seam** —
the MCU and the Linux gateway meet at `embassy-net`/`embedded-hal-async` traits, not at a vendor
SDK. Which means: **the set of chips you can ship this on is the set of chips with a
production-grade Embassy HAL.** As of 2026-07-26, that set is small, and the crates.io listing is
misleading if you read it casually.

| Crate | Version | Reality |
|---|---|---|
| `embassy-stm32` | 0.6.0 | Production. Broad family coverage, active. |
| `embassy-nrf` | 0.11.0 | Production. nRF52/53/91 families. |
| `embassy-rp` | 0.10.0 | Production. RP2040 and RP2350 Arm cores. |
| `embassy-mspm0` | 0.0.0 | **Name reservation.** No code. |
| `embassy-mcxa` | 0.0.0 | **Name reservation.** No code. |
| `embassy-imxrt` | 0.0.0 | **Name reservation.** No code. |
| `embassy-nxp` | — | Not published to crates.io. |
| `embassy-ambiq` | — | Not published to crates.io; GPIO-only in-tree. |
| `embassy-microchip` | — | Not published to crates.io. |

Be blunt about the middle block: **four of the vendor HALs that appear in ecosystem discussion are
0.0.0 placeholders.** A `cargo add embassy-imxrt` succeeds and gives you nothing. If a chip's Rust
async story rests on one of those crates, it does not have one.

**RISC-V is a redesign, not a port.** Two independent blockers: `embassy-rp` does not support the
RP2350's Hazard3 RISC-V cores, and `embassy-executor`'s RISC-V platform carries a hard
`compile_error!` on the interrupt executor. Beyond the runtime, TrustZone-M has no RISC-V
equivalent in our design, so the entire secure-world layer would need re-architecting against PMP or
a vendor TEE. Treat RISC-V as out of scope for this codebase in its current form.

**PIO is the one genuinely RP-unique dependency**, and it is used for exactly one thing: the
optional NeoPixel status LED, driven by `embassy_rp::pio_programs::ws2812`
(`ns/src/boot_neopixel.rs:39`). The feature is off by default and explicitly documented as *"a small
HIL debugging aid, not a mission function."* The portable substitute is `ws2812-spi`, which costs an
SPI bus. On any non-RP target, the cheapest correct answer is to drop the feature.

---

## 3. Candidate comparison

Filtering on the hard constraints — TrustZone-M, a production Embassy HAL, 2×UART + 1×SPI + I2C +
DMA, and enough RAM — leaves four parts.

| | **RP2350A** | **STM32H5** (H563/H573) | **STM32U5** | **nRF5340** |
|---|---|---|---|---|
| Core | Cortex-M33 | Cortex-M33 | Cortex-M33 | Cortex-M33 (app) |
| TrustZone-M | Yes | Yes | Yes | Yes |
| Bus-master access control | `ACCESSCTRL` | GTZC | GTZC | SPU |
| Embassy HAL | `embassy-rp` 0.10 | `embassy-stm32` 0.6 | `embassy-stm32` 0.6 | `embassy-nrf` 0.11 |
| On-chip Ethernet MAC | No | **Yes, 10/100, with a working `eth` example** | No | No |
| Peripheral budget | Ample | Ample | Ample | Adequate |
| SP 800-90B entropy cert | **None** | **E163** (Open for Reuse, PUD published), E283 | **E11** (Open for Reuse), E116, E135, E158 | **None** |
| CAVP algorithm certs | Not found in the searches we could run (unconfirmed — see §4) | **A1012 / A1070** (Cortex-M33 crypto library), **A6437** (Secure Manager, H573) | Covered by A1012/A1070 | Not found in the searches we could run (unconfirmed — see §4) |
| probe-rs support | Yes | First-class | First-class | First-class |
| Our status | Reference/bench target | **Recommended** | Strong alternate | Viable, weakest compliance story |

### The STM32H5 changes the board, not the code

The H5's on-chip 10/100 Ethernet MAC — with a working `eth` example in `embassy-stm32` — means
adopting it **deletes the W5500 entirely**. That removes a part from the BOM, frees an SPI bus, and
frees the CS/INT/RST GPIO trio. Because the swap happens **below** `embassy-net` at the
driver-trait seam, **nothing above the network stack changes**: the broker pump, the DDS/SPDP
publisher, DHCP, SLAAC, and every socket in `lib-net` are written against `embassy_net::Stack` and
never see the driver. This is the single largest architectural simplification available on the
candidate list, and it is a *hardware* simplification bought with a *software* no-op.

### The STM32U5 trade

The U5 has the strongest entropy-certificate position of the four (four SP 800-90B certificates,
one Open for Reuse) and is the ultra-low-power family. It has **no** on-chip Ethernet MAC, so it
keeps the W5500 and the SPI leg. If your product is power-budget-driven rather than
Ethernet-driven, the U5 is the better of the two ST families; otherwise the H5's MAC wins.

---

## 4. Why FIPS points at ST

For a US government or defense end customer, the cryptographic-validation path usually decides the
part before engineering preference gets a vote. Findings below were checked against NIST CMVP,
CAVP, and ESV listings on **2026-07-26**, and re-verified on **2026-07-27**; certificate status
changes over time, so re-check the live NIST listings before relying on any of it.

**Entropy (SP 800-90B, NIST ESV).** An entropy source validation is the hardest piece to obtain and
the one you cannot manufacture in software.

- **STM32H5 — E163 and E283.** E163 is marked *Open for Reuse* with a published Public Use Document,
  which is the condition under which a downstream module can cite an existing entropy validation
  rather than running its own ESV campaign.
- **STM32U5 — E11 (Open for Reuse), E116, E135, E158.**
- **RP2350 — none.**
- **nRF5340 — none.**

**Algorithm implementations (CAVP).** ST holds **A1012** and **A1070** for its Cortex-M33
cryptographic library, and **A6437** for the Secure Manager on the STM32H573. No CAVP certificates
were found for the RP2350 or nRF5340 **in the searches we were able to run** — but treat that
absence as unconfirmed, not verified. NIST's CAVP `validation-search` page is a client-side/
JS-gated form that does not return filtered results to a plain URL fetch: a known-positive control
query (Vendor=STMicroelectronics, which certainly has entries) came back as a blank, unpopulated
form, so the tool could not be shown to work for either positives or negatives here. Re-check
RP2350 and nRF5340 CAVP status through the live browser UI before relying on this negative. This is
unlike the ESV negative above (control query returned 16 real STMicroelectronics rows, then each
negative query returned the genuine empty-state "No certificates match the search criteria") and
the CMVP negative below, both of which passed a working control query and are properly confirmed
absences.

**The limit of the claim — state this to your certifier, not around it.** **No CMVP module
certificate exists for any of these four MCUs.** Chip choice alone cannot produce a "FIPS
validated" claim for your product. What ST's position buys is materially narrower and materially
real: a reusable, already-validated entropy source and pre-existing algorithm certificates, which
remove the two longest-lead items from a module validation you would still have to pursue yourself.
Anyone telling you the silicon makes you FIPS-validated is wrong; anyone telling you the silicon is
irrelevant is also wrong.

**Recommendation basis.** The STM32H563/H573 is the recommended target because it is the only
candidate that wins on *both* axes independently: the strongest available FIPS-adjacent evidence
(E163 Open for Reuse + A1012/A1070 + A6437 on the H573), **and** the on-chip MAC that removes the
W5500 from the design. Neither reason depends on the other.

---

## 5. What a port costs

This is where the seam discipline in this codebase pays for itself. The work decomposes into four
packages, and the first thing to note is how much is **not** in any of them.

### What does not change

The following crates contain **zero** references to any concrete HAL type. This was verified by
searching the tree, not by reading the manifests' claims:

`lib-net` · `lib-io` · `lib-canonical` · `lib-mapping` · `lib-tms` · `lib-security` · `impl-app` ·
`spec-app` · `bin-mcu-platform`

`lib-io` and `lib-net` mention `embassy-rp` only inside comments that assert their own
chip-independence — for example `lib-net/src/embassy/wiznet/mod.rs:10`, *"It names NO `embassy-rp`
type."* The generic UART seam (`lib_io::EmbassyUart<U, D, T>`) is bounded on `embedded-io-async`
traits and, per its own module doc, is deliberately *embassy-version-free*: it names no `embassy-*`
crate at all. **The protocol parsing, the canonical model, the TMS mapping, the MQTT broker, the
DDS publisher, the security seams, and the entire policy loop are chip-generic by construction and
port for free.**

### WP1 — The board layer (the actual work)

Of **79** Rust source files under `crates/bin/mcu-rp2350/ns/src/`, exactly **20** name `embassy_rp`.
They are:

| Group | Files | What they do |
|---|---|---|
| Boot helpers | `boot_seplos.rs`, `boot_w5500.rs`, `boot_mesh.rs`, `boot_i2c.rs`, `boot_ui.rs`, `boot_neopixel.rs` | Bring up one peripheral each, hand back a trait object |
| Composition root | `main.rs` (incl. `bind_interrupts!` at `main.rs:196-222`), `core1.rs` | Peripheral claim, IRQ binding, task spawn |
| Type aliases | 10 files under `types/rp2350_*.rs` | Name the concrete `embassy_rp` types the seams are instantiated with |
| UART bridge | `impl_buffered_uart_adapter.rs`, `types/buffered_uart_adapter.rs` | Bridges `embedded-io-async` 0.6 ↔ 0.7 across the HAL's version pin |

The `types/rp2350_*.rs` files are one-line type aliases. The real intellectual work is the six
`boot_*.rs` helpers and `bind_interrupts!` — and their structure is the same on any HAL: claim a
peripheral, set a config struct, wrap it in the trait the rest of the system consumes. Expect this
package to be a **rewrite of six small files and one macro invocation**, not a re-architecture.

The UART bridge is worth calling out as an artifact of a *version* mismatch, not a *chip* mismatch:
embassy-rp 0.10's `BufferedUart` implements `embedded-io-async` **0.7** while `lib-io` is bounded on
**0.6**. Whether a target HAL needs the same shim depends on its own trait-version pin, and it may
simply disappear.

### WP2 — Memory and linker layout

`ns/memory.x` and `s/memory.x` encode the S/NS split by address. On RP2350 that is:

```
S  FLASH : 0x10000000 .. 0x10020000  (128 KiB, last 4 KiB = NSC veneer subregion)
NS FLASH : 0x10020000 .. 0x10800000  (8064 KiB)
S  RAM   : 0x20000000 .. 0x20010000  (64 KiB)
NS RAM   : 0x20010000 .. 0x20080000  (448 KiB)
```

Every boundary is 4 KiB-aligned, far coarser than the **32-byte SAU/MPU minimum granularity, which
is an ARMv8-M architectural constant** — so the split *shape* transfers directly to any ARMv8-M
part; only the base addresses and lengths change. What is genuinely RP2350-specific is the
`.start_block` / IMAGE_DEF placement the RP2350 boot ROM requires — on an ST or Nordic part that
section disappears and is replaced by that vendor's own option-byte / UICR secure-boundary
configuration.

### WP3 — Security layer (mostly free)

**The SAU programming, the PMSAv8 MPU region setup, and the `bxns` non-secure jump are ARMv8-M
architectural, not RP2350-specific, and travel nearly as-is.** Both files say so explicitly in
their own module docs: `s/src/sau.rs` calls the SAU register block and its `0xE000EDD0` base
*"architectural ARMv8-M facts… confident, not RP2350-specific"*, and `ns/src/mpu.rs` documents raw
PMSAv8 `RBAR`/`RLAR` access through `cortex-m` 0.7.7's `armv8m`-cfg register block. Neither is a
vendor API.

What does **not** travel is the bus-master access control: `ACCESSCTRL` on RP2350 becomes GTZC on
STM32 or the SPU on nRF. Since that layer is *currently unimplemented on RP2350 as well* (the
documented gap in §1), a port to ST arguably starts from a better position — GTZC has HAL support
that `ACCESSCTRL` presently does not.

### WP4 — Toolchain and flashing (an upgrade)

The current dev loop uses `elf2uf2-rs --deploy --serial` over BOOTSEL mass storage
(`crates/bin/mcu-rp2350/.cargo/config.toml:7`), with the config file itself noting that probe-rs is
commented out because *"this Stage-1 skeleton has no debug-probe/JTAG bring-up story yet."*
Production imaging goes through `picotool` for UF2 conversion and IMAGE_DEF signing, and the
`justfile` recipes deliberately refuse to hard-code flag names because RP2350 multi-image layout is
picotool-version-sensitive.

Moving to ST or Nordic replaces both with **probe-rs**, for which both vendors are first-class
targets. That is a straightforward improvement: real SWD flashing and debugging, RTT, and no
BOOTSEL button in the manufacturing flow. The target triple `thumbv8m.main-none-eabihf` is
**unchanged** — every candidate is Cortex-M33 with FPU.

### Summary of port cost

| Package | Scope | Character |
|---|---|---|
| WP1 board layer | 6 boot helpers + `bind_interrupts!` + 10 type aliases | Rewrite, mechanical, well-bounded |
| WP2 memory layout | 2 `memory.x` files + boot-metadata section | Address arithmetic + vendor secure-boundary config |
| WP3 security | SAU / MPU / `bxns` transfer; bus access control is new | Mostly free; one genuinely new piece |
| WP4 toolchain | `elf2uf2-rs`/`picotool` → probe-rs | Net improvement |
| Everything else | 9 crates, all application logic | **No change** |

---

## 6. Honest limits

The portability described above is real, and it is bounded. Both halves of that sentence matter.

**The ceiling is a three-vendor ecosystem.** You are not choosing from the universe of Cortex-M33
parts. You are choosing from ST, Nordic, and Raspberry Pi, because those are the only vendors with
a production-grade Embassy HAL. A part from TI, NXP, Microchip, Ambiq, Silicon Labs, or Infineon
would require either writing a HAL or abandoning the async runtime this firmware is built on —
both of which are new-product-scale efforts, not ports. If your program has a mandated vendor
outside those three, engage us before committing to this codebase.

**Our RP2350 numbers are stale.** The footprint measurement is dated 2026-07-24 and predates the
Feather board move and four feature legs. We know flash is not the constraint and RAM is. We do
not currently know the RAM figure for the current tree. Re-measurement is an owed action, not a
completed one.

**TrustZone-M is designed in, not switched on.** The `veneers` feature is off by default, the
secure-world image is described in its own source as a demo scaffold, the secure image is not yet
demonstrated to be boot-ROM-loadable on real RP2350 hardware (`s/memory.x` documents this as an
open item), and bus-level DMA access control is unimplemented. A customer building a security
argument on the TrustZone boundary is building on an architecture we have designed and partially
implemented — not on a shipped, tested boundary.

**We have not built or booted this on ST or Nordic silicon.** The port analysis is derived from
reading our own code's coupling surface and from published HAL capabilities. It is a
well-evidenced estimate. It is not a completed port, and the first bring-up on new silicon always
surfaces something the analysis did not.

**FIPS: the ceiling is documented above and we will not move it.** No CMVP module certificate
exists for any candidate MCU. ST's entropy and algorithm certificates shorten a validation effort
you would still own end to end.

**What we would need from you to firm this up.** A target part number, your Ethernet topology (on-
chip MAC versus W5500), whether the mesh-radio and OLED legs are in scope, and your certification
target. With those, the WP1–WP4 estimate above becomes a schedule instead of a shape.

---

## See also

- [Reference bill of materials](reference-bom.md) — the parts behind the peripheral budget in §1.
- [MCU firmware: RP2350 build & flash](../deployment/mcu-firmware.md) — the current build and
  flashing procedure.
- [RP2350 footprint & perf](../perf/rp2350-footprint-perf.md) — the measurement this chapter cites,
  including the Methodology commands to re-run it.
- [ADR 0010 — MCU runtime: Zephyr → Embassy](../decisions/0010-mcu-runtime-embassy.md) — why
  Embassy is the seam.
- [ADR 0011 — MCU security boundary (RP2350)](../decisions/0011-mcu-security-boundary.md) — the
  S/NS split rationale.
- [ARM TrustZone deep dive](../security/arm-trustzone-deep-dive.md) — the security-boundary detail.
- [RS-485 wiring & transceiver prerequisites](../deployment/rs485-wiring.md) — the DE/RE pin
  requirement in §1.
