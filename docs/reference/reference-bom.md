<p class="eyebrow">Sentient METEOR · Reference · Bill of Materials</p>

# Reference-design BoM & indicative cost

This chapter is for an integrator who intends to **lay out their own board** from our reference
design rather than buy hardware from us. It answers three questions: *what parts does the
reference design actually use*, *what do they cost in the quantities you are likely to build*,
and *which of those choices are still open decisions you should make yourself*.

It is deliberately not a quotation. Everything below is an **indicative range**, and several
lines are flagged UNVERIFIED because we could not establish the fact from a primary source. That
is the house convention in this repo and we have not relaxed it for a customer-facing page: a
number you can trace is worth more than a number that looks tidy.

## Pricing basis — read this before quoting anything

| | |
|---|---|
| **Source** | DigiKey published web pricing, single distributor |
| **Date fetched** | **2026-07-26** |
| **Break points** | Qty 1 / 100 / 1000, as published. Qty-1000 figures marked `~` are the published price at the nearest break at or below 1000. |
| **What is included** | Bare distributor unit cost of the components listed |
| **What is NOT included** | PCB fabrication, assembly, test, scrap/yield, tariffs and duties, freight, non-recurring engineering, certification, tooling, enclosure, cabling, and the MCU itself |

Four caveats that materially change the number you should carry in a budget:

1. **Prices move.** Semiconductor distributor pricing on this class of part moved by
   double-digit percentages within a single year on multiple occasions in the recent past. A
   figure from a single day is a starting point for a negotiation, not a cost baseline.
