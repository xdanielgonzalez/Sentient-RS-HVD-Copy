use dust_dds::infrastructure::type_support::TypeSupport;

use super::energy_storage_info::EnergyStorageInfo;
use super::engine_info::EngineInfo;
use super::fuel_info::FuelInfo;
use super::generator_info::GeneratorInfo;
use super::power_electronics_info::PowerElectronicsInfo;
use super::synchronous_machine_coefficients::SynchronousMachineCoefficients;
use super::thermal_info::ThermalInfo;

/// dust_dds patch of `tms::PowerHardwareInfo` — `@nested @extensibility(APPENDABLE)` (idl:2305),
/// member-only (`DeviceInfo.power_hardware`, itself `@optional`). Every member is an `@optional`
/// nested struct → `Option<T>` + `#[dust_dds(optional)]` (wp-derive; the XCDR2 serializer emits a
/// 1-byte presence flag before each, wp-serializer). Field order = the IDL member order, matching
/// `lib_tms::generated::PowerHardwareInfo`. Derive-only: no hand impl, so this is a single-type file
/// by design.
#[derive(TypeSupport, Clone, Debug, PartialEq)]
#[dust_dds(nested, extensibility = "appendable")]
pub struct PowerHardwareInfo {
    #[dust_dds(optional)]
    pub engine: Option<EngineInfo>,
    #[dust_dds(optional)]
    pub fuel: Option<FuelInfo>,
    #[dust_dds(optional)]
    pub generator: Option<GeneratorInfo>,
    #[dust_dds(optional)]
    pub energy_storage: Option<EnergyStorageInfo>,
    #[dust_dds(optional)]
    pub power_electronics: Option<PowerElectronicsInfo>,
    #[dust_dds(optional)]
    pub thermal: Option<ThermalInfo>,
    #[dust_dds(optional)]
    pub synchronous_machine_model: Option<SynchronousMachineCoefficients>,
}
