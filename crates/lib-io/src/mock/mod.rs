//! Scripted in-memory `SerialTransport` for deterministic fault-injection tests.
//!
//! `#[cfg(any(test, feature = "mock"))]`. no_std + heapless; drives the real
//! `driver::read_telemetry`/`read_fault` without a wire. Each `read_until` consumes one
//! `MockStep`; every written byte and direction transition is recorded for byte-exact assertions.

mod impl_mock_serial;
mod prelude;
mod types;

pub use types::*;

#[cfg(test)]
use prelude::*;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[tokio::test]
    async fn test_reply_delivers_bytes() {
        let steps = [MockStep::Reply(b"~2000\r")];
        let mut mock = MockSerial::<16>::new(&steps);
        let mut buf = [0u8; 64];

        let n = mock.read_until(b'\r', &mut buf).await.expect("should read");
        assert_eq!(n, 6);
        assert_eq!(&buf[..n], b"~2000\r");
    }

    #[tokio::test]
    async fn test_empty_script_times_out() {
        let steps = [];
        let mut mock = MockSerial::<16>::new(&steps);
        let mut buf = [0u8; 64];

        let result = mock.read_until(b'\r', &mut buf).await;
        assert_eq!(result, Err(IoError::Timeout));
    }

    #[tokio::test]
    async fn test_timeout_step() {
        let steps = [MockStep::Timeout];
        let mut mock = MockSerial::<16>::new(&steps);
        let mut buf = [0u8; 64];

        let result = mock.read_until(b'\r', &mut buf).await;
        assert_eq!(result, Err(IoError::Timeout));
    }

    #[tokio::test]
    async fn test_device_error_step() {
        let steps = [MockStep::DeviceError];
        let mut mock = MockSerial::<16>::new(&steps);
        let mut buf = [0u8; 64];

        let result = mock.read_until(b'\r', &mut buf).await;
        assert_eq!(result, Err(IoError::Device));
    }

    #[tokio::test]
    async fn test_direction_tracking() {
        let steps = [];
        let mut mock = MockSerial::<16>::new(&steps);

        mock.set_direction(Direction::Transmit)
            .await
            .expect("should set transmit");
        mock.set_direction(Direction::Receive)
            .await
            .expect("should set receive");

        assert_eq!(mock.directions.len(), 2);
        assert_eq!(mock.directions[0], Direction::Transmit);
        assert_eq!(mock.directions[1], Direction::Receive);
    }

    #[tokio::test]
    async fn test_write_all_records_bytes() {
        let steps = [];
        let mut mock = MockSerial::<16>::new(&steps);

        mock.write_all(b"~2000").await.expect("should write");

        assert_eq!(mock.writes.len(), 5);
        assert_eq!(&mock.writes[..], b"~2000");
    }
}
