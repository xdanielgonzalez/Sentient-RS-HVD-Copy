//! The **W5500 device leg** (feature `embassy-wiznet`, EPIC #350 story S8 #359): brings the
//! on-board WIZnet W5500 Ethernet controller up as an `embassy-net` `Device`/`Runner` pair the
//! existing [`crate::embassy`] Stack path consumes — the same `embassy_net_driver_channel::Device`
//! the [`crate::embassy::adapter`] produces from a [`crate::link::NetLink`], just sourced from the
//! WIZnet SPI driver instead. So a W5500 sits under `embassy_net::Stack` exactly like the
//! adapter-bridged NetLink drivers do; the Stack/socket/broker machinery above it is unchanged.
//!
//! **HAL-free by construction (the seam rule):** [`build_w5500_stack`] is generic over the caller's
//! `embedded-hal-async` SPI (`SPI: SpiDevice`) and interrupt line (`INT: Wait`) plus the blocking
//! `embedded-hal` reset pin (`RST: OutputPin`). It names NO `embassy-rp` type — the concrete
//! `embassy_rp::spi::Spi`/`Output`/`Input` (or any other MCU's HAL) is supplied at the call site in
//! `bin/mcu-rp2350`, never here. This is why the W5500 bring-up belongs behind the lib-net seam and not
//! forked into the bin: only the pin/SPI wiring is board-specific; the device-under-Stack assembly
//! is portable.
//!
//! The returned [`W5500Parts`]'s two runners (`net_runner`, `wiznet_runner`) must each be spawned as
//! their own `#[embassy_executor::task]`. Those task fns must be concrete/non-generic (see
//! [`crate::embassy::host`]'s doc), so the concrete task wrappers live at the bin call site — this
//! module supplies the reusable, board-agnostic builder.

mod types;

pub use types::W5500Parts;

use embassy_net::{
    Config, ConfigV6, Ipv6Address, Stack, StackResources, StaticConfigV4, StaticConfigV6,
};
use embassy_net_wiznet::chip::W5500;
use embassy_net_wiznet::{InitError, State};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;

/// Bring the W5500 up over `spi_dev`/`int`/`reset` and assemble the `embassy_net::Stack` over it,
/// from caller-owned `'static` storage (the standard embassy pattern: the caller promotes the
/// WIZnet `State` and `StackResources` to `'static` via `static_cell::StaticCell` and passes
/// `&'static mut` in, rather than this fn leaking anything).
///
/// * `mac` — the L2 MAC the W5500 (and DHCP/ARP) report. Caller-owned (a per-device provisioned MAC
///   in production; a locally-administered placeholder on the bench).
/// * `config` — the `embassy_net::Config` (typically `Config::dhcpv4(Default::default())` so the
///   device acquires a lease the bench operator connects the broker on).
/// * `N_RX`/`N_TX` size the WIZnet driver's packet queues; `SOCK` sizes the stack's socket table
///   (the device-hosted broker needs one listening socket per concurrent client slot, plus DHCP).
///
/// Returns [`InitError`] if the W5500 does not respond on SPI (chip absent / mis-wired) — the caller
/// logs it and leaves the rest of the device running, rather than panicking the image over a leg
/// that is not the primary mission (mirrors the bin's fail-open posture).
#[allow(clippy::too_many_arguments)]
pub async fn build_w5500_stack<
    'd,
    SPI,
    INT,
    RST,
    const N_RX: usize,
    const N_TX: usize,
    const SOCK: usize,
>(
    mac: [u8; 6],
    wiznet_state: &'d mut State<N_RX, N_TX>,
    stack_resources: &'d mut StackResources<SOCK>,
    spi_dev: SPI,
    int: INT,
    reset: RST,
    config: Config,
    random_seed: u64,
) -> Result<W5500Parts<'d, SPI, INT, RST>, InitError<SPI::Error>>
where
    SPI: SpiDevice,
    INT: Wait,
    RST: OutputPin,
{
    let (device, wiznet_runner) = embassy_net_wiznet::new::<N_RX, N_TX, W5500, SPI, INT, RST>(
        mac,
        wiznet_state,
        spi_dev,
        int,
        reset,
    )
    .await?;
    let (stack, net_runner) = embassy_net::new(device, config, stack_resources, random_seed);

    // Make the W5500 leg DUAL-STACK (MIL-STD-3071 DDS/RTPS runs over UDP/IPv6, domain 0, SPDP
    // multicast ff02::1234 — see docs/tms-compliance.md, #328). `config` above drives IPv4 (DHCP);
    // v6 is orthogonal and `embassy_net::Config` cannot carry BOTH `dhcpv4` AND a v6 mode at once
    // (`Config::dhcpv4` hard-sets `ipv6: ConfigV6::None`; the struct is `#[non_exhaustive]`), so we
    // enable IPv6 SLAAC on the assembled Stack instead. This (a) makes
    // `Inner::apply_static_config` assign the interface its `fe80::` link-local address
    // (embassy-net-0.9.1 src/lib.rs:813-820, gated `#[cfg(feature = "slaac")]`) so the device
    // genuinely answers on IPv6 immediately, and (b) accepts a router-advertised GLOBAL prefix via
    // SLAAC, surfaced by `wait_for_config_v6`/`Stack::config_v6`. `multicast` (pulled in by `slaac`)
    // lets the DDS leg later `stack.join_multicast_group(ff02::1234)` for SPDP discovery.
    stack.set_config_v6(ConfigV6::Slaac);

    Ok(W5500Parts {
        stack,
        net_runner,
        wiznet_runner,
    })
}

