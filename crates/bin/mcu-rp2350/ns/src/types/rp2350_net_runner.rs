/// W5500 IP/DHCP/TCP pump runner type alias (for task signatures that need a concrete type).
pub(crate) type Rp2350NetRunner = embassy_net::Runner<'static, embassy_net_wiznet::Device<'static>>;
