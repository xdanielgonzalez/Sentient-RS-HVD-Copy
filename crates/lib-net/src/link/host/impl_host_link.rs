use super::prelude::*;

fn spawn_rx(mut dl_rx: Box<dyn DataLinkReceiver>, ingress_tx: tok_mpsc::Sender<WireFrame>) {
    thread::spawn(move || {
        // `next()` erroring ends the loop (link gone).
        while let Ok(bytes) = dl_rx.next() {
            // Oversize (jumbo) frames past the MTU are dropped (L2 is lossy; upper layer copes).
            if let Some(frame) = WireFrame::new(FlowDir::Ingress, bytes) {
                if ingress_tx.blocking_send(frame).is_err() {
                    break; // consumer gone
                }
            }
        }
    });
}

fn spawn_tx(mut dl_tx: Box<dyn DataLinkSender>, egress_rx: std_mpsc::Receiver<WireFrame>) {
    thread::spawn(move || {
        while let Ok(frame) = egress_rx.recv() {
            let _ = dl_tx.send_to(&frame.data, None);
        }
    });
}

impl HostLink {
    /// Open a raw datalink on `iface` (e.g. `"eth0"`). Fails closed if the interface is unknown or
    /// the socket cannot be created (`CAP_NET_RAW` missing) — no panic.
    pub fn open(iface: &str) -> Result<Self, LinkError> {
        let interface = pnet_datalink::interfaces()
            .into_iter()
            .find(|i| i.name == iface)
            .ok_or(LinkError::Device)?;
        let mac = interface
            .mac
            .map(|m| [m.0, m.1, m.2, m.3, m.4, m.5])
            .unwrap_or([0u8; 6]);

        let (dl_tx, dl_rx): (Box<dyn DataLinkSender>, Box<dyn DataLinkReceiver>) =
            match pnet_datalink::channel(&interface, Config::default()) {
                Ok(Channel::Ethernet(tx, rx)) => (tx, rx),
                Ok(_) => return Err(LinkError::Device),
                Err(_) => return Err(LinkError::Down), // e.g. EPERM without CAP_NET_RAW
            };

        // rx pump: block on the datalink, forward each frame to the ingress channel.
        let (ingress_tx, ingress) = tok_mpsc::channel::<WireFrame>(64);
        spawn_rx(dl_rx, ingress_tx);

        // tx pump: drain the egress channel, transmit each frame on the datalink.
        let (egress, egress_rx) = std_mpsc::sync_channel::<WireFrame>(64);
        spawn_tx(dl_tx, egress_rx);

        Ok(HostLink {
            ingress,
            egress,
            mac,
        })
    }
}

impl NetLink for HostLink {
    async fn recv(&mut self, out: &mut WireFrame) -> Result<(), LinkError> {
        match self.ingress.recv().await {
            Some(frame) => {
                *out = frame;
                Ok(())
            }
            None => Err(LinkError::Down), // rx pump ended
        }
    }

    async fn send(&mut self, frame: &WireFrame) -> Result<(), LinkError> {
        match self.egress.try_send(frame.clone()) {
            Ok(()) => Ok(()),
            // L2 is lossy: a full tx queue drops the frame; the owned stack retransmits.
            Err(std_mpsc::TrySendError::Full(_)) => Ok(()),
            Err(std_mpsc::TrySendError::Disconnected(_)) => Err(LinkError::Down),
        }
    }

    fn mac(&self) -> [u8; 6] {
        self.mac
    }
}