/// Await a completed DHCPv4 (or static) bring-up and return the acquired IPv4 configuration —
/// mirrors the canonical `wait_for_config` from embassy's own `rp235x/ethernet_w5500_*` examples
/// (loop `config_v4()` + `yield_now`), but seeded by `wait_config_up()` so it suspends instead of
/// busy-polling before the very first lease. The address/gateway are then the caller's to render
/// (over defmt/RTT on a console-less board, etc.) — this seam stays logging-free.
///
/// Belongs in the seam, not the bin: acquiring the lease is portable embassy-net behavior; only the
/// rendering of the result is board-specific.
pub async fn wait_for_config(stack: Stack<'static>) -> StaticConfigV4 {
    stack.wait_config_up().await;
    loop {
        if let Some(config) = stack.config_v4() {
            return config;
        }
        // `wait_config_up` returned on a non-v4 bring-up (e.g. link-local v6 only); re-poll
        // cooperatively until the v4 lease lands rather than returning a bogus address.
        embassy_futures::yield_now().await;
    }
}

/// Await a router-advertised GLOBAL IPv6 configuration (SLAAC) and return it — the v6 sibling of
/// [`wait_for_config`]. NOTE: [`Stack::config_v6`] reports ONLY a global address (embassy-net's SLAAC
/// path explicitly filters the `fe80::` link-local out — embassy-net-0.9.1 src/lib.rs:932-960), so on
/// a bench with no IPv6 router this NEVER resolves. The link-local (which the interface always holds
/// once [`build_w5500_stack`] enables SLAAC) is instead rendered deterministically via
/// [`link_local_v6`]; use THAT for immediate IPv6 observability and this only where a global address
/// is actually expected. Kept logging-free — the caller renders (defmt/RTT on a console-less board).
pub async fn wait_for_config_v6(stack: Stack<'static>) -> StaticConfigV6 {
    loop {
        if let Some(config) = stack.config_v6() {
            return config;
        }
        embassy_futures::yield_now().await;
    }
}

/// Derive the IPv6 link-local address (`fe80::/64` + modified-EUI-64 interface identifier, RFC 4291
/// §2.5.1) that embassy-net assigns the interface for a given Ethernet `mac` under SLAAC. This
/// reproduces, byte-for-byte, `EthernetAddress::as_eui_64` (smoltcp-0.13.1 src/wire/ethernet.rs:70-78,
/// including the U/L-bit flip) fed through the `fe80::/64` prefix, exactly as embassy-net's
/// `Inner::get_link_local_address` (embassy-net-0.9.1 src/lib.rs:678-683) does — so the value returned
/// here provably equals the address on the wire, WITHOUT needing a (nonexistent) public accessor for
/// the interface's assigned addresses. The caller renders it (the seam stays logging-free).
pub fn link_local_v6(mac: [u8; 6]) -> Ipv6Address {
    // Modified EUI-64 interface identifier from the 48-bit MAC.
    let iid = [
        mac[0] ^ 0x02, // flip the Universal/Local bit
        mac[1],
        mac[2],
        0xff,
        0xfe,
        mac[3],
        mac[4],
        mac[5],
    ];
    Ipv6Address::new(
        0xfe80,
        0,
        0,
        0,
        u16::from_be_bytes([iid[0], iid[1]]),
        u16::from_be_bytes([iid[2], iid[3]]),
        u16::from_be_bytes([iid[4], iid[5]]),
        u16::from_be_bytes([iid[6], iid[7]]),
    )
}
