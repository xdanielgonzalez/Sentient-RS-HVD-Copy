use super::sim_command::SimCommand;

/// A parsed, checksum-verified request frame (the fields [`SeplosSim::respond`] needs).
pub(crate) struct ParsedRequest {
    pub(crate) adr: u8,
    pub(crate) command: SimCommand,
    /// The command-group/pack-selector byte carried in the request's INFO field (0 when the
    /// request shape has no room for one — see the parsing module's `parse_request`).
    pub(crate) command_group: u8,
}
