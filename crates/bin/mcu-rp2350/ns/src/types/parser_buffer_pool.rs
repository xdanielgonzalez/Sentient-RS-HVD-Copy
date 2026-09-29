/// Length (bytes) of [`ParserBufferPool`]'s scratch. Matches `impl_app::run`'s own Seplos frame
/// scratch (`let mut buf = [0u8; 256]`) and this crate's UART ring buffers (`UART_BUF_LEN`,
/// `src/main.rs`) — a Seplos telemetry frame is < 256 bytes.
pub const PARSER_BUFFER_POOL_LEN: usize = 256;

/// A 32-byte-aligned static scratch buffer standing in for "the parser task's working buffers" —
/// the MPU region [`crate::mpu::init_ns_parser_region`] scopes.
///
/// **Demo scaffold, honestly scoped** (see the crate root doc comment): this is a DEDICATED
/// static introduced for the MPU demonstration, not (yet) the live buffer `impl_app::run`/
/// `lib_io` actually parse into — wiring the MPU region around the real in-flight parser buffer
/// would require restructuring how `impl_app::run`'s stack-local `buf` is supplied (a `crates/**`
/// change, out of this crate's territory for Stage 4 story #355). `#[repr(C, align(32))]`:
/// PMSAv8 (Armv8-M) MPU regions need a base address aligned to the region granularity (32 bytes)
/// — see `crate::mpu`'s module doc.
#[repr(C, align(32))]
pub struct ParserBufferPool {
    pub(crate) bytes: [u8; PARSER_BUFFER_POOL_LEN],
}
