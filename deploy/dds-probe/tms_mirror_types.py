"""CycloneDDS `IdlStruct` mirrors of the MIL-STD-3071 TMS types used by the golden-fixture topics
(`crates/lib-mapping/tests/wire_cdr_golden_bytes.rs`) + the `@optional` arbitration reproductions.

Field ORDER and Python type mirror `crates/lib-tms/src/generated.rs` exactly (declaration order is
the CDR member order for every type — all 136 TMS IDL types are `@extensibility(APPENDABLE)` with
implicit member IDs; see `crates/lib-tms/src/cdr/mod.rs` module doc). Enum wire values match the
Rust `#[repr]` discriminant order (both sides use the IDL declaration order; DDS-XTypes 1.3 rule (5)
default `@bit_bound` = 32, matched here by leaving enums WITHOUT an explicit `bit_bound` annotation —
`cyclonedds.idl`'s plain `EnumMachine` already serializes a bare Python `Enum` as a 4-byte value,
see `_machinery.py::EnumMachine`).

This module has NO product-code authority — it exists to let a human/agent GENERATE bytes with an
independent, external, authoritative encoder (CycloneDDS) for comparison against the committed
`fixtures/xcdr2/*.hex` goldens (`dump_reference.py`) and for live pub/sub smoke (`subscribe_assert.py`).

PLACEHOLDER SHAPES: `DeviceInfo.{control_hardware,power_hardware,control_parameters,
metric_parameters,control_service,power_device}` and `PowerHardwareUpdate.{engine,fuel,generator,
thermal}` are `Optional[bool]`/`Optional[sequence(bool)]` stand-ins, NOT the real (much larger)
nested types — every golden fixture that uses these topics sets ALL of them to `None` (absent), and
an ABSENT `@optional` serializes as a single presence-flag byte regardless of the member's value type
(the value is only serialized when present), so the stand-in shape is byte-exact for these fixtures
without transcribing the full sub-schemas. `PowerHardwareUpdate.power_electronics` is the ONE
exception — the `power_hardware_inverter` fixture has it PRESENT (`temperature_c`), so it gets the
real `PowerElectronicsState` shape (single `temperature: float32`) to exercise a genuine
present-optional-with-value case, not just the flag.
"""
from dataclasses import dataclass, field
from typing import Optional

from cyclonedds.idl import IdlStruct, IdlEnum
from cyclonedds.idl.annotations import key, appendable
from cyclonedds.idl.types import sequence, int32, uint32, uint16, float32, uint64


# ==================================================================================================
# Enums (declaration order == Rust discriminant order == wire ordinal; see generated.rs). Must
# subclass `cyclonedds.idl.IdlEnum` (its `IdlEnumMeta` metaclass), NOT plain `enum.IntEnum` — the
# builder's `get_idl_annotations()` unconditionally reads `cls.__idl_annotations__`, which only
# `IdlEnumMeta` (like `IdlMeta` for structs) populates.
# ==================================================================================================

class ConfigId(IdlEnum):
    UNKNOWN = 0
    DEFAULTS = 1
    ACTIVE = 2
    ON_REBOOT = 3
    ON_COMMS_LOSS = 4


class ReplyCode(IdlEnum):
    UNKNOWN = 0
    OK = 1
    BAD_REQUEST = 2
    METHOD_NOT_ALLOWED = 3
    CONFLICT = 4
    GONE = 5
    PRECONDITION_FAILED = 6
    INTERNAL_SERVER_ERROR = 7
    NOT_IMPLEMENTED = 8
    SERVICE_UNAVAILABLE = 9


class DeviceRole(IdlEnum):
    UNKNOWN = 0
    MICROGRID_CONTROLLER = 1
    SOURCE = 2
    LOAD = 3
    STORAGE = 4
    DISTRIBUTION = 5
    MICROGRID_DASHBOARD = 6
    CONVERSION = 7
    MONITOR = 8


