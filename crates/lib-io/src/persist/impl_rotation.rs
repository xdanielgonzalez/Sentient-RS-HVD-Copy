use std::io;
use std::path::PathBuf;

use super::prelude::*;

impl JsonlWriter {
    /// Roll to a new active file once the active file has reached `policy.max_file_bytes`. Not
    /// fatal if rotation itself fails (logged and skipped) — the active file just keeps growing
    /// past the size target rather than dropping/blocking telemetry over a rotation hiccup.
    pub(crate) fn maybe_rotate(&mut self) {
        if self.state.bytes_written < self.policy.max_file_bytes {
            return;
        }
        if let Err(e) = self.rotate() {
            log::warn!("lib-persist: rotation failed (active file keeps growing): {e}");
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        if let Some(backend) = self.backend.as_mut() {
            let _ = backend.sync();
        }
        self.backend = None;
        let active = self.active_path();
        let next_index = self.next_rotation_index()?;
        let rotated = self.rotated_path(next_index);
        std::fs::rename(&active, &rotated)?;
        self.state.files_rotated += 1;
        self.prune_rotated()?;
        // Reopen a fresh active file eagerly so the very next write doesn't pay the open cost
        // mid-line; if this fails, `write_line`'s next call reopens lazily via `ensure_open`.
        self.ensure_open()
    }

    fn rotated_path(&self, index: usize) -> PathBuf {
        self.policy
            .dir
            .join(format!("{}.{index}.jsonl", self.policy.file_prefix))
    }

    fn next_rotation_index(&self) -> io::Result<usize> {
        let max_existing = self
            .list_rotated()?
            .into_iter()
            .map(|(idx, _)| idx)
            .max()
            .unwrap_or(0);
        Ok(max_existing + 1)
    }

    /// Every rotated segment currently on disk for this writer's `file_prefix`, as `(index, path)`
    /// pairs sorted ascending by index (oldest first). A monotonic index (not a shift-rename chain)
    /// so this is recoverable across process restarts without any renumbering.
    pub(crate) fn list_rotated(&self) -> io::Result<Vec<(usize, PathBuf)>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.policy.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
        };
        let prefix = format!("{}.", self.policy.file_prefix);
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some(idx_str) = rest.strip_suffix(".jsonl") else {
                continue;
            };
            if let Ok(idx) = idx_str.parse::<usize>() {
                out.push((idx, entry.path()));
            }
        }
        out.sort_by_key(|(idx, _)| *idx);
        Ok(out)
    }

    /// Delete rotated segments past `policy.max_files`, oldest first — bounds this writer's total
    /// on-disk footprint to `max_file_bytes * (max_files + 1)` (the active file plus every
    /// retained rotated segment).
    fn prune_rotated(&mut self) -> io::Result<()> {
        let rotated = self.list_rotated()?;
        if rotated.len() <= self.policy.max_files {
            return Ok(());
        }
        let excess = rotated.len() - self.policy.max_files;
        for (_, path) in rotated.into_iter().take(excess) {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Delete the single oldest rotated segment (the ENOSPC recovery path, #56 — see
    /// `impl_disk_full.rs`). Returns `Ok(true)` if a segment was deleted, `Ok(false)` if none
    /// existed to delete.
    pub(crate) fn delete_oldest_rotated(&mut self) -> io::Result<bool> {
        match self.list_rotated()?.into_iter().next() {
            Some((_, path)) => {
                std::fs::remove_file(&path)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}
