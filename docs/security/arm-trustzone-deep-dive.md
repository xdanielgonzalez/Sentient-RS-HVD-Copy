# Fact-Check and Corrected Rewrite: ARM TrustZone Technical Deep-Dive (Sections 1–4, 6–9, 11)

## Abstract

This document fact-checks a technical deep-dive on ARM TrustZone and issues a corrected rewrite of its architecture-bearing sections (§1–§4, §6–§9, §11), verified against primary ARM documentation and peer-reviewed literature. The source material is architecturally sound. Every load-bearing hardware claim is confirmed: the EL0–EL3 model, SCR_EL3.NS, AxPROT[1], the TZC-400 nine-region controller, the GIC interrupt groups, SAU/IDAU strictest-wins attribution, the SG/BXNS/FNC_RETURN=0xFEFFFFFF gateway mechanics, and the RME four-world model with its Granule Protection Table. Three areas were stale and are corrected here: OP-TEE now implements GlobalPlatform TEE Internal Core API v1.3.1 (not v1.1.x), the current Trusted Firmware-A line is 2.15 with RME support since 2.6, and Arm CCA has moved from "no silicon" to shipping cores (Neoverse V3, with Linux Realm guest support merged in kernel 6.13 and host/KVM in 6.14). The central finding is a framing discipline that every section must preserve: TrustZone is a hardware isolation boundary, not a cryptographic proof; secure boot (TBBR) is load-time integrity, not runtime attestation; and the isolation boundary does not stop microarchitectural side channels, a limitation ARM explicitly places outside its threat model and that the Cachegrab (2019) and BUSted (2023–2024) results demonstrate in practice.

## Summary of Findings

1. **TrustZone-A core model is accurately described.** EL3 is always Secure. SCR_EL3.NS selects the security state of EL0/EL1/EL2. All transitions route through EL3 via SMC. Confirmed against ARM's *TrustZone for AArch64* guide.
2. **Secure EL2 = ARMv8.4-A (FEAT_SEL2), optional, enabled by SCR_EL3.EEL2, hosts the Secure Partition Manager.** Confirmed.
3. **TrustZone-M attribution is "strictest wins."** SAU (≤8 regions, Secure-writable) and IDAU (vendor-fixed) each present an attribution. The more-secure result wins (S > NSC > NS). The processor resets into Secure state. Confirmed.
4. **Software-stack version claims were the weakest area and are now corrected.** OP-TEE Internal Core API v1.3.1, TF-A 2.15, GlobalPlatform spec v1.4 is the latest published.
5. **Performance section needs a decomposition, not just the 2019 µs figure.** A bare SMC round-trip (context save/restore, no service) is on the order of a few hundred cycles on modern A-profile silicon. Cortex-M33 base interrupt latency is **12 cycles**, but ARM publishes **no** per-instruction cycle table for SG/BXNS.
6. **Attack-surface claims check out**, with one correction. The QSEE key-extraction CVE is **CVE-2018-11976**, disclosed by NCC Group's Keegan Ryan in 2019 (CCS 2019), and BUSted was presented at **Black Hat Asia 2023** and published at **IEEE S&P 2024**.
7. **CCA section must be de-staled.** The RME four-world model is correct, but "no silicon" is obsolete as of 2026.

## Detailed Analysis

Each section below carries a verification ledger (per-claim verdict and source) followed by a corrected rewrite. Section numbers match the source deep-dive.

-----

### §1: TrustZone-A Processor Architecture

**Verification ledger**

|#   |Claim                                                                                                                            |Verdict                                                                                                                                                     |Source                                                            |
|----|---------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------------------|
|1.1 |Four Exception Levels EL0–EL3                                                                                                    |**Confirmed**                                                                                                                                               |ARM *TrustZone for AArch64* (developer.arm.com / cs.wm.edu mirror)|
|1.2 |SCR_EL3.NS determines security state at EL0/EL1/EL2; EL3 always Secure                                                           |**Confirmed.** "For EL0/1/2, the SCR_EL3.NS bit. EL3 is always in Secure state."                                                                           |ARM *TrustZone for Armv8-A* PDF                                   |
|1.3 |All security-state transitions pass through EL3; SMC routes to EL3 dispatcher                                                    |**Confirmed**                                                                                                                                               |ARM *TrustZone for AArch64*                                       |
|1.4 |Secure Monitor saves/restores context; a small number of registers are banked by security state; ICC_BPR1_EL1 is a banked example|**Confirmed.** "A small number of registers are banked by Security state... the core automatically uses the copy that belongs to the current Security state."|ARM *TrustZone for AArch64*                                       |
|1.5 |Two physical address spaces; NS bit in Stage-1 table entries; no NS bit in Stage-2 entries                                       |**Confirmed.** "Unlike the Stage 1 tables, there is no NS bit in the Stage 2 table entries."                                                               |ARM *TrustZone for Armv8-A*                                       |
|1.6 |Caches physically tagged with security state; TLB entries carry NS/NSTID                                                         |**Confirmed** (NSTID / physically-tagged)                                                                                                                   |ARM guide; corroborated Amacher & Schiavoni 2019                  |
|1.7 |AxPROT[1] on AXI/ACE: 0=Secure, 1=Non-secure; HNONSEC on AHB5                                                                    |**Confirmed.** "the AxPROT[1] signal is used to specify which address space... 0 indicates Secure and 1 indicates Non-secure."                               |ARM *TrustZone for Armv8-A*; vlsiip.com for HNONSEC               |
|1.8 |TZASC / TZC-400 supports up to 9 regions; Secure-only config registers                                                          |**Confirmed.** "An example of a TZASC is the Arm TZC-400, which supports up to nine regions."                                                              |ARM *TrustZone for Armv8-A*; TZC-400 TRM                          |
|1.9 |GIC groups: Group 0 (Secure FIQ), Secure Group 1, Non-secure Group 1; GICD_IGROUPR/IGRPMODR Secure-only; SCR_EL3.FIQ/IRQ routing |**Confirmed**                                                                                                                                               |ARM GICv3 Architecture Spec; TF-A Interrupt Framework             |
|1.10|SMMU/DMA masters must be TrustZone-aware or gated (NIC-400 register override / SMMU NS bit)                                      |**Confirmed**                                                                                                                                               |ARM *TrustZone for Armv8-A*                                       |

