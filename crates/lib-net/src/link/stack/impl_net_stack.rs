use super::prelude::*;

impl<'a> NetStack<'a> {
    /// Build the stack for `device` with a MAC, a static IPv4 CIDR, and an optional default gateway.
    /// `sockets_storage` is a caller-owned slice (no heap).
    pub fn new<Tx, Rx>(
        device: &mut ChannelDevice<Tx, Rx>,
        mac: [u8; 6],
        ip: Ipv4Cidr,
        gateway: Option<Ipv4Address>,
        sockets_storage: &'a mut [SocketStorage<'a>],
    ) -> Self
    where
        Tx: Sender<Item = WireFrame>,
        Rx: Receiver<Item = WireFrame>,
    {
        let config = Config::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
        let mut iface = Interface::new(config, device, Instant::ZERO);
        iface.update_ip_addrs(|addrs| {
            let _ = addrs.push(IpCidr::Ipv4(ip));
        });
        if let Some(gw) = gateway {
            let _ = iface.routes_mut().add_default_ipv4_route(gw);
        }
        NetStack {
            iface,
            sockets: SocketSet::new(sockets_storage),
        }
    }

    /// Run one poll at `now_ms` (ms since an arbitrary epoch): process ingress frames, advance
    /// sockets, emit egress frames. Returns true if any socket's readiness changed.
    pub fn poll<Tx, Rx>(&mut self, device: &mut ChannelDevice<Tx, Rx>, now_ms: i64) -> bool
    where
        Tx: Sender<Item = WireFrame>,
        Rx: Receiver<Item = WireFrame>,
    {
        let ts = Instant::from_millis(now_ms);
        matches!(
            self.iface.poll(ts, device, &mut self.sockets),
            PollResult::SocketStateChanged
        )
    }

    /// Milliseconds until the next poll is needed (`None` = wait for an ingress frame). The async
    /// loop should wake on whichever comes first: a new ingress frame or this delay.
    pub fn poll_delay(&mut self, now_ms: i64) -> Option<i64> {
        self.iface
            .poll_delay(Instant::from_millis(now_ms), &self.sockets)
            .map(|d| d.total_millis() as i64)
    }

    /// Add a TCP socket with caller-owned rx/tx buffers (no heap). Returns its handle.
    pub fn add_tcp(&mut self, rx_buf: &'a mut [u8], tx_buf: &'a mut [u8]) -> SocketHandle {
        let socket = tcp::Socket::new(
            tcp::SocketBuffer::new(rx_buf),
            tcp::SocketBuffer::new(tx_buf),
        );
        self.sockets.add(socket)
    }

    /// Initiate an outbound TCP connection on `handle` to `remote`, from `local_port`.
    pub fn connect(
        &mut self,
        handle: SocketHandle,
        remote: impl Into<IpEndpoint>,
        local_port: u16,
    ) -> Result<(), tcp::ConnectError> {
        let socket = self.sockets.get_mut::<tcp::Socket>(handle);
        socket.connect(self.iface.context(), remote, local_port)
    }

    /// Mutable access to a TCP socket (send/recv on the established connection).
    pub fn tcp(&mut self, handle: SocketHandle) -> &mut tcp::Socket<'a> {
        self.sockets.get_mut::<tcp::Socket>(handle)
    }
}
