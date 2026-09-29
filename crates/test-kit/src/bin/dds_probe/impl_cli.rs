use super::prelude::*;

const DEFAULT_IFACE: &str = "eth0";
const DEFAULT_TIMEOUT_SECS: u64 = 20;
const DEFAULT_COUNT: u32 = 5;
const DEFAULT_INTERVAL_MS: u64 = 500;

impl Cli {
    pub(crate) fn parse(args: &[String]) -> Result<Self, AppError> {
        let usage = || {
            AppError::Usage(
                "one of: subscribe --topic Heartbeat [--iface eth0] [--timeout-secs 20] | \
                 publish --device-id ID [--iface eth0] [--count 5] [--interval-ms 500] | \
                 dump-fixtures"
                    .to_string(),
            )
        };

        let (head, rest) = args.split_first().ok_or_else(usage)?;
        let mode = match head.as_str() {
            "dump-fixtures" => Mode::DumpFixtures,
            "subscribe" => {
                let topic = arg_value(rest, "--topic")
                    .ok_or_else(|| AppError::Usage("subscribe requires --topic".to_string()))?;
                if topic != "Heartbeat" {
                    // Only Heartbeat is wired end-to-end through the real `DustParticipant` today
                    // (WP4 landed Heartbeat only; the remaining topics arrive with the Pi taps in
                    // WP6) — fail loud rather than silently no-op subscribing to nothing.
                    return Err(AppError::Usage(format!(
                        "subscribe --topic {topic}: only Heartbeat is wired in this build (WP6 \
                         adds the rest) — use `dump-fixtures` to render other topics offline"
                    )));
                }
                Mode::Subscribe {
                    iface: arg_value(rest, "--iface")
                        .unwrap_or(DEFAULT_IFACE)
                        .to_string(),
                    timeout_secs: arg_value(rest, "--timeout-secs")
                        .map(|v| v.parse().map_err(|_| usage()))
                        .transpose()?
                        .unwrap_or(DEFAULT_TIMEOUT_SECS),
                }
            }
            "publish" => {
                let device_id = arg_value(rest, "--device-id")
                    .ok_or_else(|| AppError::Usage("publish requires --device-id".to_string()))?;
                Mode::Publish {
                    iface: arg_value(rest, "--iface")
                        .unwrap_or(DEFAULT_IFACE)
                        .to_string(),
                    device_id: device_id.to_string(),
                    count: arg_value(rest, "--count")
                        .map(|v| v.parse().map_err(|_| usage()))
                        .transpose()?
                        .unwrap_or(DEFAULT_COUNT),
                    interval_ms: arg_value(rest, "--interval-ms")
                        .map(|v| v.parse().map_err(|_| usage()))
                        .transpose()?
                        .unwrap_or(DEFAULT_INTERVAL_MS),
                }
            }
            _ => return Err(usage()),
        };
        Ok(Cli { mode })
    }
}

/// Find `--flag VALUE` in `args`, returning `VALUE`. Simple linear scan — the arg list is always
/// tiny (this bin has at most 4 flags per mode).
fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