class EnergyStartStopLevel(IdlEnum):
    UNKNOWN = 0
    ANY = 1
    OFF = 2
    WARM = 3
    IDLE = 4
    READY = 5
    READY_SYNCED = 6
    OPERATIONAL = 7


class PowerSwitchReason(IdlEnum):
    UNKNOWN = 0
    STARTUP = 1
    MANUAL = 2
    AUTO = 3
    REQUEST = 4
    OPERATOR_LOGIN = 5
    AUTO_PROTECTION = 6
    AUTO_LOAD_SHED = 7
    AUTO_VOLTAGE_LOSS = 8
    AUTO_COMMS_LOSS = 9


class PowerSwitchTransitionCode(IdlEnum):
    UNKNOWN = 0
    NONE = 1
    IN_PROGRESS = 2
    PENDING_SYNC = 3
    PENDING_SWITCH_CONDITIONS = 4
    PENDING_INTERLOCK = 5
    COMPLETE = 6
    MANUAL_OVERRIDE = 7
    AUTO_OVERRIDE = 8
    AUTO_LOCK = 9


class FailureModeIndicator(IdlEnum):
    UNKNOWN = 0
    ABOVE_NORMAL_MOST_SEVERE = 1
    BELOW_NORMAL_MOST_SEVERE = 2
    DATA_ERRATIC = 3
    VOLTAGE_HIGH = 4
    VOLTAGE_LOW = 5
    CURRENT_LOW = 6
    CURRENT_HIGH = 7
    # ConditionExists is the value the fixtures actually use — the codegen coverage report /
    # `lib_tms::generated::FailureModeIndicator` has more variants; only the ordinal used below
    # (`CONDITION_EXISTS`) needs to be right for these fixtures. Real ordinal per generated.rs.
    CONDITION_EXISTS = 31


class DtcSeverity(IdlEnum):
    UNKNOWN = 0
    CLEAR = 1
    INFORMATIVE = 2
    PREVENTATIVE = 3
    DEGRADED = 4
    WARNING = 5
    MINOR = 6
    MAJOR = 7


# ==================================================================================================
# Nested (non-topic) appendable types
# ==================================================================================================

@appendable
@dataclass
class ClockMonotonic(IdlStruct, typename="ClockMonotonic", nested=True):
    seconds: uint32
    nanoseconds: uint32


@appendable
@dataclass
class ReplyStatus(IdlStruct, typename="ReplyStatus", nested=True):
    code: ReplyCode
    reason: str


@appendable
@dataclass
class ProductInfo(IdlStruct, typename="ProductInfo", nested=True):
    nsn: str
    gtin: str
    manufacturer_name: str
    model_name: str
    model_number: str
    serial_number: str
    software_version: str
    platform_id: Optional[str] = None


@appendable
@dataclass
class TopicInfo(IdlStruct, typename="TopicInfo", nested=True):
    data_model_version: str
    published_conditional_topics: sequence(str) = field(default_factory=list)
    published_optional_topics: sequence(str) = field(default_factory=list)
    supported_request_topics: sequence(str) = field(default_factory=list)
    extension_topics: Optional[sequence(str)] = None


@appendable
@dataclass
class DcPowerLineMeasurement(IdlStruct, typename="DcPowerLineMeasurement", nested=True):
    voltage: float32
    amperage: float32
    power: float32


@appendable
@dataclass
class DcPowerPortMeasurement(IdlStruct, typename="DcPowerPortMeasurement", nested=True):
    port_number: int32
    line: sequence(DcPowerLineMeasurement)
    power_rate_of_change: float32


@appendable
@dataclass
class AcPowerLineMeasurement(IdlStruct, typename="AcPowerLineMeasurement", nested=True):
    # TRUE IDL order: voltage, frequency, amperage, phase_offset(@optional), real_power,
    # reactive_power — `phase_offset` is NOT last. Python dataclasses require defaults to trail, so
    # `real_power`/`reactive_power` get placeholder defaults too; every caller passes ALL fields
    # explicitly (the placeholder default value is never actually read).
    voltage: float32
    frequency: float32
    amperage: float32
    phase_offset: Optional[float32] = None
    real_power: float32 = 0.0
    reactive_power: float32 = 0.0


