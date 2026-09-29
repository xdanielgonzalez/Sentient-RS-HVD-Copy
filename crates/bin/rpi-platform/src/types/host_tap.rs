use super::dds_tap::DdsTap;
use super::disk_tap::DiskTap;
use super::greengrass_tap::GreengrassTap;
use super::mesh_tap::MeshTap;

/// One configured secondary-publish tap. Enum delegation (not `Box<dyn>`) — mirrors the MCU-side
/// convention (CLAUDE.md "Embedded / Firmware Engineering Rules"); this crate is std-only on the
/// RPi `host` path but keeping the same dispatch shape avoids a second idiom in the codebase.
#[derive(Clone)]
pub enum HostTap {
    Disk(DiskTap),
    Greengrass(GreengrassTap),
    Mesh(MeshTap),
    /// DDS/RTPS (MIL-STD-3071) transport (#324, WP6) — present in every bus's `SinkSet` only when
    /// `[dds]` is configured (`runtime::supervise` builds it conditionally; see `DdsTap`'s doc
    /// comment).
    Dds(DdsTap),
}
