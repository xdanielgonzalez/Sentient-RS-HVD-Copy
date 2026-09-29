use super::prelude::*;

impl Default for BrokerConfig {
    fn default() -> Self {
        BrokerConfig {
            id: "sentient".into(),
            host: "127.0.0.1".into(),
            port: 1883,
            auth: None,
        }
    }
}
