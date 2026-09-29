use super::prelude::*;

impl From<MqttClientError> for SessionError {
    fn from(e: MqttClientError) -> Self {
        SessionError::Encode(e)
    }
}
