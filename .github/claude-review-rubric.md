# Claude PR Review Rubric

This file is the review specification for the automated Claude reviewer. PR events fire a Claude
Code Routine via the bridge in `.github/workflows/claude-pr-review.yml`; the Routine's review
session reads this file from the DEFAULT branch at review time — so the rubric can be edited here,
by anyone, in a normal PR, without touching CI, but a PR cannot weaken the criteria for its own
review.

How to use it: for each numbered category below, evaluate the **Applies when** trigger against the
PR's changed files and diff content. Run the checks of every category that matches, plus the
**Always-on checks** section, and skip the rest. Every category must still be accounted for in the
required summary table at the end — a skipped category is reported as skipped *with its reason*,
never silently dropped.

Checks are written as imperative review instructions. They describe things a reviewer can decide
from the diff plus the surrounding files, not things that need a build or a board.

---

## 1. Rust best practices & API design

**Applies when:** any `*.rs` file changes; `pub` item changes in `crates/lib-*`; a `version` or
`rust-version` bump in any `Cargo.toml`.

- Check new public items against the Rust API Guidelines conventions: naming (`as_`/`to_`/`into_`
  cost prefixes, `iter`/`iter_mut`/`into_iter`), the common-trait derives where they make sense
  (`Debug`, `Clone`, `PartialEq`, `Default`), getter naming without a `get_` prefix, and iterator
  types returned rather than collections where the caller may not need one.
- Treat removing, renaming, or narrowing the visibility of an existing public item as a
  **semver-major** change: the crate version and CHANGELOG must reflect it. Flag the change if they
  do not.
- Confirm the MSRV is not silently bumped — a new language feature, edition idiom, or dependency
  requiring a newer toolchain than the declared `rust-version` must come with an explicit
  `rust-version` change and a note.
- Verify public APIs do not leak private types: a `pub fn` must not take or return a type that
  callers outside the crate cannot name.
- Suggest `#[non_exhaustive]` on public enums and structs that are expected to grow, so a later
  variant or field is not itself a breaking change.
- Review ownership and borrowing for idiomatic use: no needless `clone()`/`to_owned()` to dodge a
  borrow, no `&Vec<T>`/`&String` where `&[T]`/`&str` would do, no `Option<&mut T>` gymnastics that a
  restructure would remove.

## 2. `unsafe` & soundness

**Applies when:** the diff contains `unsafe`, `unsafe impl Send`/`unsafe impl Sync`, or `static mut`;
or it touches a file already known to carry unsafe code.

- Require an adjacent `// SAFETY:` comment on every new or changed `unsafe` block that states the
  **invariant being relied on and why it holds at this call site** — not a restatement of what the
  line does. Flag a missing, stale, or purely descriptive SAFETY comment.
- Justify every `unsafe impl Send`/`Sync` against the type's *actual* thread-safety: look for
  interior mutability, raw pointers, and non-atomic fields reachable from the type. An `unsafe impl`
  added to silence a compiler error is a defect, not a fix.
- For unsafe code reachable from untrusted input, require either a Miri test exercising that path or
  a safe abstraction wrapping it. Untrusted bytes reaching a raw-pointer path with neither is a
  blocking finding.
- Reject `unsafe` used to side-step an existing trait seam (`SerialTransport`, `Publisher`,
  `Platform`, `NetLink`/`NetStack`). The correct move is to extend the seam; per `CLAUDE.md` the
  bypass *is* the debt.
- For new unsafe in host-testable `lib-*` code, expect Miri results to be reported in the PR — or an
  explicit "N/A" with the reason (e.g. the code cannot run under Miri because it does MMIO).

## 3. TrustZone S/NS boundary

**Applies when:** `crates/bin/mcu-rp2350/{s,ns,abi,veneers}/**` changes.

Review against ADR 0011.

- Confirm secure entry points are reachable only through the NSC veneer table. Flag any new
  secure-callable surface introduced incidentally — an exported symbol, a widened veneer table entry,
  a new `#[no_mangle]` in the secure image.
- Require every pointer, buffer, and length arriving from NS to be validated with a CMSE
  address-range check **before** it is dereferenced. An unvalidated NS-supplied pointer is a
  blocking finding.
- Reject raw pointers or references crossing the boundary: the ABI carries by-value data or opaque
  handles only. Flag any new `abi` type with a pointer, slice, or reference field.
- Verify S-side secrets are never copied into NS-readable buffers, and check the **error paths**
  specifically — a failure branch that returns a partially-filled buffer or an error struct carrying
  secret-derived bytes is the usual leak.
