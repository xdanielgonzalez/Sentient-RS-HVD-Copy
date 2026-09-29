/// In-memory publisher for tests: records every (topic, payload). Implements the shared
/// [`lib_mapping::sink::Publisher`] contract.
#[derive(Debug, Default)]
pub struct MockPublisher {
    pub sent: Vec<(String, Vec<u8>)>,
    pub healthy: bool,
}
