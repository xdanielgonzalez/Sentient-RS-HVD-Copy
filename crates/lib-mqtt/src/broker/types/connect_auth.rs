/// A broker-side CONNECT authorizer, built by [`crate::broker::seam_auth`]. Opaque wrapper around an
/// [`rumqttd::AuthHandler`] closure so [`crate::broker::BrokerConfig`] can stay `Debug`/`Clone` without requiring
/// the closure itself to implement `Debug`.
#[derive(Clone)]
pub struct ConnectAuth(pub(crate) rumqttd::AuthHandler);
