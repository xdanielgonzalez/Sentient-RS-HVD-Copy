/// A fixed-point decimal value in the [`super::json_sink::JsonSink::fixed`] shape: `scaled`
/// interpreted as the value times `10^frac_digits`, with `invert_sign` flipping the sign before
/// formatting. See [`super::json_sink::JsonSink::fixed`]'s doc comment for the worked examples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedNum {
    pub scaled: i64,
    pub frac_digits: u8,
    pub invert_sign: bool,
}
