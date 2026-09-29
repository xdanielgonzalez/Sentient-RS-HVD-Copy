/// Where the session is in the connection handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::link::mqtt_session) enum State {
    /// CONNECT not yet staged (the very first poll stages it).
    Init,
    /// CONNECT sent, waiting for the broker's CONNACK.
    AwaitingConnAck,
    /// CONNACK accepted — PUBLISH/keep-alive are now allowed.
    Connected,
    /// The handshake ended unrecoverably (refused or malformed CONNACK); no further progress.
    Dead,
}
