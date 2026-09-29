use lib_io::host::SerialKind;
use lib_io::transport::SerialParams;

/// The concrete port + line params + per-address timeout a scan run resolved from CLI flags
/// and/or the loaded config — see `crate::scan::ScanArgs::resolve`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBus {
    pub port: String,
    pub kind: SerialKind,
    pub params: SerialParams,
    pub timeout_ms: u32,
}
