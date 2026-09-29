use super::impl_core_control_parse::parse_control_line;
use super::impl_core_pacing::write_paced;
use super::prelude::*;

// ---------------------------------------------------------------------------------------------
// Networked peer mode: serve one TCP connection as the Seplos device, with a stdin control
// protocol read concurrently on a background thread.
// ---------------------------------------------------------------------------------------------

/// Serve one accepted/connected TCP stream as the Seplos peer: read request frames (delimited by
/// `\r`), reply via `SeplosSim::respond`, and apply stdin control lines as they arrive.
///
/// `initial_drift` seeds the opt-in "living dashboard" drift mode (`--drift` CLI flag / `SIM_DRIFT`
/// env); `None` (the default) preserves the pre-existing static-replay behavior exactly. A `drift
/// <arg>` stdin control line can enable/disable/reconfigure it mid-run.
pub(super) fn serve_connection(
    stream: TcpStream,
    pace: Option<PaceConfig>,
    initial_drift: Option<DriftSpec>,
) -> Result<(), AppError> {
    stream.set_nodelay(true).map_err(|e| AppError::Io {
        what: "set TCP_NODELAY".to_string(),
        source: e,
    })?;
    let mut writer = stream.try_clone().map_err(|e| AppError::Io {
        what: "clone stream for writer".to_string(),
        source: e,
    })?;
    let mut reader = stream;

    let (tx, rx) = std::sync::mpsc::channel::<Control>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("seplos-tcp-sim: stdin read error: {e}");
                    break;
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match parse_control_line(trimmed) {
                Ok(ctrl) => {
                    if tx.send(ctrl).is_err() {
                        break;
                    }
                }
                Err(msg) => eprintln!("seplos-tcp-sim: {msg}"),
            }
        }
    });

    let mut base_state = BatteryState::sample_16_cell();
    let mut sim = SeplosSim::new(base_state.clone());
    let clock = Instant::now();
    let mut drift = initial_drift;
    let mut seq: u64 = 0;
    let mut frame_buf: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        // Drain any pending control messages before reading the next request, so a Robot keyword
        // that changes behavior mid-run is applied promptly (never blocks on network I/O).
        while let Ok(ctrl) = rx.try_recv() {
            apply_control(&mut sim, &mut base_state, &mut drift, ctrl);
        }

        let n = match reader.read(&mut byte) {
            Ok(0) => {
                eprintln!("seplos-tcp-sim: peer closed connection");
                return Ok(());
            }
            Ok(n) => n,
            Err(e) => {
                return Err(AppError::Io {
                    what: "read from TCP peer".to_string(),
                    source: e,
                });
            }
        };
        if n == 0 {
            continue;
        }
        frame_buf.push(byte[0]);
        if byte[0] != EOI {
            continue;
        }

        // Apply any control messages that arrived while accumulating this frame too.
        while let Ok(ctrl) = rx.try_recv() {
            apply_control(&mut sim, &mut base_state, &mut drift, ctrl);
        }

        // T3 DIAGNOSTIC (temporary): hex-dump every request frame received, so we can confirm
        // the firmware's bytes actually reach the sim (vs. never arriving at all).
        println!(
            "recv req ({} bytes): {}",
            frame_buf.len(),
            hex_dump(&frame_buf)
        );

        // Render the "live" state for this exchange from `base_state` immediately before
        // replying, so every reply (not just the ones sampled by a poller) sees a fresh point on
        // the drift curve — `SeplosSim::respond` itself stays oblivious to drift, it just encodes
        // whatever `state()` currently holds.
        if let Some(spec) = &drift {
            *sim.state_mut() = spec.apply(&base_state, clock.elapsed());
        }

        let reply = sim.respond(&frame_buf);
        let request_cmd = request_command_byte(&frame_buf);
        frame_buf.clear();

        match reply {
            SimReply::Silence => {
                // Silent bus: nothing to write, nothing to log.
            }
            SimReply::Bytes(bytes) => {
                // T3 DIAGNOSTIC (temporary): hex-dump every reply frame sent, so we can confirm
                // what actually went out on the wire (vs. what the firmware claims it received).
                println!(
                    "send rsp ({} bytes): {}",
                    bytes.len(),
                    hex_dump(bytes.as_slice())
                );
                write_paced(&mut writer, bytes.as_slice(), pace).map_err(|e| AppError::Io {
                    what: "write reply to TCP peer".to_string(),
                    source: e,
                })?;
                seq += 1;
                let t_ns = clock.elapsed().as_nanos();
                match request_cmd {
                    Some(cmd) => {
                        println!("responded cmd=0x{cmd:02X} seq={seq} t={t_ns}");
                    }
                    None => {
                        // Should not happen for a well-formed request that produced a reply, but
                        // never fabricate a command byte — log without one rather than guess.
                        println!("responded cmd=0x?? seq={seq} t={t_ns}");
                    }
                }
                let _ = io::stdout().flush();
            }
        }
    }
}

/// T3 DIAGNOSTIC (temporary): format bytes as a compact hex string for wire-level logging.
fn hex_dump(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Best-effort extraction of the CID2 command byte (0x42/0x44/0xA4) from a raw request frame, for
/// the stdout perf stamp. Returns `None` if the frame is too short to contain it — never panics.
fn request_command_byte(frame: &[u8]) -> Option<u8> {
    // `~ VER(2) ADR(2) CID1(2) CID2(2) ...` — CID2 hex digits are at byte offset 7..=8.
    let hex = frame.get(7..9)?;
    let hi = hex_nibble(*hex.first()?)?;
    let lo = hex_nibble(*hex.get(1)?)?;
    Some((hi << 4) | lo)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

fn apply_control(
    sim: &mut SeplosSim,
    base_state: &mut BatteryState,
    drift: &mut Option<DriftSpec>,
    ctrl: Control,
) {
    match ctrl {
        Control::Behavior(b) => sim.set_behavior(b),
        Control::Faults(f) => {
            base_state.faults = f;
            sim.state_mut().faults = f;
        }
        Control::State(s) => {
            // `clone_from` (not `*base_state = s.clone()`) per clippy::assigning_clones.
            base_state.clone_from(&s);
            *sim.state_mut() = s;
        }
        Control::Drift(d) => *drift = d,
    }
}
