/// Which Seplos command a parsed request frame carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SimCommand {
    Telemetry,
    Fault,
    SerialNumber,
}
