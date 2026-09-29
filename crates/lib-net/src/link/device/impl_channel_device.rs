use super::prelude::*;

impl<Tx, Rx> ChannelDevice<Tx, Rx> {
    /// Build a device over the wire-byte channels. `mtu` is the L2 MTU (≤ [`WIRE_MTU`]).
    pub fn new(egress: Tx, ingress: Rx, mtu: usize) -> Self {
        ChannelDevice {
            egress,
            ingress,
            mtu: mtu.min(WIRE_MTU),
        }
    }
}

impl<Tx, Rx> phy::Device for ChannelDevice<Tx, Rx>
where
    Tx: Sender<Item = WireFrame>,
    Rx: Receiver<Item = WireFrame>,
{
    type RxToken<'a>
        = RxToken
    where
        Self: 'a;
    type TxToken<'a>
        = TxToken<'a, Tx>
    where
        Self: 'a;

    fn receive(&mut self, _t: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        match self.ingress.try_recv() {
            Ok(frame) => Some((
                RxToken { frame },
                TxToken {
                    egress: &mut self.egress,
                },
            )),
            Err(_) => None,
        }
    }

    fn transmit(&mut self, _t: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxToken {
            egress: &mut self.egress,
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = self.mtu;
        caps
    }
}
