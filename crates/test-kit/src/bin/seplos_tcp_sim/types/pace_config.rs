/// Wire-rate pacing configuration for reply writes (`--pace <baud>` / `--turnaround-ms <n>`).
///
/// `baud == 0` (or the flag being absent entirely) means "no pacing": replies are written
/// instantly, as before this feature existed. A real Seplos RS-485 exchange at 19200 8N1 moves
/// 10 bits/byte (1 start + 8 data + 1 stop), so `--pace 19200` paces reply bytes at that rate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PaceConfig {
    /// Wire baud rate to pace reply bytes at. `0` disables pacing.
    pub(crate) baud: u32,
    /// Simulated device-processing / RS-485 turnaround delay applied before writing the reply,
    /// in milliseconds.
    pub(crate) turnaround_ms: u64,
}
