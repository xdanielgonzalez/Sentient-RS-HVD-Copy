use crate::prelude::*;

impl ParserBufferPool {
    /// All-zero pool, ready for `static` placement (never a per-call stack local — the repo's
    /// zero-copy/stack-discipline rule).
    pub const fn new() -> Self {
        ParserBufferPool {
            bytes: [0u8; PARSER_BUFFER_POOL_LEN],
        }
    }

    /// Base address of the pool's first byte, for `mpu::init_ns_parser_region`'s `base` argument.
    /// Only meaningful once this value has `'static` storage (a `static` in `main.rs`) — the
    /// pointer is worthless once the pool is dropped, which a `static` never is.
    pub fn base_addr(&self) -> u32 {
        self.bytes.as_ptr() as u32
    }

    /// Length in bytes, for `mpu::init_ns_parser_region`'s `len` argument.
    pub const fn pool_len(&self) -> u32 {
        PARSER_BUFFER_POOL_LEN as u32
    }
}

impl Default for ParserBufferPool {
    fn default() -> Self {
        Self::new()
    }
}
