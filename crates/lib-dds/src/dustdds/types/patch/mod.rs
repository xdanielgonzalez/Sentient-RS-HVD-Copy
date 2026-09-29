//! dust_dds **patch types** — one per TMS topic (plus their member-only nested types/enums),
//! deriving `TypeSupport` with the topic's IDL field order + extensibility + `@key`. They exist so
//! the adapter can obtain each topic's `DynamicType` (`M::get_type()`) to decode our pre-serialized
//! XCDR2 payload into a `DynamicData`, and reconstitute a typed sample (`M::create_sample`) for the
//! `DataWriter`/`DataReader`. For every optional-free topic the WP3 cross-oracle proves dust_dds's
//! serialization of that value equals our chains byte-for-byte.
//!
//! WP4 landed `Heartbeat` (the integration-test topic + the simplest published type). WP6 (#324, Pi
//! integration) adds `DcMeasurementUpdate` (the `DdsTap` telemetry topic the runtime smoke test
//! proves end to end) and `Reply` (the request-reply response leg's outbound type) — both are
//! completely `@optional`-free, so the WP4 rung-C round-trip is byte-identical for them today.
//!
//! **`EnergyStartStopRequest`/`PowerSwitchRequest` are NOT patched here — confirmed, not assumed.**
//! Both carry one `@optional switch_conditions` member, and `dust_dds_derive` 0.15.0's
//! `#[dust_dds(optional)]` does **not** accept an `Option<T>` Rust field (verified empirically: `cargo
//! check` on a trial patch rejected `Option<PowerSwitchCloseConditions>` with "the trait bound
//! `Option<_>: DataStorageMapping` is not satisfied" — `DataStorageMapping` has no `Option<T>` impl in
//! `dust_dds-0.15.0/src/xtypes/data_storage.rs`; the attribute only swaps in `T::default()` for an
//! absent member, which cannot represent true absence). This is the WP0/WP4-documented "`dust_dds`
//! `@optional` quirk: inline-no-flag, cannot encode ABSENT" made concrete — its resolution is WP5's
//! CycloneDDS arbitration, not a WP6 workaround. `bin_rpi`'s `tasks::dds::request_listener` still
//! calls `DdsParticipant::subscribe` for both topics (future-proofing against a WP5-era follow-up
//! that lands their patchs); today that returns `DdsError::UnsupportedTopic` and the task logs +
//! idles, never a crash.
//!
//! **WP6.5 writer-map completion — all 8 topics now UNBLOCKED (#332 `@optional` overhaul).** WP6.5
//! set out to finish the writer map (`DeviceInfo`, `PowerPortState`, `EnergyStartStopState`,
//! `DcLoadSharingState`, `ActiveMicrogridControllerState`, `PowerHardwareUpdate`, `StorageUpdate`,
//! `ActiveDiagnosticState`). Only **`DcLoadSharingState`** was `@optional`-free across its entire
//! transitive closure and landed first; the other **seven** each carry `@optional` members (1 for
//! `ActiveMicrogridControllerState`/`EnergyStartStopState`, ~30 across `DeviceInfo`'s closure) and were
//! previously hard-blocked at the `dust_dds_derive` layer. That block is now **lifted** by the
//! vendored `dust_dds` `@optional` overhaul (#332): the fork's `TypeSupport` derive keys on
//! `field_attributes.optional` and maps `into_storage`/`try_from_storage` against the **inner** `T`
//! (member-key set ⇒ `Some`, unset ⇒ `None`), so an `Option<T>` field no longer needs a
//! `DataStorageMapping<Option<T>>` impl and compiles; and the XCDR2 serializer/deserializer now
//! carry a real **presence flag** on the wire. For every DECLARED `@optional` member, in declared
//! order (not just contiguous — `EngineState`'s two optionals straddle a required member), the
//! FINAL/APPENDABLE XCDR2 path emits a **1-byte boolean `<is_present>`** — `0x01` followed by the
//! value when present, `0x00` and nothing else when absent (DDS-XTypes 1.3 §7.4.3.5.2 + rule (20)
//! §7.4.3.5.3). No padding precedes the boolean, byte-identical to `lib_tms::cdr::emit_optional`
//! (`crates/lib-tms/src/cdr/mod.rs`). The deserializer mirrors it via an overridable
//! presence-consume whose default reads nothing, so CDR1/PL_CDR wire forms stay byte-unchanged and
//! the three optional-free topics do not regress. See `vendor/dust_dds/PATCHES.md` for the new patch
//! record (alongside the rule-(12) sequence-DHEADER patch it mirrors). All 8 topics + their full
//! transitive closures are patched and wired below; `bin_rpi`'s DDS tasks render + enqueue them
//! through the generic `DdsOutbound`/`DdsHandle` seam and `crate::stack::run_dds_stack`'s pump now
//! maps every one.

