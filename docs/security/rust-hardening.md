# Rust hardening — recommendations & tools

Reference for the Rust-level security posture of `sentient-rs`: what we do, why, and the tracked gaps.
Sourced from the rustc/Cargo docs + cargo-deny (via Context7); each row cites the authority. Companion
to [`security.md`](./README.md) (crypto/boot posture) and [`distro/HARDENING.md`](../deployment/distro.md) (the Yocto image
lockdown: systemd sandbox, KSPP kernel, nftables, CVE/SBOM).

The workspace already runs an unusually complete set of gates; this doc records that and the few real
gaps rather than implying a greenfield.

## 1. Supply-chain / dependency hygiene

| Tool | Role | Here |
|---|---|---|
| `cargo-audit` | dep tree vs the RustSec advisory DB | ✅ `just audit` |
| `cargo-deny` | four checks — **advisories**, **bans** (crates + duplicate versions + wildcards), **licenses**, **sources** | ✅ `just deny` |
| `cargo-vet` | recorded human audits; gates un-vetted deps | ✅ `just vet` |
| `cargo-auditable` | embeds the dep list in the binary for in-situ scanning | ✅ `build-auditable` + the musl build |
| `cargo-cyclonedx` | CycloneDX SBOM | ✅ `just sbom` |
| `cargo-geiger` | counts `unsafe` usage across the tree | ✅ `just geiger` / `just geiger-report`; advisory CI job (`_supply-chain.yml`, `continue-on-error`) that uploads the report — not a gate |

Our `deny.toml` already sets `wildcards = "deny"`, `unknown-git`/`unknown-registry = "deny"`, and
`multiple-versions = "warn"`. cargo-deny's own default config bans `openssl`→`rustls`, `cmake`→`cc`,
`git2`→`gix` ([checks/cfg](https://embarkstudios.github.io/cargo-deny/checks/cfg.html)); we added the
`openssl`/`openssl-sys` ban (we use **rustls/ring**, no OpenSSL in the tree — so the ban is free and
documents the preference).

## 2. Compiler / binary exploit mitigations

| Mitigation | Mechanism | Availability / here |
|---|---|---|
| Dynamic PIE + full ASLR | on by default since rustc 0.12 ([exploit-mitigations](https://doc.rust-lang.org/rustc/exploit-mitigations.html)) | **stable**; but our *static* `aarch64-unknown-linux-musl` binary can't do static-PIE — the target spec lacks `static-position-independent-executables` (verified; x86_64-musl has it). Tracked: `distro/HARDENING.md`, #150/#154 |
| Full RELRO + BIND_NOW | `-Wl,-z,relro,-z,now` | ✅ for recompiled OS packages via `sentient-security-flags.inc` (our Rust binary gets RELRO from rustc) |
| Overflow trapping | `overflow-checks = true` | ✅ **added** to `[profile.profiling]` (the shipping host binary) — integer overflow → controlled abort+restart, not silent wrap of telemetry |
| `panic = "abort"` | profile setting | ✅ already set (`[profile.release]`, inherited by `profiling`) — smaller surface; pairs with systemd `Restart=on-failure`. (Note: a panicking tokio task now aborts the process rather than being isolated — acceptable for a watchdog-supervised agent) |
| Stack canaries | `-Z stack-protector` | **nightly only** — no stable Rust equivalent (documented accepted gap) |
| Sanitizers (ASan/MSan/TSan) | `-Zsanitizer=address\|memory\|thread` + `-Zbuild-std` ([sanitizer](https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html)) | **nightly** — test/CI lane, never shipped; complements Miri |
| CFI (forward-edge) | `-Zsanitizer=cfi` (needs LTO + clang + lld) | **nightly** — aspirational high-assurance lane |
| KCFI / shadow-call-stack | `-Zsanitizer=kcfi` / `=shadow-call-stack` + `-Zfixed-x18` | **nightly**, kernel/aarch64 |

## 3. Build-profile hardening (`Cargo.toml`, stable)

Cargo's defaults are not hardened (`overflow-checks = false`, `panic = "unwind"`, `strip = "none"` —
[cargo profiles](https://doc.rust-lang.org/stable/cargo/reference/profiles.html)). Our profiles are:
`opt-level="z"`, `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true`, and now
`overflow-checks=true` (profiling). This is close to the hardened ideal for a production binary.

## 4. Unsafe / soundness verification

- **Miri** — UB/OOB/aliasing/data-race detection (planned PR gate) ✅
- **Loom** — concurrency-permutation testing for lock-free code ✅
- **cargo-fuzz** ✅ (`just fuzz`), **cargo-mutants** ✅, **clippy `-D warnings`** ✅
- **cargo-geiger** — `unsafe`-surface visibility ➕ (`just geiger`)

## 5. Tracked gaps / follow-ups

Cheapest first (see the tracking task):
1. ✅ done here: `overflow-checks = true` (profiling); `cargo-geiger` recipe; `openssl` cargo-deny ban.
2. Consider `multiple-versions = "deny"` (currently `warn`) once the dup-version tree is curated with `skip`/`skip-tree`.
3. ASLR on aarch64 (#150/#154): a glibc-**dynamic** build compiled against the image's own glibc (Yocto SDK / build-from-source) regains PIE/ASLR without glibc skew.
4. Aspirational nightly lane: CFI + AddressSanitizer for the std/Linux binary (needs `-Zbuild-std`/clang/LTO — a separate non-shipping CI job).

## 6. Pipeline: security gates that upload results

The gates don't just pass/fail — they PRODUCE machine-readable evidence the pipeline uploads, in
addition to the SBOM (`cargo-cyclonedx`) and the X.509 CMS / cosign signatures. Reproduce any report
locally with the same `just` recipe CI calls.

| Gate | Where it runs | Gates? | Result produced + uploaded | Recipe |
|---|---|---|---|---|
| cargo-audit · cargo-deny · cargo-vet | `_supply-chain.yml`, every PR | ✅ hard gate | `security-gate-reports` artifact — `cargo-audit.json`, `cargo-deny.jsonl` (`if: always()`, so a red gate still uploads the report that explains it) + a step-summary table | `just supply-chain` (gate) + `just security-report` (evidence) |
| cargo-geiger (unsafe surface) | `_supply-chain.yml`, every PR | ➖ advisory (`continue-on-error`) | `unsafe-surface-report` artifact — `cargo-geiger.json`, `cargo-geiger.md` | `just geiger-report` |
| cargo-audit + cargo-deny (release evidence) | `production.yml` build job, per release | ➖ non-fatal evidence | `sentient-rs.cargo-audit.json` / `sentient-rs.cargo-deny.jsonl` attached to the GitHub Release next to the SBOM + `.p7s` signatures | `just security-report` |
| cargo-mutants · cargo-fuzz · cargo-miri | `scheduled.yml`, weekly | ➖ advisory | in-log / job status | `just mutants` / `just fuzz` / `just miri` |

`cargo-geiger` is baked into `sentient-builder` (`.ci/Dockerfile`), so the advisory CI job just runs
the recipe — no per-run install.
