# test-acceptance

Human-readable **cucumber-rs (Gherkin) spec-alignment suite**: Seplos -> Canonical -> TMS
(MIL-STD-3071) field correctness, exercised in-process against the real production/testkit code
(`test-kit`, `lib-parse`, `lib-mapping`, `lib-tms`, `lib-canonical`) — zero
re-implementation, zero sim/decoder drift. See `../handoff_acceptance_suite.md` for the full
design rationale.

Host/std-only dev crate, `publish = false`. Never a dependency of the no_std MCU build, never
wired into `just build-no-std`.

## Running

```sh
just test-acceptance
# or directly:
cargo test -p test-acceptance --locked --test cucumber

# confirmed-only subset (skip @provisional scenarios):
cargo test -p test-acceptance --locked --test cucumber -- --tags "not @provisional"
```

The `cucumber` test target is `harness = false` — cucumber prints its own progress/summary
instead of libtest's, and it is **not** nextest-runnable, so it is deliberately its own recipe
and CI step, never folded into `just test` or `just test-e2e`.

## Layout

- `tests/cucumber.rs` — the `SpecWorld` cucumber `World` + the runner `main()`.
- `tests/steps/mod.rs` — Given/When/Then step definitions. Every decode/map/codec call
  delegates to the real crates; no Seplos protocol or Canonical->TMS transform logic is
  re-implemented here (see the file's header comment).
- `tests/features/*.feature` — one file per `docs/tms-compliance.md` section; each Feature
  header cites the `chapter-d.txt` IDL line range it covers.

## The suite is load-bearing (smoke test)

To prove a Gherkin assertion actually fails the build when the underlying value is wrong (not
just decorative prose), the following mutation was applied and reverted during development:

1. In `tests/features/transform_conventions.feature`, the sign-inversion Examples row
   `| -164 | 1.64 |` was changed to `| -164 | 1.65 |`.
2. `cargo test -p test-acceptance --locked --test cucumber` then FAILED:
   - The `Then the DC line amperage is approximately 1.65 A` step failed with
     `Step panicked. Captured output: amperage 1.64 !~ 1.65`.
   - The suite summary read `33 scenarios (32 passed, 1 failed)` / `140 steps (139 passed, 1
     failed)`.
   - The process exited with a nonzero status (101), so `cargo test` itself reports failure —
     confirming `run_and_exit`'s panic-on-failure propagates all the way out.
3. The row was reverted to `| -164 | 1.64 |` and the suite is green again (33/33 scenarios,
   140/140 steps).

This is the same mutation-and-revert pattern used to validate any acceptance/characterization
test suite: if flipping a known-correct value doesn't turn the suite red, the suite isn't
actually checking that value.
