use test_kit::seplos::DriftSpec;

use super::mode::Mode;
use super::pace_config::PaceConfig;

pub(crate) struct Cli {
    pub(crate) mode: Mode,
    /// `None` (no `--pace`, or `--pace 0`) means today's instant reply behavior.
    pub(crate) pace: Option<PaceConfig>,
    /// `None` (no `--drift`, or `--drift off`) means the pre-existing static-replay behavior —
    /// drift is opt-in.
    pub(crate) drift: Option<DriftSpec>,
}
