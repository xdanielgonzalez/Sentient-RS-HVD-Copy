//! Type definitions for the `spdp` module (RTPS SPDP discovery-announcement leg). Per the repo's
//! Go-style data/behavior split, the type definitions live here under `types/`; their `impl` blocks
//! live in the sibling `impl_*.rs` files at the `spdp` module root.

pub(crate) mod byte_cursor;
pub(crate) mod spdp_announce;

pub(crate) use byte_cursor::ByteCursor;
pub(crate) use spdp_announce::SpdpAnnounce;
