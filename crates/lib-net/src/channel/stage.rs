//! The stage abstraction: an independently-hardenable unit of the pipeline.
//!
//! A [`Stage`] consumes one input boundary message and produces one output (or a typed error). It
//! owns no global state and reaches nothing outside its input — so it is testable by feeding an
//! input and asserting the output (byte-exact, at the channel boundary), and it can run as its own
//! task or as an isolated process wired only by channels. Composition = connect stages with the
//! typed channels from [`crate::channel::channel`].

/// One transformation step in the pipeline. `In`/`Out` are boundary messages (or domain types
/// derived from them); `Error` is the stage's typed fault surface (fail-closed, no panics).
#[allow(async_fn_in_trait)]
pub trait Stage {
    /// The boundary message this stage consumes.
    type In;
    /// The boundary message this stage produces.
    type Out;
    /// The stage's typed error.
    type Error;
    /// Process one input into one output, or fail closed with a typed error. Must not panic.
    async fn process(&mut self, input: Self::In) -> Result<Self::Out, Self::Error>;
}

use crate::channel::channel::{Receiver, Sender};

/// Wire a stage between two channels and run it forever: receive `In`, process, send `Out`. This is
/// the generic connective loop — the same wiring drives every stage, whether it runs as a task or
/// its own process. A stage error is skipped (the stage owns its diagnostics); the loop ends when
/// the input channel closes (`recv` yields `None`) or the output channel closes.
pub async fn drive<St, Rx, Tx>(mut stage: St, mut rx: Rx, mut tx: Tx)
where
    St: Stage,
    Rx: Receiver<Item = St::In>,
    Tx: Sender<Item = St::Out>,
{
    while let Some(input) = rx.recv().await {
        if let Ok(out) = stage.process(input).await {
            if tx.send(out).await.is_err() {
                break; // downstream closed
            }
        }
    }
}
