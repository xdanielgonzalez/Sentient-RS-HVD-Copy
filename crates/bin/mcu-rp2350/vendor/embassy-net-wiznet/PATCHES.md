# Vendored `embassy-net-wiznet` — SENTIENT patch record

**Upstream:** `embassy-net-wiznet` **0.3.0** (crates.io / github.com/embassy-rs/embassy), published
2026-03-20, MIT OR Apache-2.0.
**Source of this tree:** the pristine crates.io 0.3.0 crate, copied verbatim (`.cargo-ok` /
`.cargo_vcs_info.json` registry bookkeeping files stripped; everything else byte-identical).

**Modified file** — one, `src/device.rs`, function `WiznetDevice::read_frame`. Everything else in
the tree is untouched. The edit is fenced with `// --- SENTIENT rx-header-corruption patch (upstream
PR #5706) ---` / `// --- end SENTIENT patch ---` markers.

## Why

RTT-confirmed on hardware (6/6 reset cycles) on our bench: the on-board discrete W5500 module (SPI0
at 16 MHz over jumper wires — see `crates/bin/mcu-rp2350/ns/src/boot_w5500.rs`) intermittently
returns a corrupted 2-byte RX frame-length header (`0xFFFF` in 5/6 crashes, `0x841F` once). Pre-patch
`read_frame` unconditionally computed `u16::from_be_bytes(frame_bytes) as usize - 2` and then sliced
`frame[..expected_frame_size]` against a fixed 1514-byte (`MTU`) buffer:

- a header `< 2` **underflows** the subtraction to (near) `usize::MAX`;
- any header whose `- 2` exceeds `frame.len()` (1514) **panics** the slice index regardless.

Both are a hard panic. This board's `panic-probe` is configured `features = ["print-defmt"]` only
(no reset-on-panic), and `ns` installs no custom HardFault handler, so the panic traps into the
default looping HardFault handler: the single embassy executor starves, `POLL_HEARTBEAT` stops
advancing, `watchdog_task`'s 8s hardware watchdog stops being fed, and the board reboots ~6-8s later
(measured panic→reboot gaps: 6.2/6.6/7.1/7.7/7.7s). `boot_w5500.rs` already dropped the SPI clock
50 MHz → 16 MHz to reduce the corruption rate, but 16 MHz alone does not eliminate it — this patch
makes the failure mode itself non-fatal (defense in depth with the frequency choice, not a
replacement for it).

**This exact bug is already fixed upstream, but never released.** It was reported and fixed against
main **after** 0.3.0 shipped:

- Fix: [embassy-rs/embassy#5706](https://github.com/embassy-rs/embassy/pull/5706), "embassy-net-wiznet:
  fix integer underflow panic in `read_frame()`", commit `14592448e1cc579d558fc3423343d5a1c3587345`,
  merged 2026-03-28. Upstream's own repro (STM32L432 + W5500 at 40 MHz, SPI clock glitches corrupting
  the header → underflow panic `range end index ... out of range for slice of length 1514`) matches
  our hardware failure mode exactly.
- Related open upstream issues: [#3509](https://github.com/embassy-rs/embassy/issues/3509) (W5500 SPI
  failures at higher SPI frequencies on RP2350-class hardware — directly relevant to our 16 MHz
  choice) and [#5708](https://github.com/embassy-rs/embassy/issues/5708) (STM32 SPI CLK hi-Z root
  cause of the header corruption).
- crates.io `max_version` for `embassy-net-wiznet` is still 0.3.0 as of this patch — there is no
  released version carrying the fix to upgrade into, hence vendoring rather than a version bump.

## The patch (mirrors upstream PR #5706 verbatim in logic)

`src/device.rs::WiznetDevice::read_frame`:

