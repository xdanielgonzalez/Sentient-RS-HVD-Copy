# lib-net-embassy

**Milestone-1/2 host-testable spike** proving an alternative owned network stack: `embassy-net`
(0.6.0) + smoltcp, adapted onto our existing [`lib_link::NetLink`]/`WireFrame` boundary via
`embassy-net-driver-channel`, as the device-hosted MQTT broker's TCP/IP stack — built and
host-tested before any emulator run.

## Status: deliberately retained, consumed by no crate

**No workspace crate depends on `lib-net-embassy`** (verified: `rtk grep lib-net-embassy
crates/*/Cargo.toml` — the only other hit is a comment in `test-acceptance/Cargo.toml` explaining
an unrelated `workspace.dependencies` default-features note, not an actual dependency edge). It is
an *alternative* to [`lib_link::stack::NetStack`] (the bespoke smoltcp-driven-directly stack that
**is** wired into the shipping Zephyr broker pump, `bin/zephyr/platform/src/broker_pump.rs`): this
crate instead sits `embassy-net` itself on top of `NetLink`, trading the hand-rolled poll-driven
`NetStack` for embassy's own `Stack`/`Runner`/task-based execution model.

It is kept in the workspace as the **MCU owned-stack proof-of-concept**, per the *Platform &
Deployment* doctrine in the repo's `CLAUDE.md`: the owned embassy-net/smoltcp path is MCU-only
since the 2026-07-12 pivot to shipping Yocto Linux on the Pi (story #159) — the Pi uses the mainline
kernel's TCP/IP stack via tokio/`std::net` instead. If the MCU path ever needs embassy-net's
task-based stack instead of (or alongside) `lib-link`'s `NetStack`, this crate is the starting
point; until then it stays an unconsumed, independently-tested spike rather than dead code deleted
outright, because it is load-bearing evidence that the `NetLink` seam composes with a *second*
independent stack implementation, not just the one `lib-link` ships.

- **Built by `just build-no-std`** — part of the `no_std_net` target set
  (`-p lib-channel -p lib-link -p lib-net-embassy`) in the `justfile`, so its `no_std` core
  (`adapter`/`config`/`pump`) is proven to compile for `thumbv7em-none-eabihf` on every CI run.
- **Excluded from `just coverage`** — `cargo-tarpaulin` can't build the `host`-feature integration
  tests that carry this crate's real coverage (`tests/host_bring_up.rs`, `tests/broker_pump.rs`),
  so the coverage recipe passes `--exclude lib-net-embassy`.

## Running

```sh
# no_std core, MCU target (part of `just build-no-std`):
cargo build -p lib-net-embassy --no-default-features --target thumbv7em-none-eabihf

# host bring-up + broker-pump integration tests (feature `host`):
cargo test -p lib-net-embassy --features host
```

## Layout

- `src/adapter/` — bridges [`lib_link::NetLink`] to `embassy-net-driver-channel`'s `Runner`
  (`link.recv()` -> the channel's RX queue, the channel's TX queue -> `link.send()`), so the same
  `NetLink` driver (Zephyr NIC, W5500, `LoopbackLink`, or the host's `HostLink`) can sit under
  `embassy_net::Stack` instead of (or alongside) `lib_link::stack::NetStack`.
- `src/config/` — the static IPv4 `StackConfig` (MTU, socket-count constants) the bring-up and the
  adapter agree on by construction.
- `src/pump/` — the broker pump: drives `lib_link::BrokerCore` over any
  `embedded_io_async`-shaped transport. Split into `drain_and_write` (generic purely over
  `embedded_io_async::Write`, `no_std`/no-`alloc`, unit-testable against an in-memory mock) and
  `broker_slot_cycle` (one client slot's accept -> recv/feed/drain -> close lifecycle).
- `src/host/` (feature `host`, std-only) — reusable, link-agnostic `embassy_net::Stack` bring-up
  building blocks (`build_stack`); the concrete `#[embassy_executor::task]` wrapper functions
  themselves must be monomorphic, so they live at the test call site instead
  (`tests/host_bring_up.rs`).
- `tests/host_bring_up.rs` — a two-peer in-memory `NetLink`, an `embassy-executor` `arch-std`
  executor on its own thread, and a real `TcpSocket` connect/accept over it: proves the M1 adapter
  composes end-to-end.
- `tests/broker_pump.rs` — the M2 broker pump (`drain_and_write` / `broker_slot_cycle`) driven
  against a real `embassy_net::tcp::TcpSocket`.

[`lib_link::NetLink`]: ../lib-link/src/types
[`lib_link::stack::NetStack`]: ../lib-link/src/stack