@appendable
@dataclass
class AcPowerPortMeasurement(IdlStruct, typename="AcPowerPortMeasurement", nested=True):
    port_number: int32
    line: sequence(AcPowerLineMeasurement)
    phase_angle: sequence(float32)
    real_power_rate_of_change: float32
    reactive_power_rate_of_change: float32


@appendable
@dataclass
class StorageCellState(IdlStruct, typename="StorageCellState", nested=True):
    # TRUE IDL order: cell_number, cycle_counter, state_of_charge(@optional), temperature, voltage —
    # `state_of_charge` is NOT last; `temperature`/`voltage` get placeholder defaults (see module doc).
    cell_number: uint16
    cycle_counter: float32
    state_of_charge: Optional[float32] = None
    temperature: float32 = 0.0
    voltage: float32 = 0.0


@appendable
@dataclass
class PowerElectronicsState(IdlStruct, typename="PowerElectronicsState", nested=True):
    temperature: float32


@appendable
@dataclass
class EnergyStorageState(IdlStruct, typename="EnergyStorageState", nested=True):
    min_temperature: float32
    average_temperature: float32
    max_temperature: float32
    run_time: float32
    storage_cells: sequence(StorageCellState) = field(default_factory=list)


@appendable
@dataclass
class DiagnosticTroubleCode(IdlStruct, typename="DiagnosticTroubleCode", nested=True):
    # TRUE IDL order: port_number(@optional) FIRST, then suspect_parameter, failure_mode,
    # occurrence_count, time_raised, severity, estimate_time(@optional), estimate_severity(@optional),
    # hint. Every non-optional field after `port_number` needs a placeholder default (see module doc);
    # `time_raised` uses `None` as a placeholder even though the true type is non-Optional —
    # callers always pass every field explicitly.
    port_number: Optional[int32] = None
    suspect_parameter: uint32 = 0
    failure_mode: FailureModeIndicator = FailureModeIndicator.UNKNOWN
    occurrence_count: uint32 = 0
    time_raised: ClockMonotonic = None  # type: ignore[assignment]
    severity: DtcSeverity = DtcSeverity.UNKNOWN
    estimate_time: Optional[ClockMonotonic] = None
    estimate_severity: Optional[DtcSeverity] = None
    hint: str = ""


@appendable
@dataclass
class Point2D(IdlStruct, typename="Point2D", nested=True):
    x: float32
    y: float32


@appendable
@dataclass
class Curve2D(IdlStruct, typename="Curve2D", nested=True):
    points: sequence(Point2D) = field(default_factory=list)


# ==================================================================================================
# Topic (top-level) appendable types — one per golden-fixture topic
# (crates/lib-mapping/tests/wire_cdr_golden_bytes.rs::all_fixtures)
# ==================================================================================================

@appendable
@dataclass
class Heartbeat(IdlStruct, typename="Heartbeat"):
    device_id: str
    key("device_id")
    sequence_number: uint32


@appendable
@dataclass
class Reply(IdlStruct, typename="Reply"):
    requesting_device_id: str
    target_device_id: str
    config: ConfigId
    port_number: int32
    request_sequence_id: uint64
    status: ReplyStatus


@appendable
@dataclass
class DeviceInfo(IdlStruct, typename="DeviceInfo"):
    device_id: str
    key("device_id")
    role: DeviceRole
    product: ProductInfo
    topics: TopicInfo
    control_hardware: Optional[bool] = None  # placeholder shape; always None in the fixture
    power_hardware: Optional[bool] = None  # placeholder shape; always None in the fixture
    control_parameters: Optional[sequence(bool)] = None  # placeholder shape; always None
    metric_parameters: Optional[sequence(bool)] = None  # placeholder shape; always None
    control_service: Optional[bool] = None  # placeholder shape; always None in the fixture
    power_device: Optional[bool] = None  # placeholder shape; always None in the fixture


