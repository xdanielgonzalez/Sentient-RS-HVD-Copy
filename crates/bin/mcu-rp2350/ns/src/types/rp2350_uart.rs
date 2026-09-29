use super::buffered_uart_adapter::BufferedUartAdapter;

/// Concrete monomorphization of the generic seam types — embassy tasks cannot be generic, so the
/// spawned task functions need one concrete instantiation per platform (this crate's only consumer
/// of `bin_mcu_platform::McuPlatform`/`ChannelPublisher`).
pub(crate) type Rp2350Uart = BufferedUartAdapter;
