use super::prelude::*;

impl Cli {
    pub(crate) fn parse(args: &[String]) -> Result<Self, AppError> {
        let mut connect: Option<String> = None;
        let mut listen: Option<u16> = None;
        let mut dump: Option<PathBuf> = None;
        let mut pace_baud: Option<u32> = None;
        let mut turnaround_ms: Option<u64> = None;
        let mut drift: Option<DriftSpec> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--connect" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--connect requires a HOST:PORT argument".to_string(),
                    ))?;
                    connect = Some(v.clone());
                    i += 2;
                }
                "--listen" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--listen requires a PORT argument".to_string(),
                    ))?;
                    let port: u16 = v
                        .parse()
                        .map_err(|_| AppError::Usage(format!("--listen: invalid port {v:?}")))?;
                    listen = Some(port);
                    i += 2;
                }
                "--dump-fixtures" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--dump-fixtures requires a DIR argument".to_string(),
                    ))?;
                    dump = Some(PathBuf::from(v));
                    i += 2;
                }
                "--pace" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--pace requires a BAUD argument (e.g. --pace 19200; 0 disables pacing)"
                            .to_string(),
                    ))?;
                    let baud: u32 = v
                        .parse()
                        .map_err(|_| AppError::Usage(format!("--pace: invalid baud {v:?}")))?;
                    pace_baud = Some(baud);
                    i += 2;
                }
                "--turnaround-ms" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--turnaround-ms requires a milliseconds argument".to_string(),
                    ))?;
                    let ms: u64 = v.parse().map_err(|_| {
                        AppError::Usage(format!("--turnaround-ms: invalid value {v:?}"))
                    })?;
                    turnaround_ms = Some(ms);
                    i += 2;
                }
                "--drift" => {
                    let v = args.get(i + 1).ok_or(AppError::Usage(
                        "--drift requires an argument (\"off\", \"on\", or key=value pairs)"
                            .to_string(),
                    ))?;
                    drift = parse_drift_spec(v)
                        .map_err(|e| AppError::Usage(format!("--drift: {e}")))?;
                    i += 2;
                }
                other => {
                    return Err(AppError::Usage(format!("unrecognized argument: {other}")));
                }
            }
        }

        let modes_given = [connect.is_some(), listen.is_some(), dump.is_some()]
            .iter()
            .filter(|b| **b)
            .count();
        if modes_given != 1 {
            return Err(AppError::Usage(
                "exactly one of --connect HOST:PORT, --listen PORT, --dump-fixtures DIR is required"
                    .to_string(),
            ));
        }
        if turnaround_ms.is_some() && pace_baud.unwrap_or(0) == 0 {
            return Err(AppError::Usage(
                "--turnaround-ms requires --pace BAUD (with BAUD > 0)".to_string(),
            ));
        }

        let mode = if let Some(dir) = dump {
            Mode::DumpFixtures(dir)
        } else if let Some(addr) = connect {
            Mode::Connect(addr)
        } else if let Some(port) = listen {
            Mode::Listen(port)
        } else {
            unreachable!("modes_given == 1 guarantees exactly one branch above matched")
        };

        // `--pace 0` (or omitted) means today's instant behavior: `pace` stays `None`.
        let pace = pace_baud.filter(|&b| b > 0).map(|baud| PaceConfig {
            baud,
            turnaround_ms: turnaround_ms.unwrap_or(PaceConfig::DEFAULT_TURNAROUND_MS),
        });

        Ok(Cli { mode, pace, drift })
    }
}
