//! Hand-written wire binding for `StorageUpdate` topic, borrowing canonical battery reading +
//! device context. Implements the generated `StorageUpdateSrc` trait and nested field-chain traits
//! to emit TMS JSON via the flat [`crate::wire`] serializer (zero-copy, constant-frame, iterative
//! over cells). Math is ported from [`crate::transform::ToStorageUpdate`] (voltage/SOC/energy
//! conversions, cell/temperature aggregations).

mod impl_storage_update;

pub use impl_storage_update::StorageUpdateWire;
