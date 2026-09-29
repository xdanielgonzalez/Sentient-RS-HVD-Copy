//! Generic poll driver — runs the Seplos (BMS) and Voltronic (inverter) exchanges over any
//! [`SerialTransport`].
//!
//! Fully generic (`<R: SerialTransport, V: BmsVendorProtocol>` / `<R: SerialTransport, V:
//! InverterVendorProtocol>`) — static dispatch, no `Box<dyn>`, no allocation. The same driver
//! monomorphizes onto a host `tokio-serial` port or an embassy MCU UART. Half-duplex RS-485
//! direction is sequenced around each transmit (a no-op on full-duplex RS-232).

mod prelude;
mod types;
pub use types::*;

mod impl_exchange_error;

use prelude::*;

/// Perform one request/response telemetry exchange with the device at `address`.
///
/// Builds the request via `vendor.build_request`, sets direction `Transmit`, writes the request,
/// sets direction `Receive`, reads until the response delimiter `\r`, and parses the response
/// into a canonical reading. All errors are propagated via `?` into `ExchangeError`.
pub async fn read_telemetry<R, V>(
    transport: &mut R,
    vendor: &V,
    address: u8,
    buf: &mut [u8],
) -> Result<CanonicalReading, ExchangeError>
where
    R: SerialTransport,
    V: BmsVendorProtocol,
{
    let result: Result<CanonicalReading, ExchangeError> = async {
        // Build the request into a heapless::Vec
        let mut req: Vec<u8, 32> = Vec::new();
        vendor.build_request(address, BmsCommand::Telemetry, &mut req)?;

        // Set direction to Transmit and write the request
        transport.set_direction(Direction::Transmit).await?;
        transport.write_all(&req).await?;

        // Switch to Receive direction and read the response until delimiter
        transport.set_direction(Direction::Receive).await?;
        let n = transport.read_until(b'\r', buf).await?;

        // Parse the response and return the reading
        let reading = vendor.parse_reading(&buf[..n], address)?;
        Ok(reading)
    }
    .await;

    if result.is_err() {
        // A failed/timed-out exchange can leave a stale partial frame buffered underneath;
        // discard it before the caller's next request, so it can't desync that read_until.
        // A clear failure is deliberately swallowed — it must never mask the original error.
        let _ = transport.discard_input().await;
    }

    result
}

/// Perform one request/response **fault** exchange with the device at `address`.
///
/// Same half-duplex sequence as [`read_telemetry`], but issues the fault command and parses the
/// `0x44` response into canonical fault flags.
pub async fn read_fault<R, V>(
    transport: &mut R,
    vendor: &V,
    address: u8,
    buf: &mut [u8],
) -> Result<lib_parse::canonical::CanonicalFault, ExchangeError>
where
    R: SerialTransport,
    V: BmsVendorProtocol,
{
    let result: Result<lib_parse::canonical::CanonicalFault, ExchangeError> = async {
        let mut req: Vec<u8, 32> = Vec::new();
        vendor.build_request(address, BmsCommand::Fault, &mut req)?;

        transport.set_direction(Direction::Transmit).await?;
        transport.write_all(&req).await?;

        transport.set_direction(Direction::Receive).await?;
        let n = transport.read_until(b'\r', buf).await?;

        let fault = vendor.parse_fault(&buf[..n], address)?;
        Ok(fault)
    }
    .await;

    if result.is_err() {
        // See `read_telemetry`'s matching comment: discard on the error path only, never
        // masking the original exchange error with a clear failure.
        let _ = transport.discard_input().await;
    }

    result
}

/// Perform one request/response exchange for `command` against an [`InverterVendorProtocol`],
/// returning the number of valid response bytes written into `buf`.
///
/// Mitigates the CRC-byte-equals-`\r` framing hazard: a genuine Voltronic response CRC byte can
/// equal `0x0D`, which truncates the initial [`SerialTransport::read_until`] early (the frame's
/// real CRC + `\r` haven't arrived yet). If [`InverterVendorProtocol::validate_frame`] fails on
/// the first read, up to 2 continuation reads are appended (each may itself be cut short by the
/// SAME hazard, hence up to 2 attempts) and the frame is revalidated after each.
async fn exchange_inverter<R, V>(
    transport: &mut R,
    vendor: &V,
    command: InverterCommand,
    buf: &mut [u8],
) -> Result<usize, ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let result: Result<usize, ExchangeError> = async {
        let mut req: Vec<u8, 16> = Vec::new();
        vendor.build_request(command, &mut req)?;

        transport.set_direction(Direction::Transmit).await?;
        transport.write_all(&req).await?;

        transport.set_direction(Direction::Receive).await?;
        let mut n = transport.read_until(b'\r', buf).await?;

        let mut continuations = 0;
        while !vendor.validate_frame(&buf[..n]) && continuations < 2 {
            let more = transport.read_until(b'\r', &mut buf[n..]).await?;
            n += more;
            continuations += 1;
        }

        if !vendor.validate_frame(&buf[..n]) {
            return Err(ExchangeError::Parse(ParseError::BadChecksum));
        }

        Ok(n)
    }
    .await;

    if result.is_err() {
        // Covers both a genuine transport error mid-continuation-read AND the exhausted-
        // continuation-budget BadChecksum case: either way the device may still have more
        // bytes in flight that were never drained, which would desync the NEXT exchange's
        // read_until if left buffered. See `read_telemetry`'s matching comment for the
        // never-mask-the-original-error rule.
        let _ = transport.discard_input().await;
    }

    result
}

