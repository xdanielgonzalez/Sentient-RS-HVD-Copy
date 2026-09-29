use super::prelude::*;

impl MockPublisher {
    pub fn new() -> Self {
        MockPublisher {
            sent: Vec::new(),
            healthy: true,
        }
    }
}

impl Publisher for MockPublisher {
    fn publish(&mut self, topic: &str, payload: &[u8]) -> Result<(), PublishError> {
        self.sent.push((topic.to_string(), payload.to_vec()));
        Ok(())
    }
    fn healthy(&self) -> bool {
        self.healthy
    }
}
