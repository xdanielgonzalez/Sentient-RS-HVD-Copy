# Handoff — Human-readable spec-alignment suite (cucumber-rs / Gherkin)

**Status:** implemented on branch `acceptance-suite-cucumber` (PR pending); `test-acceptance`
green (`just test-acceptance` — 8 features / 33 scenarios / 140 steps), `cargo deny check` +
`cargo vet` clean, `just build-no-std` unaffected, workspace fmt/clippy clean.

## Why

`sentient-rs` correctness hinges on the **Seplos → Canonical → TMS (MIL-STD-3071)** mapping, but the
alignment between the TMS spec, the Seplos protocol, and the implementation is only encoded as Rust
unit assertions (`tms_transform.rs`, `tms_live_frame.rs`), `insta` snapshots, and the hand-authored
`docs/tms-compliance.md`. There is no **human-readable acceptance layer** a reviewer or the client
(Sentient) can read to see "this Seplos frame produces these TMS fields, per this IDL section."

This adds a Gherkin (`cucumber-rs`) suite whose `.feature` files read as plain-English spec
statements and whose step definitions **reuse the real production/testkit code** — zero
re-implementation, zero sim/decoder drift.

## Decisions (locked with the user)

- **cucumber-rs (Gherkin), NOT Robot Framework.** Robot would require re-implementing the Seplos
  responder in Python — reintroducing the drift the shared-checksum `SeplosSim` design eliminates —
  and adding a Python toolchain to a pure-Rust repo. cucumber-rs keeps step defs in Rust, reusing
  `test-kit` + the real transform.
- **Scope = spec-alignment core** (in-process Seplos→Canonical→TMS field correctness). NOT a
  black-box shipped-binary system suite (that could be a later phase).

## Design

### New crate `test-acceptance` (`publish = false`, host/std-only)
Dedicated crate, NOT inside `test-e2e` (which is bound to the nextest lane + `seplos-e2e`
serialization; a cucumber `harness = false` binary is not nextest-runnable). The in-process suite
(no pty/broker/socket) runs in the fast `cargo test` lane with no serialization.

- `test-acceptance/Cargo.toml` — `[[test]] name="cucumber", harness=false`; dev-deps
  `cucumber = "0.21"`, `tokio` (workspace), and the code under test: `test-kit`,
  `lib-parse` (`seplos,std`), `lib-mapping`, `lib-schema`, `heapless`, `serde_json`,
  `serde-json-core` — all `{ workspace = true }`. `[lints] workspace=true`.