- Confirm registers and stack are scrubbed before the SG return, so residual secure data is not left
  visible to NS.
- Keep the SAU and MPU tables fail-closed. Any widening of an NS or NSC range needs a written
  justification tied back to ADR 0011.

## 4. Embedded / no_std / real-time

**Applies when:** `crates/{impl-app,spec-app}/**`, `crates/bin/{mcu-platform,rp2350}/**`, `hil/**`,
or a no_std-capable `lib-*` crate changes.

- Apply the `CLAUDE.md` embedded rules: store canonical data, not the serialized wire form; render
  into one reused buffer at transmission time; use reused `static` storage instead of large hot-path
  stack locals; keep a spinlock's critical section covering the *whole* access with owned data copied
  out under the lock; keep `heapless::spsc` strictly single-producer; and watch the match-arm
  max-frame gotcha — a function reserves the largest frame across all arms, so split divergent-cost
  arms into `#[inline(never)]` functions.
- Require ISRs to do the minimum: clear the flag, wake or signal a task, return. Flag allocation,
  blocking, formatting, or locks shared with lower-priority contexts inside an interrupt handler.
- Confirm the CPU never touches a DMA-active buffer, and that appropriate fences bracket DMA start
  and completion.
- Require MMIO through `read_volatile`/`write_volatile` or the PAC's typed API. A plain `*mut`
  dereference against a peripheral address is a defect — the compiler may reorder or elide it.
- For GPIO input-with-pull usage, check it against RP2350 **erratum E9**: the silicon stepping must
  be stated and the documented workaround applied.
- Verify watchdog feeds sit where a real hang is actually caught. A feed driven by an independent
  timer or placed outside the work it is supposed to supervise defeats the watchdog.

## 5. Async & concurrency

**Applies when:** the diff contains `.await`, `tokio::`, `embassy`, or `core::sync::atomic`.

**tokio side**

- Flag blocking calls inside an `async fn` — synchronous `std` I/O, filesystem calls, CPU-bound
  loops, a `std::sync::Mutex` held across an `.await` — unless wrapped in `spawn_blocking`.
- Reason about cancellation safety for every `tokio::select!` branch that holds partial state: if the
  branch loses the race mid-operation, say what is dropped and whether that loses data.
- Require channels fed by external input to be **bounded**, with the capacity chosen deliberately and
  the backpressure behaviour stated. An unbounded channel on an ingest path is an OOM waiting for a
  fast producer.
- Reject `block_on` called from an async context.

**Embassy side**

- Confirm every task yields regularly. A task with a loop containing no `.await` starves the
  cooperative executor and blocks every other task on that core.
- Require shared state to go through `embassy_sync` primitives or one documented critical section.
  Ad hoc `static mut` access is a finding.

**Atomics**

- Require the memory-ordering choice to be justified in a comment that states the happens-before
  relationship it establishes. `SeqCst` chosen as a default, or `Relaxed` chosen for convenience,
  are both findings — the comment is the deliverable.
- Require a Loom test for new lock-free code.
- Expect Miri to be run at the MCU pointer width (e.g. `i686`) where the code is MCU-bound.

## 6. Untrusted-input parsing & fuzz coverage

**Applies when:** `crates/lib-{parse,canonical,mapping,dds,mqtt,tms}/**`,
`tools/tms-idl-codegen/**`, or `crates/test-e2e/fuzz/**` changes.

- Require parser and decoder changes to come with a matching fuzz-target or corpus update, or an
  explicit note in the PR explaining why the existing coverage already reaches the new path.
- Require every length, count, or offset read from the wire to be bounds-checked *before* it drives
  an allocation or an index. Flag `Vec::with_capacity(untrusted_len)` and any reserve sized directly
  from wire data.
- Require arithmetic on wire-derived fields to be checked, saturating, or explicitly wrapping —
  release builds have no overflow checks, so a debug-only panic becomes a silent wrong value.
- Reject unchecked indexing (`get_unchecked`, or `[]` on a slice) driven by an attacker-controlled
  offset.

## 7. Cryptography use

**Applies when:** `crates/lib-{tls,security}/**` or wolfcrypt-related files change.

- Reject hand-rolled primitives. Flag any new byte-level hash, cipher, padding, or comparison logic —
  it belongs in a vetted crate.
- Require comparisons of secrets, keys, and MACs to be constant-time. A `==` on key or tag material
  is a blocking finding.
- Confirm randomness comes from a CSPRNG — hardware entropy on the MCU. A fixed seed, a counter, or a
  non-cryptographic PRNG in a security path is a defect even in test-shaped code that can ship.
