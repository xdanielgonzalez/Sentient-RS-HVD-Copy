/// Errors produced while building or decoding Seplos frames.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeplosError {
    /// Frame did not begin with SOI (`~`).
    BadSoi,
    /// Reserved: frame did not end with EOI (`\r`). The parser accepts an EOI-less frame by
    /// design (timeout-framed reads may drop the trailing CR), so this is not currently returned;
    /// it is kept for a future strict mode / callers that require the delimiter.
    MissingEoi,
    /// Frame shorter than the minimum valid response.
    Truncated,
    /// A field expected to be ASCII-hex contained a non-hex byte.
    NonHex,
    /// A field expected to be printable ASCII contained a non-ASCII byte.
    NonAscii,
    /// Frame checksum mismatch.
    BadChecksum { expected: u16, got: u16 },
    /// LENGTH field's LCHKSUM nibble did not validate against its LENID.
    BadLengthChecksum,
    /// LENID did not match the actual INFO payload length.
    LengthMismatch { lenid: u16, actual: usize },
    /// Response return code (RTN) was not success (`00`).
    RtnError(u8),
    /// Response device address did not match the polled DIP address.
    AddrMismatch { expected: u8, got: u8 },
    /// INFO payload was too short for the declared field layout.
    InfoUnderrun,
    /// A count field (cells/temps) exceeded the bounded capacity.
    TooManyElements,
    /// DIP address outside the encodable range (0..=15).
    AddrOutOfRange(u8),
}