- `tests/cucumber.rs` — `#[derive(cucumber::World, Debug, Default)] SpecWorld` (state as
  `Option`/`Vec`: `state`/`sim`/`raw_frame`/`reading`/`fault`/`dc`/`hw`/`diag`/`reencoded`/
  `round_tripped`/`last_error`; `Behavior` as `Option` — no `Default`); the runtime runs on a
  dedicated thread with a 64 MiB stack (`SpecWorld::cucumber().run_and_exit("tests/features")`) —
  a plain `#[tokio::main]` main thread overflows the default 1 MiB Windows main-thread stack, a
  known cucumber-rs issue unrelated to this suite's own logic; deterministic `dev()`/`now()`
  helpers + a single `TOL = 0.005` (the tighter of `tms_transform.rs`'s 0.01 and
  `tms_live_frame.rs`'s 0.005) used by every "approximately" assertion.
- `tests/steps/mod.rs` — Given/When/Then defs. Header comment + grep gate: no Seplos/transform logic
  here; all decode/map/codec calls delegate to the real crates.

### Feature corpus `tests/features/` — one file per `tms-compliance.md` section
`dc_measurement.feature` (IDL 13882–13926), `power_hardware.feature` (8035–8128),
`energy_storage.feature` (6197–6323 / 6064–6169), `active_diagnostic.feature` (10053–10111, DTC
8149–8377), `transform_conventions.feature` (§4 sign+scaling matrix), `live_frame.feature` (real
14-cell capture + frame-integrity negative paths), `dtc_overflow.feature`, `provisional.feature`.
- Each Feature header cites the `chapter-d.txt` IDL line range + `tms-compliance.md` section → the
  corpus IS the readable alignment artifact.
- Data-driven Scenario Outlines mirror the field/unit/scaling matrix (`cv_to_v 5452→54.52 V`,
  `mv_to_v 3894→3.894 V`, `dk_to_c 2801→7.0 degC`, sign-inversion `-164 cA → +1.64 A`,
  `500 cA → -5.00 A`).
- LIVE_FRAME scenario reproduces the exact known values already in `tms_live_frame.rs` (54.52 V,
  +1.64 A, 14 cells, cell[0]=3.894 V, min 3.893/max 3.895, soc 0.775) + JSON/postcard round-trips →
  binds the Gherkin suite to the same code as the Rust unit test.
- Negative paths (`live_frame.feature`): `SeplosSim::set_behavior(CorruptChecksum/WrongAddress/
  Truncated/Silence/Garbage)` — every one must be rejected, never silently accepted.

### Step-def reuse (no re-implementation)
`BatteryState::sample_14_cell()` + `SeplosSim::respond()` (reuses `lib_parse::seplos::checksum`)
or the verbatim `LIVE_FRAME`; decode via `Seplos::parse_reading`/`parse_fault`; map via
`transform::{ToTms,ToDiagnostics,FromTms,FromDiagnostics}` + `scale::*`; round-trip via
`codec::{to_json_slice, tms::{to_bytes_slice,from_bytes_slice}}`. Negatives drive
`SeplosSim::set_behavior(...)` and assert typed errors. `approx()` tolerance 0.005 uniformly
(the tighter of the two source tests' tolerances).

### `@provisional` tagging (honesty, per `tms-compliance.md §7`)
`@empirical-faultmap` (the `(byte,bit)` fault map), `@synthetic-vector` (no real 0x44 capture),
`@placeholder-spn` (SPN 1–16, uniform `Major`/`occurrence_count=1`), `@unverified-maxlen`
(MAX_PORTS=4 / MAX_STORAGE_CELLS=50) — each scenario also carries a feature-level `@provisional`
tag (`provisional.feature`) so `--tags "not @provisional"` filters the whole file at once. They
run + stay green but render as visibly-unconfirmed.

### Re-anchor stale doc cites
`docs/tms-compliance.md` had 14 stale `lib-mapping/src/schema.rs:NN` cites (lines 36, 40, 41,
45, 49, 66, 74, 76, 88, 93, 96, 118, 122, 148) — types moved to `lib-schema/src/lib.rs`.
Re-anchored each by grepping the true symbol location (own commit, first on this branch); also
refreshed the `transform.rs:NN` cites that had drifted since the report was authored.

### Wiring edits (only these existing files change)
- `Cargo.toml` (root) — add `"test-acceptance"` to `[workspace] members`.
- `justfile` — `test-acceptance: cargo test -p test-acceptance --locked --test cucumber`; add
  `--exclude test-acceptance` to `coverage`.
- `.github/workflows/_test.yml` — named `just test-acceptance` step in the `test` job. NOT
  the nextest `e2e` job. No `.config/nextest.toml` entry.
- Host-only dev crate → NOT in `just build-no-std`; the no_std MCU contract is untouched.

## Risks (resolved)
1. **cargo-vet / cargo-deny on the cucumber dep tree** (`vet` gates CI; `deny.toml wildcards=deny`) —
   ran `just deny` + `just vet` after adding cucumber; added `supply-chain/` exemptions for the new
   dep tree (gherkin, globwalk, regex, peg, synthez, etc.).
2. **cucumber 0.21 API churn** — verified against the actual 0.21.1 source: `World::cucumber()` /
   `run_and_exit`, `#[given/when/then(expr = "...")]` Cucumber Expressions (`{int}`/`{float}`/
   `{word}`/`{string}`), bare-literal `#[given("...")]` for exact-match steps with no captures
   (used to sidestep Cucumber Expression's `(...)` = optional-text syntax for step text that
   contains literal parentheses), `--tags` CLI flag.
3. **`Behavior` has no `Default`** — stored as `Option<Behavior>` in `World`, as planned.
4. **`harness=false` + nextest** — not nextest-runnable by design; documented in the crate
   description and in the CI step comment.
5. **Windows main-thread stack overflow** — cucumber's `Runner`/`Writer` future chain overflows the
   default 1 MiB Windows main-thread stack. Fixed by running the runtime on a dedicated thread with
   a 64 MiB stack instead of `#[tokio::main]` directly on `main`.

## Verification
- `just test-acceptance` green (8 features / 33 scenarios / 140 steps); `-- --tags "not
  @provisional"` runs the confirmed-only subset (29 scenarios).
- Binds to real code: LIVE_FRAME scenario asserts the same values as `tms_live_frame.rs` (both fail
  together if `transform` changes); step-defs delegate all decode/map (grep-gated, documented in
  the `tests/steps/mod.rs` header).
- Fail demonstration (suite smoke test): flip one Examples row (`-164 | 1.64` → `1.65`) →
  `approx()` Then-step fails, `cargo test` returns nonzero. Documented in
  `test-acceptance/README.md`.
- `just deny` + `just vet` clean; `just build-no-std` unaffected; workspace fmt/clippy clean.

## Critical files
New: `test-acceptance/{Cargo.toml, tests/cucumber.rs, tests/steps/mod.rs, tests/features/*.feature,
README.md}`.
Edit: root `Cargo.toml`, `justfile`, `.github/workflows/_test.yml`, `docs/tms-compliance.md`,
`supply-chain/config.toml`.
Reuse (do not modify): `test-kit/src/seplos.rs`, `lib-mapping/src/transform.rs`,
`lib-mapping/tests/tms_live_frame.rs`, `tms_transform.rs`, `lib-schema/src/lib.rs`.
Spec source: `/d/hvd/meteor-adapter/docs/chapter-d.txt`.