- Require key-bearing structs to zeroize on drop, and to not derive `Debug` or otherwise reach a log
  sink.
- Check for any trust-tier downgrade against `lib-security`'s weakest-wins invariant (ADR 0013): a
  change that lets a stronger tier be claimed than the inputs justify is a blocking finding.
- Flag TLS configuration that loosens rustls' secure defaults — cipher suites, versions, verifier
  overrides, ALPN — without inline justification.

## 8. Supply chain & dependencies

**Applies when:** `Cargo.toml`, `Cargo.lock`, `supply-chain/**`, `deny.toml`, `.cargo/audit.toml`, or
`vendor/**` changes.

- Require a cargo-vet entry for every new direct dependency. A `Cargo.lock` diff with no
  corresponding `supply-chain/` change is a finding.
- Confirm each new dependency's license is inside the `deny.toml` allowlist. Exceptions must be
  explicit entries, not a silent widening of the allowlist.
- Require new RUSTSEC ignore entries to carry a reachability argument and a removal condition, in the
  comment style the existing entries use.
- Verify dependencies pulled into no_std targets are actually no_std-compatible, including
  transitively — a crate that quietly enables `std` or `alloc` breaks the MCU link.
- Require `vendor/dust_dds*` diffs to state what changed versus upstream and why, so the delta stays
  auditable.
- Weigh the unsafe footprint of new MCU-side dependencies; a large unsafe surface on the
  memory-constrained target deserves a note.

## 9. Panic policy & error handling

**Applies when:** any `*.rs` file changes. Weight the review toward `crates/lib-*`.

- Flag new `unwrap()`, `expect()`, `panic!`, and unchecked `[]` indexing applied to I/O-, network-, or
  wire-derived data in library crates. These paths return `Result`.
- On no_std MCU crates, confirm a panic is genuinely the right response — it resets the board. A
  panic standing in for error propagation on untrusted serial input is a defect, not a policy.
- Require public error enums to be thiserror-style with variants a caller can distinguish, separating
  recoverable from non-recoverable cases. An `Error(String)` catch-all is a finding.
- Keep anyhow-style aggregation in `bin-*`. An `anyhow::Error` surfacing through a `lib-*` public API
  is a finding.

## 10. Clean code / SOLID / DRY + project conventions

**Applies when:** any source change.

- Review for single responsibility, duplication of an existing utility, dead code, and naming that
  does not say what the thing is.
- Enforce the `CLAUDE.md` type/impl split: one primary type per file under a `types/` sub-module (no
  `type_` prefix), `impl_<name>.rs` files at the module root, everything re-exported so public paths
  are unchanged, and the prelude rules — type files import directly (a sibling as `use super::a::X;`,
  externals by full path) and only `impl_*.rs` files glob `use super::prelude::*`.
- Enforce trait-seam discipline: a raw platform call, FFI call, or C shim appearing in app logic is
  the signal to define or extend a seam — never to bypass one "just for now". Keep any FFI shim dumb
  (one-shot passthrough, no loops, retry, or policy).

## 11. CI / GitHub Actions security

**Applies when:** `.github/**`, `.ci/**`, or `justfile` changes.

- Require every `uses:` to be pinned to a full commit SHA with a `# vX` version comment.
- Require least-privilege `permissions:` at both workflow and job level. A new `write` scope, or
  `id-token: write`, needs a stated need scoped to the job that uses it.
- Reject any workflow that feeds untrusted content — PR titles and bodies, issue comments, branch
  names — into an LLM-driven action, or that checks out an untrusted head while secrets are in reach.
  Give `pull_request_target` changes maximum scrutiny.
- Allow `secrets: inherit` only for first-party reusable workflows in this repo, never to a
  third-party action.
- Require new `zizmor.yml` or `deny.toml` ignore entries to carry a written reason, scoped to specific
  files rather than applied globally.

## 12. Yocto / distro / config-ops

**Applies when:** `distro/**`, `deploy/**`, or systemd unit files change.

- Require new and changed units to keep the hardening baseline: `NoNewPrivileges`,
  `ProtectSystem=strict`, `PrivateTmp`, and a tight `CapabilityBoundingSet`. Any loosened or removed
  directive needs a stated reason.
- Keep nftables rules and listener configuration least-privilege — specific ports and interfaces, not
  a wildcard bind.
- Require new feature flags to have a removal condition, with both states exercised in CI.
- Require Yocto recipes to use pinned, reproducible `SRC_URI` values — no floating `master` — and to
  stay consistent with the wrynose 5.0 LTS pin.

## 13. Wire-format / schema / breaking changes

