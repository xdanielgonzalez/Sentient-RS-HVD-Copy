# 0009 — Ports & adapters: extract the app contract into `spec-app`; platform adapters; thin binaries

**Status:** Accepted (2026-07-17)

## Context

After the crate consolidation (#289 / #285–#287) the workspace graph is clean, but one
dependency-inversion wart remains — surfaced while reviewing the regenerated crate topology:

- The **`Platform` trait** (the seam the poll loop is written against — `type Transport:
  SerialTransport`, `type Pub: Publisher`, `emit_reading`, `now`, `sleep`, …) lives *inside*
  `impl-app`, bundled with the run-loop algorithm. So a platform adapter such as
  `bin-zephyr-platform` must compile-depend on the **entire policy crate** just to see the one
  trait it implements — the *detail* depends on the *policy*.
- The **RPi platform implementation** (`HostPlatform` + the embedded-broker publisher + the
  mesh/locate taps) lives inside `impl-app` behind its `host` feature. That is why `impl-app`
  carries the optional `lib-mqtt` / `lib-net` dependencies and why those two edges hang off it in
  the graph. The **MCU platform** (`ZephyrPlatform`) is externalized as `bin-zephyr-platform` and
  injected. The result is asymmetric: the Pi platform is *embedded* in the app, the MCU platform is
  *injected* into it.

The design intent (dependency injection, ports & adapters) is that `impl-app` is **pure policy** —
the poll → map → publish loop, generic over `Platform` — with every hardware/service concretion
constructed by the binaries and injected through the trait. `impl-app`'s *core* already meets this
(`run<P: Platform>` names no concrete type); the gap is purely that the contract and one adapter
are physically in the wrong crate.

Naming was part of the decision: `lib-platform` **collides** with the adapter crates
(`bin-*-platform`) — you could not tell the interface from its implementations; `lib-app` reuses
"app" and, like `lib-`, connotes a *utility library* rather than the architectural contract, which
is the single most-depended-on artifact in the graph.

## Decision

Adopt an explicit **ports-and-adapters** layering, extracted as three behavior-preserving crate
moves:

1. **Extract `spec-app`** — a new crate holding the **contract**: the `Platform` port plus the
   shared vocabulary it speaks in (`RunConfig`, `Health`). The name follows a **category-first
   prefix scheme** where the prefix *is* the architectural role: `spec-` = contract, parallel to
   the existing `impl-` = implementation (`spec-app` ⇔ `impl-app`). This avoids the `platform`
   collision and the `lib-` utility connotation. `spec-app` depends only on the trait/data crates
   its signatures name (`lib-io` `SerialTransport`, `lib-mapping` `Publisher`/`ClockMonotonic`,
   `lib-canonical` `Canonical*`).
2. **`impl-app` becomes pure policy** — depends on `spec-app` and the domain crates its algorithm
   uses; holds `run`/`run_cycle`/`run_inverter` only. No platform impl, no `lib-mqtt`, no `lib-net`.
3. **Extract `bin-rpi-platform`** — the RPi platform adapter (`HostPlatform` + embedded-broker
   publisher + mesh/locate taps + host serial), implementing `spec_app::Platform`. A symmetric peer
   of `bin-zephyr-platform`.
4. **Both platform adapters depend on `spec-app`** (the contract), **not** `impl-app` (the policy).
5. **`bin-rpi` / `bin-zephyr` become thin composition roots** — each depends on `impl-app` (to
   `run`) + its platform adapter (to `build` the `Platform`) + `spec-app` (for `RunConfig`). The
   binaries own the concretion wiring; `impl-app` never names a concrete transport, publisher, or
   broker.

The `Platform` **trait keeps its name** — once `spec-app` owns the interface, "platform" is left to
mean exactly one thing (the concrete adapter), so there is no remaining ambiguity.

## Consequences

- **DIP realized at every layer.** High-level (`impl-app`) and low-level (the platform adapters)
  both depend *down* onto the abstraction (`spec-app`); neither depends on the other. The
  detail→policy coupling (`bin-zephyr-platform → impl-app`) inverts to `bin-zephyr-platform →
  spec-app`.
- **`impl-app` ⊥ `lib-mqtt` / `lib-net`.** The policy carries zero hardware/service dependencies;
  the `impl-app → lib-mqtt` and `impl-app → lib-net` graph edges disappear.
- **`spec-app` is the stable, abstract spine** (Stable-Dependencies / Stable-Abstractions): the
  most-depended-on, least-changing crate — pure interface + value types. This relieves today's
  tension where `impl-app` is *both* heavily depended on *and* volatile (algorithm + an adapter).
- **Composition roots know the concretions — by design.** The composition root is the one place
  permitted to see concrete detail; the concretion knowledge concentrates in the platform leaves
  (and, for lifecycle wiring, the thin bins), out of both the policy and — via the platform crate —
  the entrypoint. It is *not* abstracted to zero (something must link the broker); it is confined.
- **Testability.** A mock `Platform` depends only on `spec-app`, not the policy crate; the run loop
  is unit-testable against the contract with no concretions linked.
- **Gate/tooling updates.** `.ci/crate-graph.toml` roster gains `spec-app` and `bin-rpi-platform`;
  `deny.toml`'s `impl-app` wrapper list shrinks (adapters no longer depend on `impl-app`);
  `spec-app` needs no layering guard — like the foundation crates it is meant to be depended on
  widely; the orphan heuristic (keyed on the `lib-` prefix) is broadened so a non-`lib-` contract
  crate is covered; `just depgraph` is regenerated.
- **Behavior-preserving.** Pure crate-boundary + injection-wiring moves — no wire bytes, poll logic,
  or runtime behavior change, in the same category as the #289 folds.

## Revisit trigger

- A **third platform target** appears (validates or stresses the single `Platform` port), or
- The `Platform` port needs to **fan out into finer-grained ports** (separate clock / transport /
  publisher seams) — at which point `spec-app` may split, or
- `Publisher` / `SerialTransport` are pulled **up into `spec-app`** for a single pure-ports crate
  (deferred now: `lib-io` and `lib-mapping` legitimately own their trait *and* its impls/mocks
  today, so inverting them is higher churn for marginal gain).