**Rewritten §1:**

TrustZone-A partitions an Armv8-A/Armv9-A core into two security states, Secure and Non-secure, that are orthogonal to the four Exception Levels (EL0 application, EL1 OS, EL2 hypervisor, EL3 monitor). The state is not a property of the code. It is a property of the hardware at the moment of execution. `SCR_EL3.NS` selects the security state of EL0/EL1/EL2. EL3 is always Secure regardless of `SCR_EL3.NS`. Every transition between worlds funnels through EL3: Non-secure software issues an `SMC`, the core traps to EL3, an SMC dispatcher (Trusted Firmware-A) decides the target, flips `SCR_EL3.NS`, and returns to the destination world.

The monitor's isolation guarantee is *state save/restore discipline*, not automatic hardware banking. Only a small set of registers are banked by security state (the core keeps two copies and selects automatically; `ICC_BPR1_EL1` is one such banked register). The general-purpose and most system registers are **not** banked. EL3 firmware must save and restore them, and must scrub secure state before returning to Non-secure, or it leaks. This is a software-enforced boundary sitting on top of a hardware primitive.

Memory isolation is enforced by a second, independent mechanism: two physical address spaces. A Non-secure master can only ever emit Non-secure accesses. The core ties the bus NS signal to 1 for all Non-secure transactions. In Stage-1 translation tables the NS bit tags each descriptor's target address space (Stage-2 has no NS bit; the whole IPA space maps to one PAS controlled by a register bit). On AMBA AXI/ACE the `AxPROT[1]` signal carries this off-core: 0 = Secure, 1 = Non-secure. The AHB5 equivalent is `HNONSEC`. Caches are physically tagged with the security state bit and TLB entries carry the NS/NSTID tag, so a Secure and Non-secure access to the same address are distinct cache/TLB entries.

Off-core, a TrustZone Address Space Controller (for example the CoreLink **TZC-400**, which supports **up to nine regions**: one background region 0 plus eight programmable) filters transactions against per-region security settings. Its configuration registers are writable only by Secure masters. Interrupts are partitioned by the GIC into three groups: **Group 0** (Secure, delivered as FIQ, typically handled at EL3), **Secure Group 1**, and **Non-secure Group 1**. `GICD_IGROUPR`/`GICD_IGRPMODR` (and their GICv3 redistributor equivalents) are Secure-only, and `SCR_EL3.FIQ`/`SCR_EL3.IRQ` control whether FIQ/IRQ route to EL3. Non-processor bus masters (GPUs, DMA engines) are the classic hole: they must either be TrustZone-aware, be gated by interconnect security registers (for example NIC-400), or be routed through an SMMU that applies the NS bit like a Secure-state MMU. A DMA master that can be programmed by Non-secure software to emit Secure accesses breaks the model.

**Boundary note.** §1 describes an *isolation* boundary. It says nothing about proving to a remote party what is running in the Secure world. That is attestation, which base TrustZone-A does not provide.

-----

### §2: TrustZone-M Microcontroller Variant

**Verification ledger**

|#  |Claim                                                                                                                               |Verdict                                                                                                                        |Source                                                   |
|---|------------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------|
|2.1|ARMv8-M Security Extensions; attribution by memory map; processor resets in Secure state                                            |**Confirmed**                                                                                                                  |ARM *TrustZone for Armv8-M* v2.1; ST AN5421              |
|2.2|SAU: up to 8 programmable regions, Secure-writable only; IDAU vendor-fixed (up to 256 regions)                                      |**Confirmed**                                                                                                                  |ARM *Armv8-M Security Extension* UG; SoK arXiv 2401.15289|
|2.3|Attribution precedence: strictest-wins (S > NSC > NS); undefined regions default Secure                                             |**Confirmed.** "memory Security attribution is determined by the unit which has the strictest Security attribution specified."|ARM *Armv8-M Security Extension* UG; ST AN5421           |
|2.4|SG only meaningful in NSC region; switches to Secure and clears LSB of return address                                               |**Confirmed**                                                                                                                  |ARM *Secure software guidelines for Armv8-M*             |
|2.5|BXNS/BLXNS for Secure→Non-secure; FNC_RETURN = 0xFEFFFFFF written to LR on BLXNS                                                    |**Confirmed**                                                                                                                  |ret2ns paper (IEEE 2023); ARM docs                       |
|2.6|SP banking (MSP_S/PSP_S/MSP_NS/PSP_NS); IPSR not banked                                                                             |**Confirmed**                                                                                                                  |ret2ns paper                                             |
|2.7|NVIC_ITNS sets per-interrupt target security; automatic Secure register clearing on NS interrupt during Secure execution; AIRCR.PRIS|**Confirmed** (mechanism and PRIS mechanism confirmed)                                                                          |SoK arXiv 2401.15289; ARM ARMv8-M overview               |
|2.8|MPC/PPC/TZSC/TZIC bus-fabric components                                                                                             |**Confirmed** (vendor-specific naming)                                                                                         |Silabs AN1374; NXP LPC55S6x                              |

**Rewritten §2:**

TrustZone for Armv8-M targets Cortex-M microcontrollers (M23, M33, M35P, M55, M85). It drops the EL model and the secure monitor entirely. There is no EL3, no `SMC`, and no monitor software. Instead, security state is a function of the *address being executed*, resolved by memory attribution hardware. This makes world-switching a branch, not a firmware call. The design goal is low latency and determinism for real-time code.

