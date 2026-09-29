<p class="eyebrow">Sentient METEOR · Compliance · Trade Agreements Act</p>

# TAA applicability carve-out

**This is a map, not a claim.** Nothing here asserts that Sentient METEOR is TAA-compliant, and
nothing here is a certification. TAA applicability is **customer- and contract-directed**: it
attaches when a specific procurement invokes it, and until a contract does, there is nothing to
comply with. What this document does is answer the question a systems integrator actually needs
answered *before* signing: **if your end customer's contract invokes TAA, which parts of this
system does it touch, what does the determination turn on, and what evidence would you have to
produce?**

> **Planning aid, not legal advice.** The binding determination is made by the contracting
> officer, informed by trade counsel and — where the origin question is genuinely close — by a
> CBP country-of-origin ruling request. Treat everything below as scoping input for that
> conversation, not as a substitute for it. Citations are given so a lawyer can check our
> reading rather than take it on faith.

Two framing facts shape everything that follows:

1. **The reference design is a design, not a shipped end product.** You intend to lay out your
   own PCB. That means the substantial-transformation event that decides country of origin
   happens at *your* contract manufacturer, on *your* PCBA — not at ours. Our exposure surface is
   the firmware, the reference schematic, and the component selections we recommend. Your
   exposure surface is the assembled board.
