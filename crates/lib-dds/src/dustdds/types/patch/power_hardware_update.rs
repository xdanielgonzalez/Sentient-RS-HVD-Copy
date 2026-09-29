use dust_dds::infrastructure::type_support::TypeSupport;

use super::clock_monotonic::ClockMonotonic;
use super::energy_storage_state::EnergyStorageState;
use super::engine_state::EngineState;
use super::fuel_state::FuelState;
use super::generator_state::GeneratorState;
use super::power_electronics_state::PowerElectronicsState;
use super::thermal_state::ThermalState;

/// dust_dds patch of `tms::PowerHardwareUpdate` (topic `PowerHardwareUpdate`) —
/// `@topic @extensibility(APPENDABLE)` (idl:2359). `device_id` is the instance `@key`; `timestamp`
/// is a required `ClockMonotonic`. The six operating-state members are each `@optional` nested
/// structs → `Option<T>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer emits a 1-byte
/// presence flag before each optional in declared order, wp-serializer; the deserializer mirrors it,
/// wp-deserializer). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerHardwareUpdate`. Derive-only: no hand impl, so this is a single-type
/// file by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(extensibility = "appendable")]
pub struct PowerHardwareUpdate {
    #[dust_dds(key)]
    pub device_id: String,
    pub timestamp: ClockMonotonic,
    #[dust_dds(optional)]
    pub engine: Option<EngineState>,
    #[dust_dds(optional)]
    pub fuel: Option<FuelState>,
    #[dust_dds(optional)]
    pub generator: Option<GeneratorState>,
    #[dust_dds(optional)]
    pub energy_storage: Option<EnergyStorageState>,
    #[dust_dds(optional)]
    pub power_electronics: Option<PowerElectronicsState>,
    #[dust_dds(optional)]
    pub thermal: Option<ThermalState>,
}
