use heapless::String;
use serde::{Deserialize, Serialize};

/// Static inverter rating strings decoded from a QPIRI-style ratings query. Novel canonical type
/// (no MIL-STD-3071 analogue); PROVISIONAL. Bounded `heapless::String`s keep it `no_std`/no-alloc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InverterRatings {
    pub battery_type: String<16>,
    pub variant: String<16>,
    pub output_mode: String<24>,
}
