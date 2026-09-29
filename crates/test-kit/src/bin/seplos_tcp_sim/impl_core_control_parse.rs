use super::prelude::*;

/// Parse one stdin control line into a [`Control`]. Returns `Err(message)` (never panics) on any
/// unrecognized command or argument — the caller logs it to stderr and continues.
pub(super) fn parse_control_line(line: &str) -> Result<Control, String> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();

    match cmd {
        "behavior" => parse_behavior(rest).map(Control::Behavior),
        "faults" => parse_faults(rest).map(Control::Faults),
        "state" => parse_state(rest).map(Control::State),
        "drift" => parse_drift(rest).map(Control::Drift),
        other => Err(format!("unknown control command: {other:?}")),
    }
}

fn parse_behavior(arg: &str) -> Result<Behavior, String> {
    if let Some(n_str) = arg.strip_prefix("truncated:") {
        let n: usize = n_str
            .parse()
            .map_err(|_| format!("behavior truncated: invalid length {n_str:?}"))?;
        return Ok(Behavior::Truncated(n));
    }
    if let Some(code_str) = arg.strip_prefix("rtn-error:") {
        let code: u8 =
            if let Some(hex) = code_str.strip_prefix("0x").or(code_str.strip_prefix("0X")) {
                u8::from_str_radix(hex, 16)
                    .map_err(|_| format!("behavior rtn-error: invalid hex code {code_str:?}"))?
            } else {
                code_str
                    .parse()
                    .map_err(|_| format!("behavior rtn-error: invalid code {code_str:?}"))?
            };
        return Ok(Behavior::RtnError(code));
    }
    match arg {
        "normal" => Ok(Behavior::Normal),
        "corrupt-checksum" => Ok(Behavior::CorruptChecksum),
        "wrong-address" => Ok(Behavior::WrongAddress),
        "silence" => Ok(Behavior::Silence),
        "garbage" => Ok(Behavior::Garbage),
        other => Err(format!("unknown behavior: {other:?}")),
    }
}

fn parse_faults(arg: &str) -> Result<FaultFlags, String> {
    if arg == "none" {
        return Ok(FaultFlags::default());
    }
    let mut flags = FaultFlags::default();
    for name in arg.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        set_fault_flag(&mut flags, name)?;
    }
    Ok(flags)
}

/// Set one named field of [`FaultFlags`] by its exact `lib-parse` `FaultFlags` name (also the
/// field names of `test_kit::seplos::FaultFlags`, which mirrors it 1:1). Rejects unknown names
/// with an error string rather than panicking.
fn set_fault_flag(flags: &mut FaultFlags, name: &str) -> Result<(), String> {
    match name {
        "high_voltage_protection" => flags.high_voltage_protection = true,
        "low_voltage_protection" => flags.low_voltage_protection = true,
        "high_temperature_charge_protection" => flags.high_temperature_charge_protection = true,
        "low_temperature_charge_protection" => flags.low_temperature_charge_protection = true,
        "charge_overcurrent_protection" => flags.charge_overcurrent_protection = true,
        "discharge_overcurrent_protection" => flags.discharge_overcurrent_protection = true,
        "short_circuit_protection" => flags.short_circuit_protection = true,
        "mosfet_fault" => flags.mosfet_fault = true,
        "cell_balance_warning" => flags.cell_balance_warning = true,
        "low_capacity_warning" => flags.low_capacity_warning = true,
        "high_temperature_discharge_warning" => flags.high_temperature_discharge_warning = true,
        "low_temperature_discharge_warning" => flags.low_temperature_discharge_warning = true,
        "low_voltage_warning" => flags.low_voltage_warning = true,
        "high_voltage_warning" => flags.high_voltage_warning = true,
        "discharge_current_warning" => flags.discharge_current_warning = true,
        "charge_current_warning" => flags.charge_current_warning = true,
        other => return Err(format!("unknown fault flag: {other:?}")),
    }
    Ok(())
}

/// Parse the `state <arg>` control line. `"sample14"` and `"fault-vector"` are the two
/// pre-existing literal spellings the Robot/python suites and their default args send verbatim —
/// KEPT WORKING UNCHANGED. Any other argument is treated as the runtime spec grammar (see
/// [`parse_battery_state_spec`]), accepted either as `state set <spec>` (an explicit prefix) or
/// bare `state <spec>` (unambiguous: every existing literal state name is a single bare word with
/// no `=`, while every spec token is `key=value`) — both route through the one shared library
/// parser, so the stdin verb, a coming cucumber harness, and Robot keywords can never drift apart.
fn parse_state(arg: &str) -> Result<BatteryState, String> {
    match arg {
        "sample14" => Ok(BatteryState::sample_14_cell()),
        "fault-vector" => {
            let mut state = BatteryState::sample_14_cell();
            state.faults = FaultFlags {
                high_voltage_protection: true,
                short_circuit_protection: true,
                low_capacity_warning: true,
                charge_current_warning: true,
                ..Default::default()
            };
            Ok(state)
        }
        other => {
            let spec_text = other
                .strip_prefix("set ")
                .map(str::trim_start)
                .unwrap_or(other);
            parse_battery_state_spec(spec_text).map_err(|e| format!("state {arg:?}: {e}"))
        }
    }
}

