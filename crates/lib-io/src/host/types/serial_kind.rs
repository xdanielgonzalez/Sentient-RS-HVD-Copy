/// Which serial kind to construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialKind {
    Rs232,
    Rs485,
}