2. **TAA tests the end product, not each line of the BoM.** This is the single most useful thing
   in this document and it is under-appreciated. See [The end-product rule](#the-end-product-rule-you-do-not-certify-each-resistor)
   below. Most of the component-level anxiety that surrounds TAA is misplaced effort.

## 1. What TAA is, and when it applies

The Trade Agreements Act (19 U.S.C. §2501 et seq.), implemented at **FAR Subpart 25.4** with
clause **FAR 52.225-5**, requires that an end product delivered to the Government be a **US-made
end product** or a **designated country end product**. When the TAA applies, the Buy American Act
is *waived* for that acquisition.

The trigger is a dollar threshold. **Below** it, TAA does not apply and the **Buy American Act**
governs instead — and BAA is a materially different test (a component-cost/domestic-content
calculation, not a substantial-transformation test), with a different and generally *harder*
evidentiary burden for a board with globally sourced silicon. Getting the regime right is
therefore prior to any origin work at all.

| | Trade Agreements Act | Buy American Act |
|---|---|---|
| Applies | At or above the supply threshold, when the contract invokes FAR 52.225-5 | Below the TAA threshold |
| Test | Substantial transformation of the **end product** in the US or a designated country | Domestic end product: manufactured in the US **and** a domestic-content percentage of components |
| Practical burden | One origin determination at the PCBA level | Component-by-component cost accounting up the BoM |
| Component origin | Generally irrelevant if the end product is substantially transformed | Directly relevant — it is the arithmetic |

### The threshold

The supply-contract threshold is **$174,000** per the Federal Register rule effective
**2026-03-13**. <span class="pill pill--warn">unconfirmed</span> A second source consulted during
this review reported **$193,000** for the same period. We have not resolved which is correct and
we are not going to guess — **verify the current figure against FAR 25.402(b) / the live FAR text
before relying on it.** Thresholds also differ by contract type (supplies vs. services vs.
construction) and are re-set on a biennial cycle, so a number copied from a document of this
vintage is stale by default.

### Designated countries — the ones that matter here

FAR 25.003 defines "designated country." Fetched verbatim from the current FAR text, the entries
relevant to semiconductor sourcing are:

| Designated (TAA-eligible) | Basis |
|---|---|
| **"the Separate Customs Territory of Taiwan, Penghu, Kinmen and Matsu (Chinese Taipei)"** | WTO GPA |
| **Japan** | WTO GPA |
| **Korea, Republic of** | WTO GPA **and** the KORUS free-trade agreement |
| Singapore | WTO GPA + FTA |
| All EU member states, the United Kingdom, Switzerland, Norway, Canada, Mexico, Australia, Israel | WTO GPA / FTA |

**Taiwan is designated.** This matters more than any other single fact in this document: the
dominant foundries for the class of parts in this design are TSMC fabs in Taiwan, and a
Taiwan-fabbed part clears TAA *on origin grounds*.

**Not designated:** **China, Malaysia, Thailand, the Philippines, Vietnam.** Note what those five
have in common — they are the major **assembly, packaging, and test** hubs. That is precisely why
the next section is the crux.

## 2. The origin rule: origin follows the fab, not assembly or test

For semiconductors, CBP has held consistently that the country of origin is where the **wafer is
fabricated** (front-end processing) — not where the die is packaged, not where it is tested, and
not where it was designed.

- **CBP HQ H341544** (2024-09-25). Chinese design house; wafer fabricated in Japan or Taiwan;
  packaged in China. CBP held: *"The country of origin ... will be either Japan or Taiwan,
  depending on where the front-end processing occurs."* The design origin was addressed and
  explicitly held irrelevant.
- **CBP HQ H309802** (2020-05-27). Wafer fabricated in Italy (approximately 95% of total process
  time); packaged in China (about four days). CBP held the origin to be **Italy** and the part
  *"not a product of China."*

Three corollaries follow, and each one kills a common false alarm:

- **Vendor headquarters is not country of origin.** A part from a Chinese-headquartered vendor
  fabbed at TSMC Taiwan takes **Taiwan** origin.
- **Assembly and test site is not country of origin.** "Packaged in Malaysia" or "tested in the
  Philippines" does not make a part Malaysian or Philippine, and does not by itself make it
  non-compliant.
- **Design origin is not country of origin.** Symmetrically, a US- or UK-designed part fabbed in
  a non-designated country does **not** inherit its designer's nationality.

The practical consequence for this design: a UK-designed, Taiwan-fabbed MCU is a **Taiwan** end
product for origin purposes and clears. A part whose only known fact is its assembly site tells
you nothing yet — you have to go get the fab.

### The end-product rule: you do not certify each resistor

TAA tests the **substantially transformed end product**. Components may originate anywhere,
including non-designated countries, **provided** the final assembly and test constitutes
substantial transformation performed in the US or a designated country. The end product must
emerge as a new and different article of commerce with a name, character, or use distinct from
its inputs.

What does **not** qualify, per the FAR definition and CBP practice: *"labeling, repackaging,
simple testing, or minor assembly."*

So the determination lives at the **PCBA / contract-manufacturer level**. A populated,
programmed, and functionally tested board built in the US or a designated country is a strong
substantial-transformation posture even with a globally sourced BoM. This is the lever that makes
the problem tractable — and it is the lever you control, because you are building the board.

### But self-certification is not self-executing

Do not read the end-product rule as permission to accept a supplier's word for it. **GAO,
*HPI Federal LLC*, B-422583 (2024-08-09)** held it unreasonable for an agency to accept a
certification that a product was merely *"assembled in"* a compliant country, on the ground that
**"assembly alone may not constitute substantial transformation."**

The operational reading: a supplier letter that says "packaged in Malaysia" or "assembled in
Mexico" is **not a usable origin claim**. What is needed is enough process detail to support the
conclusion — what operations occur, where, and why they amount to substantial transformation.
Plan for a protest-grade record, not a checkbox.

## 3. The carve-out map

Each row is a delivered or recommended aspect of the system. "Touched" means TAA would reach it
if a contract invokes the clause and this aspect is part of the deliverable.

| System aspect | TAA touches it? | What the determination turns on | Evidence needed |
|---|---|---|---|
| **The assembled adapter PCBA** (your board, from our reference design) | **Yes — this is the determination.** | Whether your CM's operations constitute substantial transformation, and whether that CM is in the US or a designated country. Populate + program + functional test is a strong posture; kitting or "final QA only" is not. | CM's process description and site country; a substantial-transformation memo (counsel-reviewed); the CM's own TAA certification, backed by process detail rather than an "assembled in" assertion |
| **MCU — RP2350** (Raspberry Pi Ltd; Adafruit RP2350 Feather is the reference carrier) | Yes, if silicon origin is questioned below the end-product level | **Wafer fab country**, not Raspberry Pi Ltd's UK domicile and not the package/test site. <span class="pill pill--warn">unconfirmed</span> We have not obtained a written fab-site statement from the vendor. | Vendor-issued COO/TAA certificate naming the fab country, per MPN |
| **Ethernet controller — W5500** (WIZnet) | Yes, same basis | Fab country. WIZnet's Korean domicile is *suggestive* — Korea is designated on a dual basis (WTO GPA + KORUS) — but domicile is not the test. <span class="pill pill--warn">unconfirmed</span> | Vendor COO/TAA certificate per MPN |
| **LoRa radio module — Heltec V4 (Espressif SoC + Semtech transceiver)** | **Yes — highest-risk line in the BoM** | Module-level: where the SoC wafer is fabbed *and* where the module is assembled. A Chinese-domiciled module vendor is not automatically disqualifying under the origin rule, but it is where a contracting officer will look first. **Not** a Section 889 issue — see §4. | Fab-site statement for the SoC; module assembly site; a documented substitution path if the answer is non-designated and the end-product argument is not accepted |
| **Power-monitor / sensor / display ICs** (INA3221 class, SSD1306-class OLED controller, PMIC, passives) | Indirectly only | Subsumed by the end-product determination in nearly all cases. Becomes a first-order question only if the contract invokes **BAA** instead, where component cost is the arithmetic. | Distributor traceability with lot/date codes; COO certificates only if BAA applies or a CO asks specifically |
| **Host SBC — Raspberry Pi 4-64 / Pi 5**, if delivered as part of the system | Yes | The SBC is a COTS **end product in its own right** with its own origin. Its board-level manufacture (UK, for the Pi 5 line) is the relevant fact, not the origin of the SoC on it. <span class="pill pill--warn">unconfirmed</span> — get it in writing. | Manufacturer TAA statement for the specific SKU and revision |
| **Firmware and OS image** — the Rust crates, the `bin/mcu-rp2350` MCU image, the Yocto Pi image | Yes, as a separately deliverable end product | For software, origin generally follows where it was **developed and compiled** into the delivered form. Our build is CI-produced and reproducible from a signed pipeline (see [Release lifecycle](../deployment/release-lifecycle.md)). <span class="pill pill--warn">unconfirmed</span> — whether the CO treats firmware as a distinct end product or as part of the board is contract-specific. | Statement of development and build location; the signed-release provenance chain; CycloneDX SBOM already emitted per target |
| **Open-source dependencies** (crates.io graph, Yocto layers) | **No, not as a TAA origin question** | Upstream contributor nationality is not a TAA country-of-origin input. Do not let anyone turn this into one. It *is* a supply-chain-risk question under FASCSA and SBOM/attestation obligations — a different regime, §4. | SBOM (already generated), `cargo-audit` / `cargo-deny` evidence (already generated), FASCSA exclusion-list screening |
| **Observed third-party devices** — Seplos BMS, inverter | Only if you resell them | If customer-furnished or separately procured by the end user, they are outside your delivery and outside your certification. If you resell, each is an end product with its own determination — and BMS-class hardware is commonly Chinese-manufactured. | If resold: per-SKU TAA statement from that manufacturer |
| **The reference design itself** — schematics, BoM, layout guidance | Probably not | A design package is technical data, not a supply end product. <span class="pill pill--warn">unconfirmed</span> — data deliverables carry their own clause set (DFARS 252.227 rights in technical data), which is a separate analysis we have not done. | Out of scope for TAA; flag to counsel under the data-rights clauses instead |
| **Cloud / hosted services**, if any are contracted | Possibly, under a different rule | Services have their own designated-country treatment under FAR 25.4 distinct from supplies, with a different threshold. <span class="pill pill--warn">unconfirmed</span> — not analyzed here. | Service-provider place-of-performance representation |

## 4. Adjacent regimes that gate independently

Each of these is a **separate** carve-out with its own test. Passing TAA does not satisfy any of
them, and failing one is not a TAA finding. Keeping them distinct is not pedantry — conflating
them is how a bid gets an unnecessary finding.

### NDAA FY2023 §5949 — prohibited-foundry semiconductors

Bans covered semiconductor products and services from **SMIC, CXMT, YMTC** and their affiliates.
Two things make this different from Section 889:

- It **reaches components inside products** — not just the end item you sell.
- It carries a **72-hour disclosure duty** on discovery of covered parts in the supply chain.

Statutory effective date is **2027-12-23**. The implementing FAR rule was **proposed 2026-02-17**
and was **not final as of 2026-06-03**; re-check its status, because the final rule will fix the
flow-down and diligence mechanics that the proposed rule only sketches.

**Current posture:** no candidate vendor in this design uses those fabs. That is a statement
about the parts we selected, not a certified representation — it is a starting position for
diligence, and it needs to be re-verified against the final rule and against any part
substitutions your layout introduces.

### Section 889 — covered telecommunications equipment

A **named-entity** test: Huawei, ZTE, Hytera, Hikvision, Dahua, and their subsidiaries and
affiliates.

**Section 889 does not reach an MCU selection.** State this precisely, because overclaiming here
is exactly the kind of error a contracting officer catches and it costs credibility on
everything else in the package. Specifically: **Espressif and Heltec are not on the Covered
List.** Their exposure in this design is a **TAA country-of-origin** question and nothing more.
Do not describe a Heltec module as a "Section 889 risk" — it is not one, and saying so invites
scrutiny of every other claim you make.

### DFARS 252.246-7007 and -7008 — counterfeit electronic parts

Both at their **JAN 2023** revisions.

| Clause | Applies to | What it requires |
|---|---|---|
| **252.246-7007** Contractor Counterfeit Electronic Part Detection and Avoidance System | **Only Cost Accounting Standards-covered contractors.** Check whether you are one before building to it. | A documented detection-and-avoidance system, with traceability *"from the original manufacturer to product acceptance by the Government."* |
| **252.246-7008** Sources of Electronic Parts | Broader applicability | A **three-tier sourcing hierarchy**: original manufacturers, their authorized suppliers, and suppliers that obtain parts exclusively from those, **first**; other sources only with notification and additional inspection/test obligations. |

Note what these clauses do **not** say: neither cites SAE **AS5553** or **AS6081** by number.
Those standards are the common industry implementation, not the regulatory text — do not
represent them as the requirement.

Practical read for this program: the tier-1 sourcing hierarchy in -7008 aligns with what you want
for TAA evidence anyway. **Buy from authorized distribution, keep lot and date codes.** One
sourcing discipline serves both regimes.

### FASCSA exclusion orders

Federal Acquisition Supply Chain Security Act exclusion orders are a **live list** at
`sam.gov/supplychainorders`. It changes. **Re-check it immediately before finalizing a BoM and
again before contract award** — a screen performed months earlier is not a screen.

## 5. Evidence checklist

Two hard truths about evidence-gathering, learned the tedious way.

**Distributor listings are not origin evidence.** In a spot check during this review, **7 of 7
DigiKey product pages published no country of origin at all.** Distributor catalog data,
marketing pages, and datasheet headers are not origin records and will not survive a challenge.

**"Assembled in X" is not an origin claim** — see *HPI Federal* above.

What actually constitutes a usable record:

| Evidence item | Source | Why it is required |
|---|---|---|
| **Vendor-issued COO/TAA certificate, per MPN** | The component manufacturer directly (not the distributor, not the datasheet) | The only document that speaks to fab location with the manufacturer's authority behind it |
| **Authorized-distributor traceability with lot and date codes** | Franchised distribution channel | Ties the parts you actually received to the certified MPN; simultaneously satisfies DFARS -7008 tier-1 sourcing |
| **Process detail supporting substantial transformation** | Your contract manufacturer | What operations happen at the named site, and why they exceed "simple testing or minor assembly." This is what *HPI Federal* says a bare certification lacks. |
| **CM site country and TAA representation** | Contract manufacturer | The end-product determination hangs on this one fact |
| **SBOM + build provenance for firmware** | Already generated by CI — CycloneDX per target, `cargo-audit` / `cargo-deny` evidence, CMS-signed release chain | Supports the software end-product story and covers FASCSA/SBOM obligations that TAA does not reach |
| **FASCSA screening record, dated** | `sam.gov/supplychainorders` | The list is live; an undated screen is not a screen |
| **CBP ruling request** — only if the origin question is genuinely close | CBP, via counsel | Converts an argued position into a binding one. Slow. Worth it only when a real determination is contested. |

Sequencing note: **do not start collecting per-MPN certificates until §6's first question is
answered.** If the contract invokes TAA and your CM's substantial-transformation posture is
strong, most of this component-level work is unnecessary. If it invokes BAA, the work is
different and larger. Doing it in the wrong order is the most common way to waste a quarter.

## 6. Open questions

Stated plainly, because the repo's convention is to flag gaps rather than smooth them over — and
a customer-facing document earns trust the same way.

1. **Does the contract invoke TAA, BAA, or the DFARS counterfeit-parts clauses — and which ones?**
   <span class="pill pill--warn">highest leverage</span> This is the single question that
   determines whether component-level origin work is needed at all. Under TAA with a strong
   substantial-transformation posture at the PCBA, most of it is not. Under BAA, component cost
   accounting is unavoidable. Under DFARS -7007, you additionally need a documented
   detection-and-avoidance *system* — but only if you are CAS-covered. **Ask the contracting
   officer for the clause list before spending a dollar on diligence.**
2. **The threshold discrepancy is unresolved.** $174,000 (Federal Register, effective
   2026-03-13) versus $193,000 from a second source. Verify against the live FAR before relying
   on either. The answer decides question 1's first branch.
3. **No vendor fab-site statements have been obtained** for any part in this design. Every origin
   row in the §3 table marked <span class="pill pill--warn">unconfirmed</span> is unconfirmed
   because we asked no vendor. We are not going to infer a fab from a corporate domicile — that
   is exactly the inference CBP HQ H341544 rejects.
4. **NDAA §5949's implementing FAR rule is not final.** The flow-down and diligence mechanics
   will be set by the final rule, not the proposed one. Re-check before it takes effect
   2027-12-23.
5. **Firmware's treatment as a distinct end product is contract-specific** and we have not seen a
   CO take a position on it for a system of this shape.
6. **Technical-data deliverables** (the reference design package) sit under a different clause
   family (DFARS 252.227 rights in technical data) that this document does not analyze at all.
7. **Services, if any are in scope**, have their own designated-country rules and their own
   threshold under FAR 25.4. Not analyzed.

---

See also: [Release lifecycle & promotion](../deployment/release-lifecycle.md) for the signed
build-provenance chain that backs the firmware evidence row, and
[Attestation pipeline](attestation.md) for how machine-generated compliance evidence is produced
and gated in CI.