@appendable
@dataclass
class EnergyStartStopState(IdlStruct, typename="EnergyStartStopState"):
    # TRUE IDL order: ... present_level_reason, present_level_actor(@optional),
    # future_level_reason — the optional is NOT last; `future_level_reason` gets a placeholder
    # default (see module doc).
    device_id: str
    key("device_id")
    present_level: EnergyStartStopLevel
    future_level: EnergyStartStopLevel
    request_lock: bool
    elapsed_time: float32
    remaining_time: float32
    present_level_reason: PowerSwitchReason
    present_level_actor: Optional[str] = None
    future_level_reason: PowerSwitchTransitionCode = PowerSwitchTransitionCode.UNKNOWN


@appendable
@dataclass
class DcMeasurementUpdate(IdlStruct, typename="DcMeasurementUpdate"):
    device_id: str
    key("device_id")
    time_measured: ClockMonotonic
    external_measurement: sequence(DcPowerPortMeasurement)
    internal_measurement: sequence(DcPowerPortMeasurement) = field(default_factory=list)


@appendable
@dataclass
class PowerHardwareUpdate(IdlStruct, typename="PowerHardwareUpdate"):
    device_id: str
    key("device_id")
    timestamp: ClockMonotonic
    engine: Optional[bool] = None  # placeholder shape; always None in the fixtures
    fuel: Optional[bool] = None  # placeholder shape; always None in the fixtures
    generator: Optional[bool] = None  # placeholder shape; always None in the fixtures
    energy_storage: Optional[EnergyStorageState] = None
    power_electronics: Optional[PowerElectronicsState] = None
    thermal: Optional[bool] = None  # placeholder shape; always None in the fixtures


@appendable
@dataclass
class ActiveDiagnosticState(IdlStruct, typename="ActiveDiagnosticState"):
    device_id: str
    key("device_id")
    codes: sequence(DiagnosticTroubleCode)
    overflow: uint32


@appendable
@dataclass
class StorageUpdate(IdlStruct, typename="StorageUpdate"):
    # TRUE IDL order: device_id, internal_voltage, state_of_charge(@optional), available_energy,
    # hold_time, charge_time, discharge_time, max_charge_time, max_discharge_time,
    # max_charge_rate(@optional), max_discharge_rate(@optional). `state_of_charge` is NOT last —
    # every mandatory field after it needs a placeholder default (see module doc).
    device_id: str
    key("device_id")
    internal_voltage: float32
    state_of_charge: Optional[float32] = None
    available_energy: float32 = 0.0
    hold_time: float32 = 0.0
    charge_time: Curve2D = None  # type: ignore[assignment]
    discharge_time: Curve2D = None  # type: ignore[assignment]
    max_charge_time: Curve2D = None  # type: ignore[assignment]
    max_discharge_time: Curve2D = None  # type: ignore[assignment]
    max_charge_rate: Optional[float32] = None
    max_discharge_rate: Optional[float32] = None


@appendable
@dataclass
class AcMeasurementUpdate(IdlStruct, typename="AcMeasurementUpdate"):
    device_id: str
    key("device_id")
    time_measured: ClockMonotonic
    external_measurement: sequence(AcPowerPortMeasurement)
    internal_measurement: sequence(AcPowerPortMeasurement) = field(default_factory=list)


# ==================================================================================================
# The optional-arbitration minimal reproduction (matches wire_cdr_cross_oracle.rs's hand-derived
# `ActiveMicrogridControllerState` vectors byte-for-byte — see dump_reference.py `arbitration` mode).
# ==================================================================================================

@appendable
@dataclass
class ActiveMicrogridControllerState(IdlStruct, typename="ActiveMicrogridControllerState"):
    device_id: str
    key("device_id")
    master_id: Optional[str] = None