mod active_conversion_info;
mod active_diagnostic_state;
mod active_microgrid_controller_state;
mod cable_sense_status;
mod circuit_continuity;
mod circuit_wiring;
mod clock_monotonic;
mod config_id;
mod control_curve;
mod control_hardware_info;
mod control_service_info;
mod conversion_feature;
mod conversion_info;
mod conversion_tap_info;
mod curve2d;
mod dc_load_sharing_parameters;
mod dc_load_sharing_state;
mod dc_measurement_update;
mod dc_power_line_measurement;
mod dc_power_port_measurement;
mod device_info;
mod device_role;
mod diagnostic_trouble_code;
mod distribution_feature;
mod distribution_info;
mod dtc_severity;
mod energy_start_stop_level;
mod energy_start_stop_state;
mod energy_storage_info;
mod energy_storage_state;
mod engine_info;
mod engine_operating_state;
mod engine_state;
mod failure_mode_indicator;
mod fuel_info;
mod fuel_state;
mod generator_info;
mod generator_state;
mod grounding_design_type;
mod grounding_info;
mod heartbeat;
mod load_feature;
mod load_info;
mod load_sharing_info;
mod load_step_response;
mod microgrid_controller_feature;
mod microgrid_controller_info;
mod microgrid_dashboard_feature;
mod microgrid_dashboard_info;
mod parameter_metadata;
mod passive_conversion_info;
mod point2d;
mod power_connector_feature;
mod power_connector_info;
mod power_connector_polarity;
mod power_connector_type;
mod power_device_info;
mod power_electronics_info;
mod power_electronics_state;
mod power_hardware_info;
mod power_hardware_update;
mod power_port_conversion_info;
mod power_port_directionality;
mod power_port_id;
mod power_port_info;
mod power_port_state;
mod power_port_status;
mod power_switch_feature;
mod power_switch_info;
mod power_switch_reason;
mod power_switch_status;
mod power_switch_transition_code;
mod product_info;
mod reply;
mod reply_code;
mod reply_status;
mod source_feature;
mod source_info;
mod storage_cell_state;
mod storage_feature;
mod storage_info;
mod storage_update;
mod synchronous_machine_coefficients;
mod thermal_info;
mod thermal_state;
mod topic_info;

pub use active_conversion_info::ActiveConversionInfo;
pub use active_diagnostic_state::ActiveDiagnosticState;
pub use active_microgrid_controller_state::ActiveMicrogridControllerState;
pub use cable_sense_status::CableSenseStatus;
pub use circuit_continuity::CircuitContinuity;
pub use circuit_wiring::CircuitWiring;
pub use clock_monotonic::ClockMonotonic;
pub use config_id::ConfigId;
pub use control_curve::ControlCurve;
pub use control_hardware_info::ControlHardwareInfo;
pub use control_service_info::ControlServiceInfo;
pub use conversion_feature::ConversionFeature;
pub use conversion_info::ConversionInfo;
pub use conversion_tap_info::ConversionTapInfo;
pub use curve2d::Curve2D;
pub use dc_load_sharing_parameters::DcLoadSharingParameters;
pub use dc_load_sharing_state::DcLoadSharingState;
pub use dc_measurement_update::DcMeasurementUpdate;
pub use dc_power_line_measurement::DcPowerLineMeasurement;
pub use dc_power_port_measurement::DcPowerPortMeasurement;
pub use device_info::DeviceInfo;
pub use device_role::DeviceRole;
pub use diagnostic_trouble_code::DiagnosticTroubleCode;
pub use distribution_feature::DistributionFeature;
pub use distribution_info::DistributionInfo;
pub use dtc_severity::DtcSeverity;
pub use energy_start_stop_level::EnergyStartStopLevel;
pub use energy_start_stop_state::EnergyStartStopState;
pub use energy_storage_info::EnergyStorageInfo;
pub use energy_storage_state::EnergyStorageState;
pub use engine_info::EngineInfo;
pub use engine_operating_state::EngineOperatingState;
pub use engine_state::EngineState;
pub use failure_mode_indicator::FailureModeIndicator;
pub use fuel_info::FuelInfo;
pub use fuel_state::FuelState;
pub use generator_info::GeneratorInfo;
pub use generator_state::GeneratorState;
pub use grounding_design_type::GroundingDesignType;
pub use grounding_info::GroundingInfo;
pub use heartbeat::Heartbeat;
pub use load_feature::LoadFeature;
pub use load_info::LoadInfo;
pub use load_sharing_info::LoadSharingInfo;
pub use load_step_response::LoadStepResponse;
pub use microgrid_controller_feature::MicrogridControllerFeature;
pub use microgrid_controller_info::MicrogridControllerInfo;
pub use microgrid_dashboard_feature::MicrogridDashboardFeature;
pub use microgrid_dashboard_info::MicrogridDashboardInfo;
pub use parameter_metadata::ParameterMetadata;
pub use passive_conversion_info::PassiveConversionInfo;
pub use point2d::Point2D;
pub use power_connector_feature::PowerConnectorFeature;
pub use power_connector_info::PowerConnectorInfo;
pub use power_connector_polarity::PowerConnectorPolarity;
pub use power_connector_type::PowerConnectorType;
pub use power_device_info::PowerDeviceInfo;
pub use power_electronics_info::PowerElectronicsInfo;
pub use power_electronics_state::PowerElectronicsState;
pub use power_hardware_info::PowerHardwareInfo;
pub use power_hardware_update::PowerHardwareUpdate;
pub use power_port_conversion_info::PowerPortConversionInfo;
pub use power_port_directionality::PowerPortDirectionality;
pub use power_port_id::PowerPortId;
pub use power_port_info::PowerPortInfo;
pub use power_port_state::PowerPortState;
pub use power_port_status::PowerPortStatus;
pub use power_switch_feature::PowerSwitchFeature;
pub use power_switch_info::PowerSwitchInfo;
pub use power_switch_reason::PowerSwitchReason;
pub use power_switch_status::PowerSwitchStatus;
pub use power_switch_transition_code::PowerSwitchTransitionCode;
pub use product_info::ProductInfo;
pub use reply::Reply;
pub use reply_code::ReplyCode;
pub use reply_status::ReplyStatus;
pub use source_feature::SourceFeature;
pub use source_info::SourceInfo;
pub use storage_cell_state::StorageCellState;
pub use storage_feature::StorageFeature;
pub use storage_info::StorageInfo;
pub use storage_update::StorageUpdate;
pub use synchronous_machine_coefficients::SynchronousMachineCoefficients;
pub use thermal_info::ThermalInfo;
pub use thermal_state::ThermalState;
pub use topic_info::TopicInfo;
