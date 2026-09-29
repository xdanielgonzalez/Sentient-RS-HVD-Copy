/// Seam over the actual disk write path, so a disk-full (`ENOSPC`)/IO-error condition is
/// host-testable against a fake backend without needing a real full disk. The production backend
/// (a thin wrapper over `std::fs::File`) and a fault-injecting fake both implement this — see
/// `impl_disk_full.rs`.
pub trait WriteBackend: Send {
    /// Append `buf` to the currently-open file.
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
    /// Flush + `fsync` the currently-open file.
    fn sync(&mut self) -> std::io::Result<()>;
    /// Bytes written to the currently-open file so far.
    fn len(&self) -> u64;
    /// True if nothing has been written to the currently-open file yet.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
