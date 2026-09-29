/// Error from an INA3221 register I2C exchange: wraps the injected bus's own error type `E`
/// (`I2C::Error` from `embedded_hal_async::i2c::I2c`).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ina3221Error<E> {
    /// The underlying I2C bus reported an error (NACK, bus busy, arbitration lost, ...).
    Bus(E),
}