/// Read `QPIGS`-style status telemetry. `mode`/`ratings` are left at their "not yet known"
/// placeholders ([`InverterMode::Unknown`] / `None`) — Voltronic/PI30 needs several distinct
/// commands per cycle (status + mode, plus a periodic fault poll), so merging a separately
/// polled [`read_inverter_mode`] / cached [`read_inverter_ratings`] result into one composite
/// reading is the caller's concern, not this single-command exchange's.
pub async fn read_inverter_status<R, V>(
    transport: &mut R,
    vendor: &V,
    buf: &mut [u8],
) -> Result<CanonicalInverterReading, ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let n = exchange_inverter(transport, vendor, InverterCommand::Status, buf).await?;
    Ok(vendor.parse_status(&buf[..n])?)
}

/// Read the `QMOD`-style inverter operating mode.
pub async fn read_inverter_mode<R, V>(
    transport: &mut R,
    vendor: &V,
    buf: &mut [u8],
) -> Result<InverterMode, ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let n = exchange_inverter(transport, vendor, InverterCommand::Mode, buf).await?;
    Ok(vendor.parse_mode(&buf[..n])?)
}

/// Read `QPIWS`-style inverter protection/warning fault flags.
pub async fn read_inverter_fault<R, V>(
    transport: &mut R,
    vendor: &V,
    buf: &mut [u8],
) -> Result<CanonicalInverterFault, ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let n = exchange_inverter(transport, vendor, InverterCommand::Fault, buf).await?;
    Ok(vendor.parse_fault(&buf[..n])?)
}

/// Read `QPIRI`-style inverter static ratings.
pub async fn read_inverter_ratings<R, V>(
    transport: &mut R,
    vendor: &V,
    buf: &mut [u8],
) -> Result<InverterRatings, ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let n = exchange_inverter(transport, vendor, InverterCommand::Ratings, buf).await?;
    Ok(vendor.parse_ratings(&buf[..n])?)
}