Two units attribute every address. The **SAU** (Security Attribution Unit) is inside the core, has **up to eight programmable regions**, and is writable only in Secure state. The **IDAU** (Implementation-Defined Attribution Unit) is a vendor-fixed, external signal source (typically driven from address bits) and can define far more regions (implementations expose up to 256). Each unit labels an address Secure, Non-secure-Callable (NSC), or Non-secure. **The strictest attribution wins** (Secure > NSC > Non-secure), and any address not covered defaults to Secure. On reset the whole map is Secure and the processor boots in Secure state from a Secure vector, and the vendor's Secure boot code then opens up Non-secure regions. The practical consequence: software can only ever *tighten* security relative to the vendor's IDAU floor via the SAU. It cannot make an IDAU-Secure region Non-secure.

Crossing from Non-secure to Secure is gated by the **SG** (Secure Gateway) instruction. SG only does something when it is (a) fetched from an NSC region and (b) the first instruction of the branch target. Then it switches to Secure state and clears the LSB of the return address in LR. Anywhere else it is a no-op, which is why the NSC attribute must be applied *only* to genuine entry veneers: a data byte pattern that happens to equal the SG opcode in ordinary Secure memory must never be callable, or Non-secure code could jump straight into the middle of Secure code. Returning Secure→Non-secure uses **BXNS/BLXNS**. `BLXNS` pushes return state to the Secure stack and loads LR with the reserved sentinel **FNC_RETURN = 0xFEFFFFFF**, whose later use triggers the hardware unstack. Stack pointers are banked four ways (MSP_S, PSP_S, MSP_NS, PSP_NS). The IPSR is not banked.

Interrupts are partitioned by `NVIC_ITNS`, which assigns each interrupt to Secure or Non-secure. When a Non-secure interrupt preempts Secure execution, the hardware automatically pushes and **clears the Secure register contents** before entering the Non-secure handler, so secrets in registers do not leak across the boundary. This is the M-profile analogue of the A-profile monitor's context scrubbing, done in hardware. `AIRCR.PRIS` lets Secure software deprioritize all Non-secure interrupts below Secure ones. System-level isolation for buses and peripherals is completed by vendor fabric blocks: memory protection controllers (MPC), peripheral protection controllers (PPC), and TrustZone security/illegal-access controllers (TZSC/TZIC in ST parlance). Their naming is vendor-specific but their function is uniform: enforce the S/NS attribution on the bus for memory, peripherals, and DMA.

**Boundary note.** TrustZone-M does not encrypt Secure memory and provides no hardware attestation. Its guarantee is on-die access control.

-----

### §3: Software Architecture Stack (TF-A / OP-TEE / Secure Boot)

**Verification ledger**

