/// One scripted response to a read exchange.
#[derive(Debug, Clone, Copy)]
pub enum MockStep<'a> {
    /// Deliver these bytes (should end in the `\r` delimiter for a well-formed frame).
    Reply(&'a [u8]),
    /// Simulate a silent/non-responding bus.
    Timeout,
    /// Simulate a response that overflows the caller's buffer before the delimiter.
    ShortTransfer,
    /// Simulate a transport/device read failure.
    DeviceError,
    /// Deliver garbage bytes (no valid frame; still ends in `\r` so read_until returns).
    Garbage(&'a [u8]),
}
