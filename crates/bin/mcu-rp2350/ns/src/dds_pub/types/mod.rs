//! Type definitions for the `dds_pub` module (RTPS user-data publication leg). Per the repo's
//! Go-style data/behavior split, the type definitions live here under `types/`; their `impl` blocks
//! live in the sibling `impl_*.rs` files at the `dds_pub` module root.

pub(crate) mod dds_publisher;

pub(crate) use dds_publisher::DdsPublisher;
