//! Core 1 offload for the Seplos parser/poll leg (`core1-parser` feature, default OFF).
//!
//! Only WHICH core runs [`crate::run_task`] changes: core 1 runs the parser/poll leg, core 0 keeps
//! the publish drain leg ([`crate::drain_task`]). Both share [`crate::MQTT_CHANNEL`], safe across
//! cores because `CriticalSectionRawMutex` is hardware-spinlock-backed on the RP2350 (embassy-rp's
//! SIO spinlocks). The channel boundary is the design; core count is a deployment detail — the
//! same wiring collapses to one core with no code change on a single-core target.
//!
//! Boots a second `embassy_executor::Executor` on core 1 via `spawn_core1` + a `'static`
//! `Stack<SIZE>`, running exactly one task: [`crate::run_task`].

use embassy_executor::Executor;
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::peripherals::CORE1;
use embassy_rp::Peri;
use static_cell::StaticCell;

use crate::{run_task, Rp2350Platform, RunConfig};

/// Core 1's stack, sized for one `embassy_executor::Executor` running exactly one task
/// (`run_task`) — matches embassy-rp's own multicore-doc example's `Stack<4096>` demo size; NOT
/// measurement-derived (TODO: measure via `-Zemit-stack-sizes` + the stack-analyzer before this
/// leaves demo-scaffold status, per the repo's "size stacks by measurement" rule).
const CORE1_STACK_SIZE: usize = 4096;

static CORE1_STACK: StaticCell<Stack<CORE1_STACK_SIZE>> = StaticCell::new();
static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();

/// Spawn `run_task(platform, cfg)` on its own executor on core 1. Consumes `core1` (the
/// embassy-rp `CORE1` peripheral singleton) and returns once core 1 has been handed off — core 1
/// itself spins in `Executor::run` forever after that, same as core 0's own executor does today.
pub fn spawn_parser_core1(core1: Peri<'static, CORE1>, platform: Rp2350Platform, cfg: RunConfig) {
    let stack = CORE1_STACK.init(Stack::new());
    spawn_core1(core1, stack, move || {
        let executor = CORE1_EXECUTOR.init(Executor::new());
        executor.run(|spawner| {
            spawner.spawn(
                run_task(platform, cfg)
                    .expect("failed to create run_task token on core1 (pool exhausted)"),
            );
        });
    });
}