/// Parse the `drift <arg>` control line: `"off"` disables drift, `"on"` enables it at
/// [`DriftSpec::default`], and anything else is the `key=value` overlay grammar (see
/// [`parse_drift_spec`]'s doc comment) — one shared parser for the stdin verb and the `--drift`
/// CLI flag alike.
fn parse_drift(arg: &str) -> Result<Option<DriftSpec>, String> {
    parse_drift_spec(arg).map_err(|e| format!("drift {arg:?}: {e}"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// Exact control lines observed in `bin/zephyr/qemu/e2e_*.robot` /
    /// `bin/zephyr/qemu/lib/SeplosSimKeywords.py` (grepped, not re-derived) — every one of these
    /// spellings must keep parsing successfully after the spec-grammar refactor. `drift ...` was
    /// added alongside the other known verbs when drift mode landed (#226) — no existing Robot
    /// suite sends it yet, but it must parse like every other verb here.
    #[test]
    fn observed_robot_and_python_control_lines_still_parse() {
        let observed = [
            "state sample14",
            "behavior silence",
            "behavior corrupt-checksum",
            "behavior wrong-address",
            "behavior truncated:20",
            "behavior garbage",
            "behavior normal",
            "faults high_voltage_protection,short_circuit_protection,low_capacity_warning,charge_current_warning",
            "drift on",
            "drift off",
            "drift period=60 current=100",
        ];
        for line in observed {
            parse_control_line(line)
                .unwrap_or_else(|e| panic!("observed control line {line:?} must still parse: {e}"));
        }
    }

    #[test]
    fn state_sample14_yields_the_unmodified_baseline() {
        let Control::State(state) = parse_control_line("state sample14").expect("parses") else {
            panic!("expected Control::State");
        };
        assert_eq!(state, BatteryState::sample_14_cell());
    }

    #[test]
    fn state_fault_vector_still_parses_and_sets_the_four_expected_flags() {
        let Control::State(state) = parse_control_line("state fault-vector").expect("parses")
        else {
            panic!("expected Control::State");
        };
        assert!(state.faults.high_voltage_protection);
        assert!(state.faults.short_circuit_protection);
        assert!(state.faults.low_capacity_warning);
        assert!(state.faults.charge_current_warning);
    }

    #[test]
    fn state_bare_spec_grammar_is_accepted() {
        let Control::State(state) = parse_control_line("state cells=3800,3805,3810 current=-164")
            .expect("bare key=value spec must parse")
        else {
            panic!("expected Control::State");
        };
        assert_eq!(state.cell_voltages_mv.as_slice(), &[3800, 3805, 3810]);
        assert_eq!(state.current_ca, -164);
    }

    #[test]
    fn state_set_prefixed_spec_grammar_is_accepted() {
        let Control::State(state) = parse_control_line("state set current=42").expect("parses")
        else {
            panic!("expected Control::State");
        };
        assert_eq!(state.current_ca, 42);
    }

    #[test]
    fn state_unknown_bare_word_is_a_fail_closed_error_not_a_panic() {
        assert!(parse_control_line("state charging").is_err());
    }

    #[test]
    fn drift_off_yields_none() {
        let Control::Drift(drift) = parse_control_line("drift off").expect("parses") else {
            panic!("expected Control::Drift");
        };
        assert_eq!(drift, None);
    }

    #[test]
    fn drift_on_yields_the_default_spec() {
        let Control::Drift(drift) = parse_control_line("drift on").expect("parses") else {
            panic!("expected Control::Drift");
        };
        assert_eq!(drift, Some(DriftSpec::default()));
    }

    #[test]
    fn drift_key_value_spec_is_accepted() {
        let Control::Drift(drift) = parse_control_line("drift period=60 noise=0").expect("parses")
        else {
            panic!("expected Control::Drift");
        };
        let spec = drift.expect("Some");
        assert_eq!(spec.period_s, 60);
        assert_eq!(spec.noise_mv, 0);
    }

    #[test]
    fn drift_unknown_key_is_a_fail_closed_error_not_a_panic() {
        assert!(parse_control_line("drift bogus=1").is_err());
    }
}