|#  |Claim                                                                                                                                                                                           |Verdict                                        |Source                                                              |
|---|------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------|--------------------------------------------------------------------|
|3.1|TF-A stages BL1 (root/ROM), BL2 (loader), BL31 (EL3 runtime), BL32 (Secure-EL1 payload/TEE), BL33 (Non-secure bootloader)                                                                       |**Confirmed**                                  |TF-A *Firmware Design* 2.15                                         |
|3.2|BL31 is the permanent EL3 resident runtime                                                                                                                                                      |**Confirmed.** "BL31 is the runtime resident." |TF-A docs; Neoverse RD docs                                         |
|3.3|SMC calling convention = SMCCC (DEN0028)                                                                                                                                                        |**Confirmed** (ID); PSCI = DEN0022             |TF-A docs                                                           |
|3.4|OP-TEE implements GlobalPlatform TEE Internal Core API **v1.1.x** and Client API v1.0                                                                                                           |**CORRECTED → v1.3.1**                         |optee.readthedocs.io/latest (v1.3.1); older mirrors still say v1.1.x|
|3.5|Latest published GlobalPlatform Internal Core API spec                                                                                                                                          |**Updated → v1.4** (GPD_SPE_010)               |globalplatform.org                                                  |
|3.6|tee-supplicant RPC model; TAs run at S-EL0                                                                                                                                                      |**Confirmed**                                  |optee.readthedocs.io                                                |
|3.7|Dynamic (REE-FS) TAs individually signed (RSA PKCS#1 v1.5 SHA-256 default), verified by OP-TEE core; pseudo/static TAs compiled into core → covered by core secure boot, not individually signed|**Confirmed**                                  |optee.readthedocs.io (Trusted Applications)                         |
|3.8|TBBR chain of trust; root of trust in OTP/eFuse                                                                                                                                                 |**Confirmed**                                  |TF-A TRUSTED_BOARD_BOOT / TBBR                                      |

**Rewritten §3:**

On A-profile, the reference secure stack is **Trusted Firmware-A**. Boot proceeds in stages, each verifying the next. **BL1** is the immutable boot ROM and root of trust. **BL2** is the trusted loader that authenticates and loads the remaining images. **BL31** is the **permanent EL3 runtime** (the secure monitor) that lives resident for the life of the system and services `SMC`s. **BL32** is the optional Secure-EL1 payload (a Trusted OS such as OP-TEE, managed by a Secure Payload Dispatcher). **BL33** is the Non-secure bootloader (U-Boot/UEFI) that eventually boots the rich OS. On platforms with a separate secure boot processor, TF-A can reset straight into BL31 (`RESET_TO_BL31`), skipping BL1/BL2. The current TF-A release line is **2.15**. SMC dispatch follows the **SMC Calling Convention (SMCCC, DEN0028)**. Power coordination uses PSCI (DEN0022).

The Trusted OS layer is typically **OP-TEE**. Correcting a stale claim: OP-TEE implements the GlobalPlatform **TEE Internal Core API v1.3.1** (the API exposed to Trusted Applications) and the **TEE Client API v1.0** (the Normal-world side). Older documentation mirrors, including the trustedfirmware.org project page and some vendor wikis, still cite "v1.1.x," and those are out of date. The authoritative optee.readthedocs.io "latest" states v1.3.1. Separately, the latest *published* GlobalPlatform Internal Core API specification is **v1.4** (GPD_SPE_010). OP-TEE tracks a subset of it, so "OP-TEE implements v1.3.1" and "GlobalPlatform has published v1.4" are both true and not in conflict.

OP-TEE's runtime model: Trusted Applications run in secure user mode (**S-EL0**), one privilege level below the OP-TEE core (S-EL1). The core in turn is below EL3/TF-A. A Normal-world client calls through the TEE Client API into a Linux driver, which issues an `SMC`, and the OP-TEE dispatcher schedules the target TA. When a TA needs Normal-world services (filesystem, sockets, REE time), OP-TEE calls back out via RPC to the **tee-supplicant** daemon running in the REE. This RPC dependency is a design point worth stating plainly: persistent secure storage is physically stored in the untrusted REE filesystem, protected by encryption and a hash tree. OP-TEE detects tampering and rollback of individual objects but relies on the REE to actually hold the bits.

TA trust is enforced two different ways, and the distinction is load-bearing. **Dynamic (REE-FS) TAs** live in the untrusted Normal-world filesystem as signed ELF files. `tee-supplicant` passes them in and the OP-TEE core verifies the signature before mapping. The default algorithm is `TEE_ALG_RSASSA_PKCS1_V1_5_SHA256` (RSA PKCS#1 v1.5 with SHA-256), against the key baked into the OP-TEE core at build time, with optional subkey chains and optional AES-GCM encryption. **Pseudo (static) TAs** are compiled directly into the OP-TEE core binary and run at S-EL1. They carry no individual signature and are instead covered by the secure-boot verification of the core blob itself. **Early TAs** are user-mode S-EL0 TAs linked into the core blob so they are available before the REE filesystem exists, also covered by core secure boot.

Secure boot itself is the **TBBR** (Trusted Board Boot Requirements) chain of trust: BL1 verifies BL2 verifies BL3x, each link authenticated with certificates whose root public-key hash is fused into on-die **OTP/eFuse**. This is the anchor everything else hangs from.

**Boundary note, the single most important framing in the report.** TBBR secure boot is **load-time integrity**. It proves that the images loaded at boot matched signatures chained to a fused root key *at the moment they were loaded*. It is **not runtime attestation**: it produces no fresh, nonce-bound, remotely verifiable evidence of what is executing *now*, and it does not detect post-load runtime compromise. Conflating "secure boot passed" with "the Secure world is currently trustworthy and can prove it to a remote party" is the classic error this report must avoid.

-----

### §4: TrustZone-A vs TrustZone-M

**Verification ledger.** All comparative claims **Confirmed** against ARM *TrustZone for Armv8-A* / *Armv8-M* guides and the Lauterbach and Silabs references.

**Rewritten §4:**

|Dimension                  |TrustZone-A (Armv8-A/v9-A)                   |TrustZone-M (Armv8-M)                           |
|---------------------------|---------------------------------------------|------------------------------------------------|
|Privilege model            |EL0–EL3; state orthogonal to EL              |Thread/Handler modes; no EL3                    |
|What defines security state|`SCR_EL3.NS` (runtime, set by monitor)       |Address being executed (SAU/IDAU attribution)   |
|World switch               |`SMC` → EL3 monitor saves/restores context   |`SG`/`BXNS`/`BLXNS` branch; hardware auto-stacks|
|Switch cost                |Hundreds of cycles (context save/restore)    |Near branch-latency; no monitor                 |
|Memory isolation           |MMU NS bit + TZASC + physically-tagged caches|SAU/IDAU + MPC/PPC bus fabric                   |
|Secure memory encryption   |No (isolation only)                          |No                                              |
|Attestation                |Not in base TrustZone                        |Not in base TrustZone                           |
|Typical secure OS          |OP-TEE / TF-A                                |TF-M (bare-metal secure partition)              |
|Target                     |Application processors (phones, servers)     |Microcontrollers (IoT, real-time)               |

The two share a philosophy, hardware-enforced two-world isolation on a shared core, time-sliced, and differ in mechanism. A-profile pays for a rich, virtualizable secure world (a full Trusted OS at S-EL1, optional Secure EL2) with a monitor round-trip on every switch. M-profile trades that flexibility for switch latency measured in branch cycles and no monitor at all, at the cost of a smaller, flatter secure world. Neither encrypts secure memory. Neither provides attestation in its base form. Both are isolation boundaries.

-----

### §6: Performance / World-Switch Cost

**Verification ledger**

|#  |Claim                                                                                |Verdict                                                           |Source                                                   |
|---|-------------------------------------------------------------------------------------|------------------------------------------------------------------|---------------------------------------------------------|
|6.1|~1–5 µs SMC overhead on Cortex-A53 from Amacher & Schiavoni 2019 (arXiv 1906.09799)  |**Confirmed as the origin; now stale as the *only* datapoint**    |arXiv 1906.09799                                         |
|6.2|Modern bare SMC round-trip (save/restore, no service) ≈ 421 cycles / 182 instructions|**Confirmed as reported by OpenCCA** (see caveat)                 |Bertschi & Shinde (ETH Zurich), OpenCCA, arXiv 2506.05129|
|6.3|Cortex-M33 base interrupt latency = 12 cycles                                        |**Confirmed**                                                     |*Definitive Guide to Cortex-M23/M33* (Yiu)               |
|6.4|ARM documents specific SG/BXNS transition cycle counts                               |**REFUTED.** ARM publishes no per-instruction cycle table for M33 |Cortex-M33 TRM; ST community                             |
|6.5|Secure exception frames larger than Non-secure (extra stacking cost)                 |**Confirmed**                                                     |ARM developer blog; ARMv8-M overview                     |
|6.6|CCA Realm VM exit costs ~1.5 µs extra vs vanilla KVM                                 |**Confirmed** (verbatim)                                          |USENIX *Enabling Realms* (login2023_cca.pdf)             |

**Rewritten §6:**

The report's single figure, "~1–5 µs SMC overhead on Cortex-A53," traces to Amacher & Schiavoni's 2019 OP-TEE benchmark on a Raspberry Pi 3B (quad Cortex-A53 @ 1.2 GHz, arXiv 1906.09799). That number is real but should not stand alone. It bundles OP-TEE software cost with the hardware switch and is board-specific. State the components instead.

The hardware floor is the monitor round-trip: a bare `SMC` into EL3 that saves and restores world context *without invoking any service*. Bertschi and Shinde (ETH Zurich), in the OpenCCA framework paper (arXiv:2506.05129, June 2025, measured on a Radxa Rock 5B / RK3588, Armv8.2-A), report such a round-trip at roughly **421 CPU cycles / 182 instructions**. (Caveat: these exact values appear in the paper's evaluation discussion. Treat them as OpenCCA's reported figure and re-confirm against the paper's evaluation table before publication.) Real SMC calls that actually enter a Trusted OS and do work cost more. The µs-scale figures in the 2019 paper are dominated by the OP-TEE call path and Linux driver, not the raw switch. For confidential-VM context, an Arm CCA Realm exit to the hypervisor costs about **1.5 µs more** than a vanilla KVM exit. The USENIX *Enabling Realms* team states verbatim, "An exit to the hypervisor is more expensive on CCA KVM, taking an extra 1.5 µs." That is why I/O-heavy workloads (millions of exits) show the largest CCA overhead ("in the worst case, overhead was 18% for MongoDB, an I/O intensive workload"), while the same paper reports "less than 8% overhead versus unmodified KVM for most workloads."

On M-profile, correct an overstated claim: **ARM does not publish a per-instruction cycle-count table for the Cortex-M33** (unlike the Cortex-M4 TRM). The one firm ARM-sourced number is **12-cycle base interrupt latency** (zero-wait-state memory, no security transition; *Definitive Guide to Cortex-M23/M33*). `SG`, `BXNS`, and `BLXNS` are branch-class instructions that are effectively single-cycle plus pipeline refill, but no official cycle figure exists for them, and any specific "SG = N cycles" claim should be flagged as not backed by an ARM TRM. What ARM *does* document is that **Secure exception stack frames are larger than Non-secure ones** (the core reserves space to stack all registers in case a Non-secure interrupt arrives during Secure execution), so a security-state-crossing interrupt costs more cycles than a same-state one, but the exact delta is implementation-defined.

**Bottom line.** Report the switch cost as a decomposition: hardware round-trip (a few hundred cycles on A-profile; branch-latency plus a larger secure frame on M-profile) plus software service cost (the µs-scale OP-TEE path). Do not report a single legacy microsecond figure.

-----

### §7: Attack Surface and Known Vulnerabilities

**Verification ledger**

|#  |Claim                                                                                                                                                  |Verdict                                                                                                                                                                                     |Source                                             |
|---|-------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------|
|7.1|NCC Group 2019 QSEE ECDSA key extraction (Cachegrab / "Hardware-Backed Heist")                                                                         |**Confirmed.** Keegan Ryan recovered a full 256-bit ECDSA key from Qualcomm's hardware-backed keystore; CVE-2018-11976; Prime+Probe + branch-predictor; Nexus 5X / Snapdragon 808; CCS 2019|nccgroup.com whitepaper; Threatpost                |
|7.2|Cache-timing Prime+Probe on TrustZone (TruSense, INFOCOM 2018)                                                                                         |**Unverifiable in this pass** (primary not retrieved); Prime+Probe on TrustZone otherwise well-documented                                                                                   |Naiwei et al.; PMU cross-world channel (PMC9571368)|
|7.3|2023 bus-interconnect arbitration side-channel on Cortex-M33 (BUSted, Black Hat Asia 2023); ARM says side channels are outside the Armv8-M threat model|**Confirmed.** Pinto & Rodrigues, Universidade do Minho; later IEEE S&P 2024 ("BUSted!!!"); ARM statement verbatim below                                                                    |blackhat.com slides; The Register                  |
|7.4|DIT introduced Armv8.4-A (A-profile) and Armv8.1-M (M-profile)                                                                                         |**Partially confirmed.** Armv8.4-A confirmed and mandatory; Armv8.1-M claim **Unverified**                                                                                                 |ARM KA005181; TF-A build options                   |
|7.5|TEE CVE history: Samsung TEEGRIS, QSEE, Trusty SMC-parsing bugs                                                                                        |**Plausible/uncorroborated in this pass**                                                                                                                                                   |n/a                                                  |
|7.6|TOCTOU on shared memory; TA type confusion                                                                                                             |**Confirmed as generic class**                                                                                                                                                              |n/a                                                  |

**Rewritten §7:**

TrustZone's boundary stops direct memory access across worlds. It does not stop inference from shared microarchitecture, and the public record proves it repeatedly.

**Cachegrab / "Hardware-Backed Heist" (NCC Group, 2019).** Keegan Ryan extracted a **full 256-bit ECDSA private key** from Qualcomm's Secure Execution Environment (QSEE) hardware-backed keystore, the exact asset the keystore exists to protect. The leak came through shared L1 data cache and the branch predictor, observed with a Prime+Probe-style trace-driven attack using the open-source Cachegrab tool. The demonstration ran on a rooted Nexus 5X (Snapdragon 808) and required Normal-world root. It is **CVE-2018-11976**, disclosed to Qualcomm in March 2018, fixed in the April 2019 Android update, published at ACM CCS 2019. The takeaway is precise: TrustZone isolated the key's *storage*, but the *computation* shared caches and predictor state with the Normal world, so a secret-dependent implementation leaked.

**BUSted (Universidade do Minho, 2023–2024).** Sandro Pinto and Cristiano Rodrigues presented the first practical microarchitectural timing side-channel on TrustZone-**M** at **Black Hat Asia 2023** ("Hand Me Your Secret, MCU!"), published at IEEE S&P 2024. The novelty: most Cortex-M cores have no cache or branch predictor, so the attack exploits **bus-interconnect arbitration**. When the CPU and a DMA master contend for a shared memory port, the interconnect prioritizes one and delays the other, and a Non-secure spy can measure that delay to infer a Secure application's secret (demonstrated recovering a PIN from a Secure keypad TA on a Cortex-M33). ARM's response is important to quote, because it defines the threat model, not because it dismisses the finding: *"The Security Extensions for the Armv8-M architecture do not claim to protect against side-channel attacks due to control flow or memory access patterns."* In other words, this is behaving as specified. Side channels are explicitly out of scope.

**Data-Independent Timing (DIT).** For A-profile, **FEAT_DIT is mandatory from Armv8.4-A** and guarantees timing-invariance for a defined set of data-processing instructions when `PSTATE.DIT` is set (ARM KA005181; enabled in TF-A via `ENABLE_FEAT_DIT`). The report's separate claim that DIT was introduced for M-profile in **Armv8.1-M** could **not be verified** against a primary ARM source in this pass and should be re-checked against the Armv8-M Architecture Reference Manual before publication rather than asserted.

**Other classes.** The report's mention of TEE CVE history (Samsung TEEGRIS, QSEE, Trusty SMC-parsing bugs) is a reasonable summary of a real pattern. The Trusted OS's SMC/parameter-parsing surface is the highest-value bug class because a parsing bug in S-EL1 code compromises the whole secure world. I could not corroborate specific named CVEs in this pass, so cite specific CVE numbers or soften to "documented classes." TOCTOU on Normal/Secure **shared memory** (the Non-secure side mutating a buffer after the Secure side validates it) and **TA type/parameter confusion** are confirmed generic classes and correctly listed. The mitigation is that Secure code must copy-then-validate out of shared memory, never validate-in-place.

**Boundary note.** Every §7 result reinforces the framing: an isolation boundary is not a side-channel-proof boundary, and ARM does not claim it is.

-----

### §8: TrustZone vs Peer TEE Technologies

**Verification ledger**

|#  |Claim                                                          |Verdict                                                                                                                            |Source                                   |
|---|---------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------|
|8.1|Intel SGX phased out on newer consumer CPUs, continues on Xeon |**Confirmed & sharpened.** Deprecated on 11th/12th-gen Core (11th-gen+ Core have no SGX at all); continues on Xeon for data-center|Intel; Wikipedia; codentium              |
|8.2|SGX EPID attestation deprecated vs DCAP/ECDSA                  |**Confirmed.** EPID EOL for production **2 April 2025**; Ice Lake-SP+ Xeon are DCAP-only                                          |Intel; Gramine docs                      |
|8.3|AMD SEV-SNP attestation via VCEK signed by AMD Secure Processor|**Confirmed**; VLEK alternative exists                                                                                             |AMD; arXiv 2406.01186                    |
|8.4|Apple Secure Enclave architecture                              |**Confirmed at high level** (dedicated coprocessor, own boot ROM, memory encryption)                                               |general/primary Apple docs not re-fetched|
|8.5|Comparison table accurate as of 2026                           |**Confirmed with corrections above**                                                                                               |n/a                                        |

**Rewritten §8:**

|TEE                     |Isolation unit        |Memory encryption  |Attestation                             |2026 status                                          |
|------------------------|----------------------|-------------------|----------------------------------------|-----------------------------------------------------|
|**TrustZone-A**         |Two worlds, whole-core|No (isolation only)|None in base; CCA adds it               |Ubiquitous on mobile/embedded                        |
|**TrustZone-M**         |Two worlds, MCU       |No                 |None                                    |Ubiquitous on Cortex-M                               |
|**Intel SGX**           |Per-process enclave   |Yes (enclave EPC)  |EPID (deprecated) → DCAP/ECDSA          |**Removed from consumer Core (11th-gen+); Xeon only**|
|**AMD SEV-SNP**         |Whole VM              |Yes (per-VM key)   |VCEK/VLEK signed by AMD Secure Processor|Shipping on EPYC                                     |
|**Arm CCA**             |Realm VM (4th world)  |Yes (Realm PAS)    |Realm + platform token, RMM-generated   |First silicon shipping (see §9)                      |
|**Apple Secure Enclave**|Dedicated coprocessor |Yes                |Vendor-internal                         |Shipping on all Apple SoCs                           |

Positioning, stated plainly: TrustZone and Apple's Secure Enclave answer "protect a small trusted service or key store on a device." SGX answered "protect a process from a compromised OS," but Intel **deprecated it on consumer processors** (11th-generation Core and later have no SGX at all) and continues it only on Xeon for confidential-computing servers. Its original **EPID** attestation reached **end-of-life for production on 2 April 2025**. The supported path is **DCAP/ECDSA** attestation, which Ice Lake-SP and later Xeons support exclusively. AMD **SEV-SNP** and Arm **CCA** answer the modern cloud question, "protect a whole VM from a compromised hypervisor," and both provide hardware-rooted remote attestation (SEV-SNP via a VCEK, or the newer provider-specific VLEK, signed by the on-die AMD Secure Processor; CCA via RMM-generated Realm and platform tokens). The dividing line to communicate: SGX/SEV-SNP/CCA all provide **memory encryption and remote attestation**; base TrustZone provides **neither**, which is precisely the gap Arm CCA closes for the Arm ecosystem.

-----

### §9: ARMv9 CCA / RME

**Verification ledger**

|#  |Claim                                                                                                                              |Verdict                                                                                                            |Source                                        |
|---|-----------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------|----------------------------------------------|
|9.1|RME four worlds: Root, Secure, Realm, Normal                                                                                       |**Confirmed**                                                                                                      |Linux kernel CCA docs; ARM CCA HW Architecture|
|9.2|Root PAS private to EL3; security context set by SCR_EL3.{NS,NSE}                                                                  |**Confirmed.** Realm = {NSE,NS}={1,1}                                                                             |ARM CCA HW Arch; OpenCCA                      |
|9.3|GPT enables dynamic page-granule PAS transitions; GPC downstream of MMU; GPF on violation; GPT in Root memory, editable only by EL3|**Confirmed**                                                                                                      |ARM CCA HW Architecture                       |
|9.4|RMM runs at Realm EL2 (R-EL2)                                                                                                      |**Confirmed**                                                                                                      |ARM RMM spec DEN0137; TF-A RME docs           |
|9.5|"No RME silicon yet"                                                                                                               |**CORRECTED.** Neoverse V3 is first Neoverse CPU with CCA support; Arm AGI CPU (Mar 2026) ships up to 136 V3 cores |ARM Neoverse V3 page; ARM Newsroom            |
|9.6|Veraison attestation verifier maturity                                                                                            |**Confirmed active.** Veraison + cca-realm-measurements tool in use                                                |veraison GitHub; FOSDEM 2025                  |
|9.7|RMM specification version                                                                                                         |**Confirmed.** DEN0137 v1.0 published; v2.1 in ALPHA draft                                                         |ARM developer                                 |
|9.8|Linux/KVM Realm upstreaming                                                                                                       |**Confirmed.** Guest support merged **6.13** (Jan 2025); host/KVM merged **6.14-rc1**                              |LWN; kernelnewbies                            |
|9.9|TF-A / TF-RMM release status                                                                                                     |**Confirmed.** RME in TF-A since 2.6, current 2.15; `ENABLE_RME` still experimental; TF-RMM tracks RMM spec        |TF-A docs                                     |

**Rewritten §9:**

Arm CCA extends the two-world model to **four worlds** (**Normal, Secure, Realm, Root**) via the Armv9-A **Realm Management Extension (RME)**. The core mechanism is a new physical-address-space (PAS) dimension plus dynamic reassignment. EL3 firmware (the Root world monitor, TF-A) owns the **Root PAS**, which is private to EL3 and holds the **Granule Protection Table (GPT)**. Every physical access, from any world and any master routed through an SMMU, passes a **Granule Protection Check (GPC)** performed *downstream of address translation*, and a violation raises a **Granule Protection Fault (GPF)**. Because only Root-world code can edit the GPT, EL3 can **dynamically move a page of physical memory between Normal, Secure, and Realm** at page granularity. This is how a hypervisor donates memory to a Realm without ever being able to read it. Security state is encoded by `SCR_EL3.{NSE,NS}` (Realm = {1,1}). EL3 uses the NSE+NS bits in translation descriptors to map any of the four PAS.

The **Realm Management Monitor (RMM)** runs at **Realm EL2 (R-EL2)** and manages Realm VMs (which run at R-EL1/R-EL0). A Normal-world hypervisor (KVM) drives Realm lifecycle through the **RMI** SMC interface. A Realm guest talks to the RMM through the **RSI** interface, including to request attestation. Attestation produces a **Realm token** (measurement of the Realm's initial state, the RIM, plus extensible REMs) and a **platform token** (hardware, firmware, RMM state), signed and verifiable by a relying party. This is genuine runtime remote attestation, the thing base TrustZone lacks.

**De-staling the open item.** The report's flag, "CCA Realm attestation flows still maturing; no silicon," is now partly obsolete:

- **Silicon exists.** Neoverse **V3** is the first Neoverse CPU to support Arm Confidential Compute. Arm's **AGI CPU**, announced in a press release dated "Cambridge, England, Mar. 24, 2026," is Arm's first production silicon and ships with "Up to 136 Arm Neoverse V3 cores per CPU" (Armv9.2-A, TSMC 3nm, 300W TDP, lead partner Meta per corroborating trade coverage). This supersedes OpenCCA's own February 2025 survey statement that "no public Arm CPU supports CCA yet."
- **Kernel support is upstream.** Linux **Realm guest** support merged in **6.13** (Jan 2025). **host/KVM** support merged in **6.14-rc1**.
- **Firmware is real.** TF-A has supported RME since **2.6** (current line 2.15), though the `ENABLE_RME` build option is **still marked experimental**. **TF-RMM** is the reference RMM and tracks the RMM spec (**DEN0137**, v1.0 published, v2.1 circulating in ALPHA draft).
- **Attestation tooling is maturing, not mature.** The **Veraison** verifier project and the `cca-realm-measurements` tool (for pre-computing the RIM) are actively developed and demonstrated (FOSDEM 2025 keybroker demo), but this is early-adopter tooling, not turnkey.

**Boundary note.** CCA is the first Arm technology in this report that delivers *both* memory isolation-with-encryption *and* runtime attestation. But attestation is only as strong as the verifier and the supply chain behind the fused root key. A signed Realm token proves initial state and platform identity. It does not prove the absence of runtime side channels inside the Realm.

-----

### §11: Implementation Checklist (general, non-repo items)

**Verification ledger.** General items map to Confirmed primitives above. No repo-specific items assessed.

**Rewritten §11, general implementation checklist:**

1. **Anchor the root of trust in OTP/eFuse and enable TBBR.** Verify the fused root-public-key hash is provisioned and that BL1→BL2→BL3x authentication is on (`TRUSTED_BOARD_BOOT=1`). This gives load-time integrity, nothing more.
2. **Separate load-time integrity from runtime attestation in your threat model.** If you need to prove runtime state to a remote party, you need CCA/Realm attestation (or an external attestation service). TBBR alone does not provide it. State which one each requirement depends on.
3. **Lock the memory map before running Non-secure code.** On A-profile, program the TZC/TZASC regions and confirm Secure-only config registers are locked. On M-profile, set SAU regions, confirm the IDAU floor, and mark **only** genuine SG veneers as NSC, never stacks or data.
4. **Audit every non-CPU bus master.** GPUs, DMA engines, and debug masters must be TrustZone-aware, gated by interconnect security registers, or routed through an SMMU. An ungated DMA master is a full bypass.
5. **Scrub context on world exit.** On A-profile confirm the monitor saves/restores and clears non-banked registers. On M-profile confirm `NVIC_ITNS` targeting and rely on hardware register clearing for Non-secure interrupts during Secure execution.
6. **Treat shared memory as hostile.** Copy-then-validate out of Normal/Secure shared buffers, never validate-in-place (TOCTOU). Validate all SMC/RSI/RMI parameters at the trust boundary.
7. **Minimize and sign the Secure world.** Keep the Trusted OS and TA surface small (the SMC/parameter-parsing path is the highest-value bug class). Sign dynamic TAs (default RSA PKCS#1 v1.5 SHA-256) and prefer pseudo/early TAs covered by core secure boot where the TA must exist before the REE.
8. **Do not rely on TrustZone against side channels.** ARM explicitly excludes microarchitectural side channels from the Armv8-M threat model, and Cachegrab/BUSted show A- and M-profile leaks. Use constant-time crypto, enable `FEAT_DIT` (`PSTATE.DIT`) on A-profile where available, and keep secret-dependent control flow out of the secure world.
9. **Track firmware versions as critical dependencies.** Pin and monitor TF-A (2.15 line), OP-TEE (Internal Core API v1.3.1), and, if using CCA, TF-RMM and the RMM spec (DEN0137). Treat `ENABLE_RME` as experimental until your silicon and stack are qualified.

## Recommendations

- **Ship the three corrections now** (OP-TEE v1.3.1; TF-A 2.15 / RME since 2.6; CCA silicon + kernel 6.13/6.14). These are the claims most likely to embarrass the report if left stale. Benchmark to re-open: if OP-TEE publishes a release advancing the Internal Core API version, or a new TF-A LTS lands, refresh §3.
- **Replace the lone 2019 µs SMC figure** with the decomposition in §6 (hardware round-trip ≈ a few hundred cycles, OpenCCA reports ~421 cycles/182 instructions; software path µs-scale; CCA exit +~1.5 µs). Threshold to revisit: any first-party ARM cycle numbers for a specific core you deploy on.
- **Remove the "ARM documents specific M33 transition cycle counts" claim.** It is refuted. Substitute the 12-cycle base interrupt latency and the larger-secure-frame qualitative fact.
- **Re-verify the M-profile DIT (Armv8.1-M) claim** against the Armv8-M Architecture Reference Manual before publishing. The A-profile Armv8.4-A claim is solid.
- **Add specific CVE numbers or soften the TEE CVE history** (TEEGRIS/QSEE/Trusty). Cite CVE-2018-11976 for the QSEE case, which is confirmed.
- **Keep the boundary framing in every section.** Isolation is not cryptographic proof. Load-time integrity is not runtime attestation. An isolation boundary is not side-channel resistance.

## Conclusion

The source deep-dive is technically sound and needs no structural rework, only three currency corrections and one framing discipline held consistently. Ship the corrections now: OP-TEE Internal Core API v1.3.1, TF-A 2.15 with RME since 2.6, and Arm CCA as shipping silicon with upstream kernel support. Replace the lone 2019 microsecond SMC figure with the §6 decomposition, and remove the refuted claim that ARM publishes M33 transition cycle counts.

The enduring contribution is the boundary discipline, because it is the part most often stated wrong. Three distinctions carry the report: isolation is not cryptographic proof, load-time integrity is not runtime attestation, and an isolation boundary is not side-channel resistance. Base TrustZone, on both A-profile and M-profile, provides hardware-enforced two-world isolation and nothing more. It does not encrypt secure memory, it does not attest runtime state to a remote party, and it does not defend against microarchitectural side channels, which ARM places outside its threat model. Arm CCA is the first technology in this report to add memory encryption and genuine runtime attestation, and even there the guarantee is only as strong as the verifier and the fused-key supply chain behind it.

For an implementer, the operational takeaways are in §11: anchor the root of trust in fused OTP and enable TBBR, separate load-time integrity from runtime attestation in the threat model, lock the memory map and audit every non-CPU bus master before running Non-secure code, treat shared memory as hostile, minimize and sign the secure world, and do not rely on TrustZone against side channels. A design that states which requirement each mechanism satisfies, and never conflates "secure boot passed" with "the secure world can prove it is trustworthy now," will not make the classic error this report exists to prevent.

## Limitations and Caveats

- **Version currency.** Figures are current to July 2026 from primary docs. OP-TEE ships quarterly (v4.4.0 was the most recent release I could confirm, Oct 2024), so a newer minor release may exist. The *API* version (Internal Core API v1.3.1) is the load-bearing claim and is current per optee.readthedocs.io "latest."
- **Conflicts flagged, not hidden.** (a) OP-TEE API version: authoritative "latest" docs say v1.3.1 while trustedfirmware.org's project page and some vendor wikis still say v1.1.x. I take v1.3.1 as correct. (b) CCA architecture version: RME is described as an optional A-profile feature around Armv9.2–9.3-A, while Neoverse V3 / the AGI CPU are described as Armv9.2-A with CCA. Treat the exact minor version as unsettled between sources.
- **Figure to re-confirm.** The ~421-cycle / 182-instruction bare-SMC round-trip is OpenCCA's (Bertschi & Shinde, arXiv 2506.05129) reported value on RK3588. Confirm against the paper's evaluation table before publishing as a hard number.
- **Unverifiable in this pass.** TruSense (INFOCOM 2018) primary source; specific TEEGRIS/Trusty CVE numbers; the M-profile Armv8.1-M DIT introduction. These are marked, not asserted.
- **Excluded as instructed.** §5 (lib-security Rust crate) and §10 (CI/pipeline/attestation) were not verified. No repo-specific claims were assessed.
- **Apple Secure Enclave** is described at architecture level from general knowledge. Its primary documentation was not re-fetched in this pass.
