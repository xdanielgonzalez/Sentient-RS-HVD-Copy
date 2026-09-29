//! XCDR2 CDR binding for `ActiveDiagnosticState` (inverter sourced) — QPIWS faults + synthesized
//! QMOD fault DTC. CDR twin of `wire::inverter::impl_active_diagnostic`.

use lib_parse::canonical::{CanonicalInverterFault, InverterMode};
use lib_tms::generated::{DtcSeverity, FailureModeIndicator};
use lib_tms::generated_cdr::active_diagnostic_state_cdr::ActiveDiagnosticStateCdrSrc;
use lib_tms::generated_cdr::diagnostic_trouble_code_cdr::DiagnosticTroubleCodeCdrSrc;
use lib_tms::{ClockMonotonic, Identity};

/// Fault table entry: (predicate, SPN, hint string).
type FaultTableEntry = (fn(&CanonicalInverterFault) -> bool, u32, &'static str);

/// SPN = 100 + bit index (105, 109, 110, 112, 115, 117); disjoint from battery SPNs (1..16).
const INVERTER_FAULT_TABLE: &[FaultTableEntry] = &[
    (|f| f.line_fail, 105, "line fail"),
    (|f| f.overtemperature_fault, 109, "over temperature"),
    (|f| f.fan_locked_fault, 110, "fan locked"),
    (|f| f.battery_low_fault, 112, "battery low"),
    (|f| f.overload_fault, 115, "overload"),
    (|f| f.output_short_fault, 117, "output short"),
];

/// One DTC from the inverter fault table or the synthesized QMOD fault.
pub struct DiagnosticTroubleCodeInverterCdrView<'a> {
    spn: u32,
    hint: &'a str,
    now: &'a ClockMonotonic,
}

impl<'a> DiagnosticTroubleCodeCdrSrc for DiagnosticTroubleCodeInverterCdrView<'a> {
    fn port_number(&self) -> Option<i32> {
        None
    }
    fn suspect_parameter(&self) -> u32 {
        self.spn
    }
    fn failure_mode(&self) -> FailureModeIndicator {
        FailureModeIndicator::ConditionExists
    }
    fn occurrence_count(&self) -> u32 {
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
        = &'b ClockMonotonic
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

/// `ActiveDiagnosticState` inverter CDR view — QPIWS faults + synthesized QMOD fault DTC.
pub struct ActiveDiagnosticStateInverterCdrView<'a> {
    pub fault: &'a CanonicalInverterFault,
    pub mode: InverterMode,
    pub device_id: &'a Identity,
    pub now: &'a ClockMonotonic,
}

impl<'a> ActiveDiagnosticStateCdrSrc for ActiveDiagnosticStateInverterCdrView<'a> {
    fn device_id(&self) -> &str {
        self.device_id.as_str()
    }
    type CodesItem<'b>
        = DiagnosticTroubleCodeInverterCdrView<'b>
    where
        Self: 'b;
    fn codes(&self) -> impl Iterator<Item = Self::CodesItem<'_>> + '_ {
        let fault = self.fault;
        let mode = self.mode;
        let now = self.now;
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
                Some(DiagnosticTroubleCodeInverterCdrView { spn, hint, now })
            });
        // Synthesized SPN 130 DTC when mode == Fault.
        let mode_dtc = if matches!(mode, InverterMode::Fault) {
            Some(DiagnosticTroubleCodeInverterCdrView {
                spn: 130,
                hint: "inverter fault mode",
                now,
            })
        } else {
            None
        };
        iter1.chain(mode_dtc)
    }
    fn overflow(&self) -> u32 {
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
        overflow
    }
}