/// Read the `QID`-style inverter serial number into `out`.
pub async fn read_inverter_serial<R, V>(
    transport: &mut R,
    vendor: &V,
    buf: &mut [u8],
    out: &mut String<32>,
) -> Result<(), ExchangeError>
where
    R: SerialTransport,
    V: InverterVendorProtocol,
{
    let n = exchange_inverter(transport, vendor, InverterCommand::SerialNumber, buf).await?;
    vendor.parse_serial(&buf[..n], out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::mock::{MockSerial, MockStep};

    /// A minimal test-only inverter protocol whose "checksum" is an XOR-fold of the body bytes
    /// (deliberately NOT the real Voltronic CRC) — trivial on purpose, so this test isolates
    /// `exchange_inverter`'s continuation-read RETRY MECHANISM from the Voltronic protocol's own
    /// framing/CRC correctness (proven separately, against Python-derived ground truth, in
    /// `lib-parse`'s `voltronic_crc_vectors.rs`/`voltronic_vectors.rs`).
    #[derive(Debug, Clone, Copy, Default)]
    struct FakeInverterVendor;

    impl InverterVendorProtocol for FakeInverterVendor {
        fn build_request(
            &self,
            _command: InverterCommand,
            out: &mut Vec<u8, 16>,
        ) -> Result<(), ParseError> {
            out.clear();
            out.extend_from_slice(b"CMD\r")
                .map_err(|_| ParseError::CapacityExceeded)
        }

        fn validate_frame(&self, bytes: &[u8]) -> bool {
            let n = bytes.len();
            if n < 3 || bytes[0] != b'(' || bytes[n - 1] != b'\r' {
                return false;
            }
            let body = &bytes[1..n - 2];
            let checksum = body.iter().fold(0u8, |acc, &b| acc ^ b);
            checksum == bytes[n - 2]
        }

        fn parse_status(&self, _bytes: &[u8]) -> Result<CanonicalInverterReading, ParseError> {
            Ok(CanonicalInverterReading {
                grid_voltage_dv: 1,
                grid_frequency_dhz: 2,
                ac_output_voltage_dv: 3,
                ac_output_frequency_dhz: 4,
                ac_output_apparent_power_va: 5,
                ac_output_active_power_w: 6,
                bus_voltage_v: 7,
                battery_voltage_cv: 8,
                battery_charging_current_a: 9,
                battery_discharge_current_a: 10,
                inverter_temperature_c: 11,
                pv_current_da: 12,
                pv_voltage_dv: 13,
                pv_charging_power_w: 14,
                status_bits: 0,
                mode: InverterMode::Unknown,
                ratings: None,
            })
        }

        fn parse_mode(&self, _bytes: &[u8]) -> Result<InverterMode, ParseError> {
            Ok(InverterMode::Unknown)
        }

        fn parse_fault(&self, _bytes: &[u8]) -> Result<CanonicalInverterFault, ParseError> {
            Ok(CanonicalInverterFault::default())
        }

        fn parse_ratings(&self, _bytes: &[u8]) -> Result<InverterRatings, ParseError> {
            Err(ParseError::Malformed)
        }

        fn parse_serial(&self, _bytes: &[u8], out: &mut String<32>) -> Result<(), ParseError> {
            out.clear();
            Ok(())
        }
    }

    /// Full frame `(AL\r\r`: `(` + body `"AL"` (XOR-fold checksum = `0x0D`) + the checksum byte
    /// (which HAPPENS to equal `0x0D`) + the real terminator `\r`. A naive
    /// `read_until(b'\r')` stops at the first `0x0D` — the checksum byte — one byte short of the
    /// real frame end. This is exactly the embedded-CRC-byte-equals-`\r` hazard the continuation
    /// reads exist for.
    #[tokio::test]
    async fn exchange_inverter_recovers_from_a_checksum_byte_equal_to_0x0d() {
        let full_frame: &[u8] = b"(AL\r\r";
        let first_read = &full_frame[..4]; // "(AL\r" -- read_until stops here (false terminator)
        let continuation = &full_frame[4..]; // the remaining real "\r"

        let steps = [MockStep::Reply(first_read), MockStep::Reply(continuation)];
        let mut mock = MockSerial::<4>::new(&steps);
        let mut buf = [0u8; 64];

        let n = exchange_inverter(
            &mut mock,
            &FakeInverterVendor,
            InverterCommand::Status,
            &mut buf,
        )
        .await
        .expect("recovers via a continuation read");

        assert_eq!(n, full_frame.len());
        assert_eq!(&buf[..n], full_frame);
    }

    #[tokio::test]
    async fn read_inverter_status_returns_parsed_reading_after_recovery() {
        let full_frame: &[u8] = b"(AL\r\r";
        let steps = [
            MockStep::Reply(&full_frame[..4]),
            MockStep::Reply(&full_frame[4..]),
        ];
        let mut mock = MockSerial::<4>::new(&steps);
        let mut buf = [0u8; 64];

        let reading = read_inverter_status(&mut mock, &FakeInverterVendor, &mut buf)
            .await
            .expect("recovers and parses");
        assert_eq!(reading.grid_voltage_dv, 1);
    }

    #[tokio::test]
    async fn exchange_inverter_gives_up_after_2_failed_continuation_reads() {
        // Every read (initial + both continuations) leaves the accumulated buffer failing
        // `validate_frame` (no trailing `\r` until the very end, and even then the checksum
        // never matches) — after the continuation budget is exhausted this must surface as a
        // parse error, never loop forever or panic.
        let steps = [
            MockStep::Reply(b"(XX\xFF\r"), // checksum('X'^'X'=0) != 0xFF -> invalid
            MockStep::Reply(b"more1"),
            MockStep::Reply(b"more2"),
        ];
        let mut mock = MockSerial::<4>::new(&steps);
        let mut buf = [0u8; 64];

        let result = exchange_inverter(
            &mut mock,
            &FakeInverterVendor,
            InverterCommand::Status,
            &mut buf,
        )
        .await;
        assert_eq!(result, Err(ExchangeError::Parse(ParseError::BadChecksum)));
    }

    #[tokio::test]
    async fn exchange_inverter_propagates_a_genuine_transport_error_during_continuation() {
        let steps = [MockStep::Reply(b"(XX\xFF\r"), MockStep::Timeout];
        let mut mock = MockSerial::<4>::new(&steps);
        let mut buf = [0u8; 64];

        let result = exchange_inverter(
            &mut mock,
            &FakeInverterVendor,
            InverterCommand::Status,
            &mut buf,
        )
        .await;
        assert_eq!(result, Err(ExchangeError::Io(IoError::Timeout)));
    }
}