2. **Single source, single date.** We did not cross-check Mouser, Arrow, Avnet, or a broker.
   One of the parts below (see [Lifecycle traps](#lifecycle-and-part-number-traps)) has a
   *contradictory lifecycle status* between two distributors, which is exactly the failure mode
   single-sourcing your pricing research produces.
3. **Lead time is unpriced schedule risk.** Several lines carry 32-, 37-, and 52-week quoted
   lead times. None of that is reflected in the dollar figures. If you need boards in six
   months, the lead time — not the unit price — is your binding constraint, and the cheapest
   part on a line is frequently the one with the worst lead time.
4. **The MCU is excluded.** The roll-up is the supporting BoM around the microcontroller. Add
   your MCU and its support components separately.

## Rolled-up indicative cost

Excluding the MCU, using a **pre-certified LoRaWAN module** for the RF leg and a **10/100
magjack** (not gigabit):

| Build quantity | Indicative BoM cost |
|---|---|
| 100 | **~$26 – $35** |
| 1000 | **~$23 – $30** |

Treat that as a **band, not arithmetic**. It is not the sum of the table midpoints below — it
includes a lumped allowance for bulk passives, decoupling, and the parts a real layout adds that
a reference schematic does not enumerate. The width of the band is dominated by two choices you
have not made yet: which RF module, and fixed versus pluggable field connectors.

If you use the Meshtastic prototype radio instead, the RF line changes and the TAA posture
changes with it — see [The RF decision](#the-rf-decision) below.

## Functional blocks

The TAA-posture column is a **starting point for your own determination**, not a determination.
See [TAA applicability](../compliance/taa-applicability.md) for what the rule actually says, when
it is invoked, and why a distributor listing cannot close it.

### Ethernet

The reference design uses the WIZnet **W5500** — a hardwired TCP/IP offload controller on SPI,
which is why the firmware's network leg is small enough to live on a microcontroller at all.

| Function | Part | Qty 1 | Qty 100 | Qty 1000 | Origin / TAA posture | Notes |
|---|---|---|---|---|---|---|
| Ethernet controller | WIZnet **W5500** | $3.78 | $2.36 | ~$2.20 | Korea (designated country); WIZnet is **fabless** — actual wafer fab UNVERIFIED | What our firmware drives today |
| Ethernet controller (successor) | WIZnet **W6100** | not priced | — | — | Same vendor posture | Dual-stack IPv4/IPv6. Worth evaluating if your program mandates IPv6 |
| Magjack, 10/100 | Bel Fuse **SI-60062-F** | $5.65 | $4.08 | ~$3.60 | UNVERIFIED | <span class="pill pill--warn">32-week lead</span> |
| Magjack, gigabit | Pulse **JK0654219NL** | $19.74 | ~$16.78 | ~$12.70 | UNVERIFIED | See the specification trap below |

**Specification trap — do not fit a gigabit magjack.** The W5500 is a 10/100 device. A gigabit
magjack costs roughly **4x** the 10/100 part at qty 100 and buys bandwidth the controller
physically cannot use. This is a live trap because "gigabit" reads as a safe default on a
requirements sheet. If your program genuinely requires gigabit at the connector, the W5500 is
the wrong controller and the whole network block needs re-architecting — the magjack is not the
place to fix it.

**On IPv6.** The W6100 is WIZnet's dual-stack successor. Our own compliance work treats IPv6 as
required for the DDS/RTPS transport layer, so if you are targeting that profile, price the W6100
and validate driver availability before committing to the W5500 footprint. We have not priced or
bring-up-tested the W6100 — that is a gap, stated as one.

### RS-485 (the BMS link)

This is the battery-management link. It is slow, and the transceiver choice should reflect that
rather than reaching for the fastest part available.

| Function | Part | Qty 1 | Qty 100 | Qty 1000 | Origin / TAA posture | Notes |
|---|---|---|---|---|---|---|
| Transceiver, 3.3 V | TI **THVD1450DR** | $2.04 | $1.22 | ~$1.04 | TI (US-headquartered, multi-site fab) | 3–5.5 V supply; wide-range default |
| Transceiver, 3.3 V, slew-limited | TI **SN65HVD72DR** | $2.75 | $1.67 | $1.46 | TI | 250 kbps slew-limited — **better electrical fit for a slow BMS link** (reduced emissions, better cable tolerance) |
| Transceiver, low-cost | MaxLinear **SP3485EN-L/TR** | $0.95 | $0.541 | $0.470 | UNVERIFIED | <span class="pill pill--warn">38-week lead, no stock</span> — cheapest and least available |
| Isolated transceiver | TI **ISO3082DWR** | $3.95 | $2.47 | $2.19 | TI | <span class="pill pill--warn">NRND</span> — do not baseline a new design on it |
| Isolated transceiver | TI **ISO1412DWR** | $4.93 | $3.13 | ~$2.98 | TI | Active, but **full-duplex and not pin-compatible** with the half-duplex isolated parts |

**Do not fit SN65HVD3082E.** It is a **5 V** part. It appears adjacent to the 3.3 V family in
search results and in a lot of copied reference schematics, and it is not suitable for a
3.3 V-only board. This is the single most common substitution error on this line.

**Isolation is a real decision, and it roughly doubles to triples this line.** A galvanically
isolated transceiver protects the logic side from ground-potential differences across a battery
string — which is exactly the environment a BMS link lives in. Whether you need it depends on
your cabling topology and how far the BMS sits from the board. We are not making that call for
you. What we can tell you is the cost shape: non-isolated is ~$1.20–1.70 at qty 100, isolated is
~$2.50–3.15, and the isolated parts carry the lifecycle and pin-compatibility complications
above.

<span class="pill pill--warn">UNVERIFIED</span> We could **not** confirm whether the isolated
families' logic side operates from a 3.3 V-only rail. They are documented as 5 V families and we
could not confirm the `VCC1` minimum from a primary source. If the logic side requires 5 V, an
isolated design may force you to add a 5 V rail you would not otherwise need — which is a board
cost and a power-budget cost, not just a component swap. **Verify against the datasheet before
you commit the schematic.**

### Current monitoring

The reference firmware reads adapter-rail current for self-telemetry (the W5500 and radio
rails). Our bench wiring uses the three-channel part.

| Function | Part | Qty 1 | Qty 100 | Qty 1000 | Origin / TAA posture | Notes |
|---|---|---|---|---|---|---|
| 3-channel monitor | TI **INA3221AIRGVR** | $3.47 | $2.16 | ~$2.02 | TI | What the bench uses |
| Single-channel, ±1% | TI **INA219AIDR** | $2.12 | $1.28 | ~$1.06 | TI | |
| Single-channel, ±0.02% | TI **INA226AIDGSR** | $2.66 | $1.62 | ~$1.36 | TI | Precision tier |

**The three-channel part is cheaper than three single-channel parts.** At qty 100: **$2.16 for
the INA3221 versus $3.84 for three INA219s.** It also consumes one I2C address instead of three
and one footprint instead of three. If you need three rails monitored, the multi-channel part
wins on cost, board area, and bus addressing simultaneously — an unusually clean decision.

**Do not order INA3221AI-RGVT.** The `-RGVT` sibling suffix is **OBSOLETE**. The suffix encodes
packaging (tape-and-reel variant), which makes it very easy to paste the wrong one into a BoM
line and discover the problem at purchasing rather than at design review.

### Power

| Function | Part | Qty 1 | Qty 100 | Qty 1000 | Origin / TAA posture | Notes |
|---|---|---|---|---|---|---|
| Buck regulator | TI **TPS54331DR** | $1.58 | $0.93 | $0.81–0.86 | TI | Primary step-down |
| LDO, 3.3 V | Microchip **MCP1700T-3302E/TT** | $0.51 | $0.40 | ~$0.39 | Microchip (US-headquartered) | Low-noise / low-quiescent rail |

Power is a small fraction of this BoM (~$1.30 at qty 100 for both parts). It is not a cost lever
worth optimizing; it is a line to get electrically right and then stop thinking about.

### Protection, timing, termination, connectors

| Function | Part | Qty 1 | Qty 100 | Qty 1000 | Origin / TAA posture | Notes |
|---|---|---|---|---|---|---|
| 25 MHz crystal | Abracon **ABM8G-25MHz** | $0.44 | $0.329 | $0.286 | UNVERIFIED | W5500 reference clock |
| RS-485 ESD array | Bourns **CDSOT23-SM712** | $1.26 | $0.521 | $0.367 | UNVERIFIED | Asymmetric 12 V / 7 V — **purpose-built for RS-485**, not a generic TVS |
| Ethernet ESD array | TI **TPD4E1U06DCKR** | $0.93 | $0.374 | $0.258 | TI | |
| 120 Ω termination | Yageo **RC0603FR-07120RL** | $0.10 | $0.0122 | $0.0066 | UNVERIFIED | RS-485 bus termination |
| Field connector, fixed | On Shore **OSTTA024163** | $0.72 | $0.38 | $0.287 | UNVERIFIED | Screw terminal, wire lands directly on the board |
| Field connector, pluggable | Phoenix **1803581** | $3.14 | — | — | UNVERIFIED | **Plus its mating header 1803280 — budget two line items, not one** |
| Lumped misc allowance | — | — | $3.25–9.90 | $2.35–7.70 | — | Bulk passives, decoupling, the parts a schematic does not enumerate |

**Use the purpose-built RS-485 ESD array.** The CDSOT23-SM712's asymmetric 12 V / 7 V clamp
matches the RS-485 common-mode range. A symmetric general-purpose TVS either clips valid
signalling or fails to protect — this is a part where the $0.52 "expensive" option is the
correct one.

## What actually drives the cost

Three findings from assembling this BoM, ranked by how much they move the number and by how
often the intuition is wrong.

### 1. The RF module dominates — but the spread is certification, not silicon

The RF module is **40–50% of the priced BoM**. That much is expected. What is not expected is
*why* the spread between options is roughly 2x.

The spread is **not** driven by radio performance or silicon cost. It is driven by whether the
module arrives **pre-certified**. A pre-certified module transfers FCC, ISED, and AS/NZS
approvals to your product; bare silicon does not, and you bear the full intentional-radiator
certification NRE yourself — test-lab time, design iterations to pass, and schedule.

So the honest framing is: **the expensive module is buying down your certification NRE, not
buying you a better radio.** Describing this line as "the module is expensive" is misleading and
will push a program toward bare silicon that then discovers a five-figure certification bill and
a schedule slip. At qty 100, the difference between a pre-certified module and bare silicon is a
few dollars a unit; a single FCC intentional-radiator certification campaign is not.

The crossover is a volume question you should compute with your own numbers: certification NRE
amortized across your build quantity, versus the per-unit premium. At 100 units, pre-certified
almost always wins. At very high volume it may not.

### 2. The magjack does *not* dominate — unless you over-spec it

At $3.60–4.08, the 10/100 magjack is a mid-sized line, not a driver. It becomes one only if
somebody specifies **gigabit** — at $12.70–16.78 it jumps to roughly the size of the RF module,
for bandwidth the W5500 cannot use. This is a live specification trap, not a hypothetical: it is
the kind of thing that gets written into a requirements document by someone reasoning about
future-proofing rather than about the controller's actual PHY.

### 3. The surprise second driver is **connectors**, not passives

Generic passives — resistors, caps, the crystal, termination — total only about **$0.40–1.40**
of the misc block. Meanwhile a **single pluggable connector pair** ($3.14 for the Phoenix 1803581
plus a separate mating header) can exceed the cost of every passive on the board combined.

Fixed screw terminal versus pluggable is a **>10x per-connector swing** ($0.38 versus $3.14+ at
qty 100). It is also a genuine engineering trade, not just a cost one: pluggable connectors make
field service, cable pre-assembly, and board swap-out dramatically easier, which is worth real
money in a deployed system where a technician's time is the expensive resource. We are flagging
the magnitude, not the answer — but you should make this choice deliberately, because it is
easy to make it by accident while copying a reference schematic.

## The RF decision

This is the largest open decision in the design, and it is **yours, not ours**. The repo
owner's direction, stated plainly:

> **Meshtastic as a first prototype, LoRaWAN or another RF (customer directed) for long term.**

So: the mesh leg as it exists today is **prototype-grade**, deliberately. The long-term RF choice
is a customer decision. We present both branches with evidence and do not recommend one.

### Critical: LoRaWAN is not Meshtastic

<span class="pill pill--warn">read this before substituting a module</span>

The two are not interchangeable, and the pre-certified TAA-defensible modules in the table below
are **not drop-in substitutes** for what our firmware speaks today.

**What our firmware speaks.** The mesh leg emits **Meshtastic protobuf framing over a plain
UART** — 38400 baud, 8N1, `0x94 0xC3` magic bytes, big-endian `u16` length, then a hand-rolled
`ToRadio` protobuf message. The radio module is a separate board running Meshtastic firmware; our
MCU talks to it as a serial peripheral. (Implementation: `crates/lib-net/src/mesh/` and
`crates/bin/mcu-rp2350/ns/src/boot_mesh.rs`.)

**What the pre-certified modules speak.** The Ezurio, Murata, and Microchip modules below run
**LoRaWAN** stacks. Adopting one has two consequences you must budget for:

1. **Our mesh codec is rewritten.** The protobuf encoder, the stream framer, and the transport
   glue are Meshtastic-specific. A LoRaWAN module needs a LoRaWAN join procedure, device/network
   session keys, an uplink/downlink port model, and a network-server integration. That is new
   firmware, not a configuration change.
2. **Mesh routing is lost.** LoRaWAN is **star-to-gateway**. Every device talks to a gateway;
   devices do not relay for each other. Meshtastic is a mesh — nodes relay, and coverage extends
   past any single node's radio horizon. If the reason you wanted this radio was
   infrastructure-free multi-hop coverage, LoRaWAN does not provide it and no amount of firmware
   work will make it.

These are architectural differences, not integration details. Decide the RF architecture before
you decide the module.

### Branch A — prototype (Meshtastic, what we used)

| Part | Role | TAA posture |
|---|---|---|
| Heltec **WiFi LoRa 32 V4** | The module our bench uses | **China-origin — TAA-disqualifying if TAA is invoked.** Heltec is Chengdu; the on-board ESP32 is Espressif, Shanghai |

This is fine as a **bench article**. It is not a production path for a TAA-covered procurement.

**The wider ecosystem does not fix this.** The Meshtastic hardware ecosystem is
China-dominated — Heltec, LilyGo, Seeed, and RAKwireless are the mainstream vendors, and
switching between them does not change the origin picture.

One naming trap worth calling out: **RAKwireless is Shenzhen**, despite a Hong-Kong-style
"Limited" corporate name that reads as though it might sit outside mainland China. The naming
invites false confidence in a compliance review. Verify origin from the manufacturer, not from
the company name.

### Branch B — long-term, TAA-defensible (LoRaWAN, pre-certified)

| Part | Qty 1 | Qty 100 | Qty 1000 | Origin | Certification | Notes |
|---|---|---|---|---|---|---|
| Ezurio **453-00139R** (RM1262) | $12.82 | UNVERIFIED | $10.475 | **US HQ, Akron OH** | **FCC + ISED + AS/NZS pre-certified** | Approvals transfer to you — real money against your certification budget |
| Murata **LBAA0QB1SJ-296** (Type 1SJ) | $15.67 | $11.96 | $10.48 | Japan (designated country) | Pre-certified | <span class="pill pill--warn">37-week lead</span> |
| Microchip **RN2903A-I/RM105** | $17.40 | — | — | US | Pre-certified | <span class="pill pill--warn">Lifecycle disputed</span> — DigiKey lists Active, Mouser lists NRND |
| Murata **CMWX1ZZABZ-091** | — | — | — | Japan | Pre-certified | <span class="pill pill--warn">LAST TIME BUY 2027-03-31</span> — **do not baseline a new design on it** |

The Ezurio part is the strongest combination of US headquarters, three-region pre-certification,
and price. We are noting that, not recommending it — your TAA determination, your certification
regions, and your supply agreements may all point elsewhere.

### Branch C — bare silicon (you certify)

| Part | Qty 1 | Qty 100 | Qty 1000 | Origin | Notes |
|---|---|---|---|---|---|
| ST **STM32WLE5CCU6** | $8.92 | $5.87 | $5.32 | CN / FR / IT sites — all designated countries | <span class="pill pill--warn">52-week lead</span>. MCU + LoRa radio on one die |
| Semtech **SX1262** | $8.53 | $6.47 | $5.70 | US HQ; **fabless** | Radio only |

Neither carries certification. **You bear the full FCC / IC intentional-radiator NRE**, plus the
schedule to get through it and the risk of a re-spin if you fail. The per-unit saving versus a
pre-certified module is real (~$4–6 at qty 100) but small next to a certification campaign at
low-to-moderate volume. See the cost-driver discussion above.

### Branch D — a custom Meshtastic board

Worth knowing, because it is the branch that keeps mesh routing **and** improves TAA posture:

**Meshtastic supports nRF52840 + SX1262 as a platform.** A customer-designed board around that
pair could run Meshtastic firmware — keeping the mesh topology and our existing codec unchanged —
on silicon with a more defensible origin story than the Heltec/Espressif combination.

The costs of this branch are honest ones: you design and maintain a **board variant**, you get it
accepted as a Meshtastic hardware target (or maintain a fork), and you bear **your own FCC
certification** exactly as in Branch C. It buys mesh + posture at the price of NRE and ongoing
firmware maintenance.

## Lifecycle and part-number traps

Consolidated, because each of these will cost somebody a purchase order or a board spin:

| Trap | What goes wrong |
|---|---|
| **SN65HVD3082E** on a 3.3 V board | It is a **5 V** part. Sits next to the 3.3 V family in search results |
| **INA3221AI-RGVT** | **OBSOLETE** package suffix; the correct part is `-RGVR` |
| **ISO3082DWR** | **NRND** — will not survive a long production life |
| **ISO1412DWR** as an ISO3082 replacement | **Full-duplex, not pin-compatible** — it is not a footprint swap |
| **CMWX1ZZABZ-091** | **LAST TIME BUY 2027-03-31** |
| **RN2903A-I/RM105** | **Lifecycle disputed between distributors** (DigiKey Active, Mouser NRND) |
| **Gigabit magjack** with a W5500 | ~4x cost for bandwidth the controller cannot use |
| **Pluggable connector** priced without its mating header | Phoenix 1803581 needs 1803280 — **two line items** |
| **"RAKwireless Limited"** read as non-mainland | Shenzhen. The name invites a false compliance conclusion |

## Lead-time exposure

None of this is in the dollar figures. It is often the binding constraint.

| Line | Quoted lead |
|---|---|
| Bel Fuse SI-60062-F magjack | 32 weeks |
| MaxLinear SP3485EN-L/TR | 38 weeks, no stock |
| Murata LBAA0QB1SJ-296 | 37 weeks |
| ST STM32WLE5CCU6 | 52 weeks |

Note the pattern: on both the RS-485 line and the RF line, **the cheapest option carries the
worst availability**. If your program has a delivery date, price second sources on these lines
before you commit the footprint, and prefer parts where a pin-compatible alternate exists.

## UNVERIFIED — what we could not establish

Stated explicitly rather than smoothed over, because a compliance file built on assumptions is
worse than one with acknowledged gaps.

<span class="pill pill--warn">material</span> **7 of 7 DigiKey product pages published no country
of origin.** Not one of the pages we consulted stated a country of origin for the part. This is
the central limitation of this entire document as a compliance input: **distributor listings
cannot support a TAA compliance file.** Country-of-origin claims must come from the
manufacturer — a certificate of origin, a declaration letter, or a contractual flow-down — not
from a web listing or from a vendor's headquarters address.

Other open items:

- **Fabless vendors are not their fabs.** WIZnet (Korea) and Semtech (US HQ) are both **fabless**.
  Where the wafers are actually fabricated and where the parts are assembled and tested is
  UNVERIFIED, and for TAA purposes the substantial-transformation location is what matters, not
  the brand's address.
- **Isolated RS-485 logic-side voltage.** Whether the ISO3082 / ISO1412 families' logic side runs
  from a 3.3 V-only rail is unconfirmed; they are documented as 5 V families and the `VCC1`
  minimum was not established from a primary source. If it is 5 V, an isolated design forces a
  5 V rail.
- **Ezurio 453-00139R qty-100 price** — not published; we have qty 1 and qty 1000 only.
- **W6100 pricing and bring-up** — not priced, not tested with our firmware.
- **Bel Fuse, Abracon, Bourns, Yageo, On Shore, Phoenix, MaxLinear origins** — UNVERIFIED, per
  the point above.
- **Microchip RN2903A-I/RM105 lifecycle** — genuinely contradictory between two distributors.
  Resolve with Microchip directly before designing it in.
- **Roll-up completeness.** The band assumes a 10/100 magjack, a pre-certified LoRaWAN module,
  and excludes the MCU. It carries a lumped misc allowance rather than a fully-enumerated passive
  count, because a real layout's passive count depends on the layout.

## Related chapters

- [TAA applicability](../compliance/taa-applicability.md) — what the rule is, when it is invoked,
  and what actually satisfies it. Read it before treating any origin note above as a
  determination.
- [Rust API docs](README.md) — the crate-level reference, including `lib-net` (the mesh codec and
  network layer) and `lib-io` (the `SerialTransport` seam the RS-485 and mesh legs both sit on).
