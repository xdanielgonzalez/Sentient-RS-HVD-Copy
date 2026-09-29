//! `WireFlat` impl for `ActiveDiagnosticState` (inverter sourced) — QPIWS faults + synthesized QMOD fault.

use crate::wire::{JsonSink, WireError, WireFlat};
use lib_parse::canonical::{CanonicalInverterFault, InverterMode};
use lib_tms::fields::emit_object;
use lib_tms::generated::{DtcSeverity, FailureModeIndicator};
use lib_tms::generated_fields::active_diagnostic_state_fields::ActiveDiagnosticStateSrc;
use lib_tms::generated_fields::diagnostic_trouble_code_fields::DiagnosticTroubleCodeSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Fault table type: (predicate, SPN, hint string).
type FaultTableEntry = (fn(&CanonicalInverterFault) -> bool, u32, &'static str);

/// Flat wire binding for `ActiveDiagnosticState` (inverter sourced) — QPIWS faults + synthesized
/// QMOD fault DTC.
pub struct ActiveDiagnosticStateInverterWire<'a> {
    pub fault: &'a CanonicalInverterFault,
    pub mode: InverterMode,
    pub device_id: &'a Identity,
    pub now: ClockMonotonic,
}

/// One DTC from the inverter fault table or the synthesized QMOD fault.
struct DiagnosticTroubleCodeInverterView<'a> {
    spn: u32,
    hint: &'a str,
    now: &'a ClockMonotonic,
}

impl<'a> DiagnosticTroubleCodeSrc for DiagnosticTroubleCodeInverterView<'a> {
    fn port_number(&self) -> Option<i64> {
        None
    }
    fn suspect_parameter(&self) -> i64 {
        self.spn as i64
    }
    fn failure_mode(&self) -> FailureModeIndicator {
        FailureModeIndicator::ConditionExists
    }
    fn occurrence_count(&self) -> i64 {
        1
    }
    type TimeRaised<'b>
        = &'b ClockMonotonic
    where
        Self: 'b;
    fn time_raised(&self) -> Self::TimeRaised<'_> {
        self.now
    }
    fn severity(&self) -> DtcSeverity {
        DtcSeverity::Major
    }
    type EstimateTime<'b>
        = lib_tms::fields::Absent
    where
        Self: 'b;
    fn estimate_time(&self) -> Option<Self::EstimateTime<'_>> {
        None
    }
    fn estimate_severity(&self) -> Option<DtcSeverity> {
        None
    }
    fn hint(&self) -> &str {
        self.hint
    }
}

/// Active diagnostic state wrapper (inverter sourced).
struct ActiveDiagnosticInverterView<'a> {
    fault: &'a CanonicalInverterFault,
    mode: InverterMode,
    device_id: &'a Identity,
    now: &'a ClockMonotonic,
}

impl<'a> ActiveDiagnosticStateSrc for ActiveDiagnosticInverterView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    type CodesItem<'b>
        = DiagnosticTroubleCodeInverterView<'b>
    where
        Self: 'b;
    fn codes(&self) -> impl Iterator<Item = Self::CodesItem<'_>> + '_ {
        let fault = self.fault;
        let mode = self.mode;
        let now = self.now;

        // Fault table: SPN = 100 + bit index (105, 109, 110, 112, 115, 117); disjoint from battery SPNs (1..16).
        const INVERTER_FAULT_TABLE: &[FaultTableEntry] = &[
            (|f| f.line_fail, 105, "line fail"),
            (|f| f.overtemperature_fault, 109, "over temperature"),
            (|f| f.fan_locked_fault, 110, "fan locked"),
            (|f| f.battery_low_fault, 112, "battery low"),
            (|f| f.overload_fault, 115, "overload"),
            (|f| f.output_short_fault, 117, "output short"),
        ];

        let mut emitted = 0usize;
        let iter1 = INVERTER_FAULT_TABLE
            .iter()
            .filter_map(move |&(is_set, spn, hint)| {
                if !is_set(fault) {
                    return None;
                }
                if emitted >= 64 {
                    return None;
                }
                emitted += 1;
                Some(DiagnosticTroubleCodeInverterView { spn, hint, now })
            });

        // Synthesized SPN 130 DTC when mode == Fault.
        let mode_dtc = if matches!(mode, InverterMode::Fault) {
            Some(DiagnosticTroubleCodeInverterView {
                spn: 130,
                hint: "inverter fault mode",
                now,
            })
        } else {
            None
        };

        iter1.chain(mode_dtc)
    }

    fn overflow(&self) -> i64 {
        const INVERTER_FAULT_TABLE: &[FaultTableEntry] = &[
            (|f| f.line_fail, 105, "line fail"),
            (|f| f.overtemperature_fault, 109, "over temperature"),
            (|f| f.fan_locked_fault, 110, "fan locked"),
            (|f| f.battery_low_fault, 112, "battery low"),
            (|f| f.overload_fault, 115, "overload"),
            (|f| f.output_short_fault, 117, "output short"),
        ];

        let mut overflow = 0u32;
        let mut emitted = 0usize;
        for &(is_set, _, _) in INVERTER_FAULT_TABLE {
            if is_set(self.fault) {
                if emitted >= 64 {
                    overflow += 1;
                } else {
                    emitted += 1;
                }
            }
        }
        overflow as i64
    }
}

impl WireFlat for ActiveDiagnosticStateInverterWire<'_> {
    fn serialize_flat(&self, sink: &mut impl JsonSink) -> Result<(), WireError> {
        let view = ActiveDiagnosticInverterView {
            fault: self.fault,
            mode: self.mode,
            device_id: self.device_id,
            now: &self.now,
        };
        emit_object::<_, lib_tms::generated_fields::active_diagnostic_state_fields::Chain, _>(
            &view, sink,
        );
        Ok(())
    }
}
