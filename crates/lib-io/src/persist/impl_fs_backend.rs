use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

use super::prelude::*;

impl FsBackend {
    /// Open (create if absent, append-position) the file at `path` for writing, seeding `len` from
    /// its current on-disk size — correct even if this process is restarting onto a
    /// partially-written active file left over from a previous run.
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }
}

impl WriteBackend for FsBackend {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.file.write_all(buf)?;
        // Only counted after a fully successful write: on error, `Write::write_all` gives no
        // guarantee how many bytes actually landed, so `len` must not be advanced on a failed
        // attempt (see `impl_disk_full.rs`'s ENOSPC-retry path, which reuses this same backend).
        self.len += buf.len() as u64;
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.file.sync_data()
    }

    fn len(&self) -> u64 {
        self.len
    }
}
