use super::mock_step::MockStep;
use crate::transport::Direction;
use heapless::{Deque, Vec};

/// A scripted transport. `N` bounds the script length.
pub struct MockSerial<'a, const N: usize> {
    pub(crate) script: Deque<MockStep<'a>, N>,
    /// Every byte the driver wrote, in order — assert byte-exact request frames.
    pub writes: Vec<u8, 512>,
    /// Every direction transition, in order — assert Transmit-then-Receive sequencing.
    pub directions: Vec<Direction, 32>,
}