1. Read the 2-byte header into `raw: usize` **without** subtracting yet.
2. If `raw < 2`: the header is corrupt. Advance the RX read pointer past the already-read header
   bytes (`set_rx_read_ptr`), issue `Command::Receive` to hand the (still-buffered) rest of this
   record back to the chip, and return `Ok(0)` — the same benign "nothing to deliver" outcome the
   function already returns for `rx_size == 0`. `Runner::run` (`src/lib.rs`) already handles `Ok(0)`
   via `rx_chan.rx_done(0)`, and smoltcp discards a zero-length frame, so no caller-side change is
   needed. Returning without resyncing the pointer would leave the same corrupt bytes to be
   re-read forever; committing `Command::Receive` on the still-advanced pointer differs from
   upstream only in that upstream's board never revisits the *same* garbage (this line only
   advances past the 2 header bytes, matching upstream's `set_rx_read_ptr(read_ptr)` call at that
   point — see upstream diff, reproduced from the PR review below).
3. Otherwise, compute `expected_frame_size = raw - 2` (now underflow-safe) and cap the actual read to
   `read_len = expected_frame_size.min(frame.len())` — so even a header that is `>= 2` but still
   implausibly large (chip says more bytes than our 1514-byte MTU buffer holds) can no longer index
   out of bounds. If the chip claims more bytes than we read, the read pointer is advanced past the
   unread remainder (`read_ptr.wrapping_add(expected_frame_size - read_len)`) so it stays in sync
   with the chip's own bookkeeping for the next frame.
4. The function now returns `read_len` (previously `expected_frame_size`) — identical to the old
   value on every non-corrupt frame (the `.min()` is a no-op whenever the header is valid), so the
   happy path is byte-for-byte unchanged.

### `warn!` logging

Upstream's PR #5706 logs the discard via embassy's shared internal `warn!` macro (defined in a
`mod fmt;` boilerplate most embassy crates carry). This particular 0.3.0 tree has **no** `mod fmt` /
`warn!` at all — its only existing precedent for conditional logging is the `InvalidChipVersion`
branch in `WiznetDevice::new`, which calls `defmt::error!(...)` directly behind
`#[cfg(feature = "defmt")]` (the crate already carries `defmt` as an optional dependency gated by its
own `defmt` feature — see `Cargo.toml`). This patch follows that existing in-tree precedent exactly:
`#[cfg(feature = "defmt")] defmt::warn!("wiznet rx: bogus frame size {}, discarding", raw);`. Note
our `crates/bin/mcu-rp2350/Cargo.toml` deliberately does NOT enable this crate's own `defmt` feature
(see that file's `embassy-net-wiznet` comment), so on our current build this line compiles out
entirely and emits nothing — the fail-safe (no panic) is unconditional; the log line is dormant until
that feature is turned on.

No corruption counter was added (a stronger no-panic + `warn!`-log variant was considered mid-patch
and explicitly rejected in favor of staying byte-identical to upstream's own fix, so that a future
upgrade to a real release carrying PR #5706 is a clean drop of this whole `vendor/` directory rather
than a diff to reconcile).

## How it is consumed

Via `[patch.crates-io]` **path** override in `crates/bin/mcu-rp2350/Cargo.toml` (a git source is
forbidden by the root `deny.toml`'s `unknown-git = "deny"`; a path patch keeps the source
registry-clean, same reasoning as `vendor/dust_dds` at the repo root — see that crate's own
`PATCHES.md` for the identical convention this mirrors). This is a separate, **excluded** cargo
workspace (`crates/bin/mcu-rp2350`, its own `Cargo.lock`) — the patch does not apply to, and is not
needed by, the root `sentient-rs` workspace's own `lib-net` (which pins a different, older
`embassy-net` generation there and is unaffected).

## Removal plan

Delete `vendor/embassy-net-wiznet` and the `[patch.crates-io]` block in
`crates/bin/mcu-rp2350/Cargo.toml` as soon as a released version of `embassy-net-wiznet` greater than
0.3.0 carrying PR #5706 is published on crates.io — bump the `embassy-net-wiznet` version pin in that
same manifest at the same time.