**Applies when:** `crates/lib-{canonical,tms,dds,mapping,mqtt,store}/**` or
`tools/tms-idl-codegen/**` changes.

- Classify every canonical, TMS, or DDS-IDL change as **additive** or **breaking**. A removed,
  renamed, or retyped field, or a changed discriminant, is breaking and needs a migration note plus
  an artifact version bump.
- Require MQTT topic changes to ship a compatible transition — dual-publish or a versioned topic
  segment — rather than a silent rename that strands existing subscribers.
- Require generated code to be regenerated and committed in the **same PR** as the IDL source it
  derives from, so the tree is never internally inconsistent.
- Require `lib-store` schema changes to include a migration path, or an explicit statement that
  nothing is persisted yet.

## 14. Observability & logging

**Applies when:** the diff contains `tracing::`, `log::`, or `defmt::`.

- Reject secrets, key material, tokens, and full device or BMS serial numbers at `info` or `debug`
  level. Redact or truncate identifiers that reach a log sink.
- Check the level matches the severity: a recoverable retry is not an `error`, and a real fault is not
  a `debug`.
- Require MCU hot paths to use `defmt` rather than `core::fmt` formatting — `core::fmt` pulls in
  formatting machinery and costs both flash and cycles.
- Check the defmt-to-host log bridge for fields dropped or degraded across the boundary, so a message
  that is useful on the wire is still useful in the host log.

---

## Always-on checks (every PR)

Run these regardless of which files changed.

- **Secrets:** no secrets, credentials, private keys, `.pem`, or `.key` material anywhere in the
  diff — including test fixtures and example configs.
- **Suppressions:** every new `#[allow(...)]` carries a justification for why the lint is wrong here.
- **Conventions:** the type/impl-split and trait-seam conventions hold for every new or changed file.
- **Tests:** new logic has a host-testable test — `cargo test`, plus Miri for unsafe and Loom for
  lock-free code. Emulator-only or hardware-only verification is not sufficient coverage.
- **Docs:** new public items have doc comments, and non-obvious logic has an explanatory comment
  saying *why*, not *what*.
- **Placeholders:** no new `TODO`, `FIXME`, `todo!()`, or `unimplemented!()` without a tracking-issue
  reference.
- **Scope:** flag a PR mixing unrelated concerns and suggest how to split it.

**Do not restate what CI already enforces mechanically** — formatting, clippy lints, cargo-audit,
cargo-deny, and the type/impl audit all have their own gates. Mention one of those only when the diff
is actively fighting the gate (for example, a new ignore entry or an `#[allow]` added to get past it).

---

## Required output

Post line-specific findings as inline review comments on the relevant lines, each tagged with a
severity: **CRITICAL**, **HIGH**, **MEDIUM**, or **LOW**.

Then post exactly **one** summary comment. It must end with a table listing **all 14 categories** —
none omitted — each marked either ✅ Reviewed or ⏭️ Skipped, and every skipped row carrying the
reason it did not apply:

| # | Category | Status |
|---|----------|--------|
| 1 | Rust best practices & API design | ✅ Reviewed |
| 2 | `unsafe` & soundness | ⏭️ Skipped — no `unsafe`, `unsafe impl`, or `static mut` in the diff |
| 3 | TrustZone S/NS boundary | ⏭️ Skipped — no TrustZone files changed |
| … | … | … |

The reason is the point of a skipped row: it shows the category was evaluated and ruled out, not
forgotten. "Not applicable" on its own is not a reason — name the trigger that failed to match.

### Formatting

Review output is GitHub-flavored Markdown and must use GitHub's rich-formatting affordances:

- **Line-specific findings belong on the PR'd files** — post them as inline review comments on the
  exact file and line of the diff, not as prose in the summary. The summary points at them; it does
  not restate them.
- **Cite committed code by permalink reference, not by pasted snippet.** To show committed code in
  a comment body, put a blob permalink at the reviewed head SHA on its own line —
  `https://github.com/<owner>/<repo>/blob/<head-sha>/<path>#L<start>-L<end>` — and GitHub renders
  it as an embedded, linked code block. Reserve fenced code blocks for code that does NOT exist in
  the repo (proposed replacements, expected output).
- **Use autolinked references** ([docs](https://docs.github.com/en/get-started/writing-on-github/working-with-advanced-formatting/autolinked-references-and-urls)):
  bare full commit SHAs for commits, `#<number>` for issues/PRs, `owner/repo#<number>` across
  repos, bare URLs elsewhere — not hand-built Markdown links where an autolink works.
- **In inline comments, propose concrete fixes as ` ```suggestion ` blocks** scoped to the
  commented lines, so the fix is one-click applicable.
