use super::prelude::*;

impl Default for PublishConfig {
    fn default() -> Self {
        PublishConfig {
            prefix: "tms".into(),
        }
    }
}
