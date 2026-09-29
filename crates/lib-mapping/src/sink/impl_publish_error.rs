use super::prelude::*;

impl core::fmt::Display for PublishError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PublishError::Serialize => write!(f, "payload serialization buffer too small"),
            PublishError::TopicTooLong => write!(f, "topic buffer too small"),
            PublishError::Sink => write!(f, "publish sink failed"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PublishError {}
