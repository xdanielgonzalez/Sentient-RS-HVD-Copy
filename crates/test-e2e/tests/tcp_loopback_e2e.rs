//! Two independent `NetStack`s, bridged over an in-memory `WireFrame` loopback, complete a real TCP
//! three-way handshake, exchange known byte strings in both directions, and see them byte-exact.
//!
//! The existing unit tests in `lib-link` (`device.rs::device_bridges_ingress_and_egress`,
//! `stack.rs::stack_builds_and_polls_without_traffic`) only ever exercise a *single* `NetStack`
//! against a single-ended fake channel with no traffic — there is no other coverage of two stacks
//! actually talking TCP to each other. This is that coverage, built on the two-stack bridge helper
//! in the `test-kit` dev-dependency crate (`LoopbackBridge::split`).

// Test code opts out of the workspace's `unwrap_used`/`expect_used`/`panic` lint (production posture
// only) — same convention as the existing unit tests in this crate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use smoltcp::iface::SocketStorage;
use smoltcp::socket::tcp::State;
use smoltcp::wire::{IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr};
use test_kit::LoopbackBridge;

const CLIENT_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
const SERVER_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
const CLIENT_IP: Ipv4Address = Ipv4Address::new(10, 0, 0, 1);
const SERVER_IP: Ipv4Address = Ipv4Address::new(10, 0, 0, 2);
const SERVER_PORT: u16 = 4242;
const CLIENT_PORT: u16 = 55000;
const CLIENT_MSG: &[u8] = b"hello-from-client";
const SERVER_REPLY: &[u8] = b"hello-from-server";
/// Bounded poll budget — a mock millisecond clock, no wall-clock sleeps. If two real stacks can't
/// finish a handshake + one exchange within this many synchronous polls, that's a real failure, not
/// a flaky timing assumption.
const MAX_POLLS: usize = 500;

#[test]
fn two_stacks_complete_tcp_handshake_and_exchange_known_bytes() {
    let (mut device_client, mut device_server) = LoopbackBridge::new(1514).split();

    let client_ip = Ipv4Cidr::new(CLIENT_IP, 24);
    let server_ip = Ipv4Cidr::new(SERVER_IP, 24);

    let mut client_sockets: [SocketStorage; 1] = Default::default();
    let mut server_sockets: [SocketStorage; 1] = Default::default();
    let mut client_stack = lib_net::link::NetStack::new(
        &mut device_client,
        CLIENT_MAC,
        client_ip,
        None,
        &mut client_sockets,
    );
    let mut server_stack = lib_net::link::NetStack::new(
        &mut device_server,
        SERVER_MAC,
        server_ip,
        None,
        &mut server_sockets,
    );

    let mut client_rx = [0u8; 512];
    let mut client_tx = [0u8; 512];
    let mut server_rx = [0u8; 512];
    let mut server_tx = [0u8; 512];
    let client_handle = client_stack.add_tcp(&mut client_rx, &mut client_tx);
    let server_handle = server_stack.add_tcp(&mut server_rx, &mut server_tx);

    server_stack
        .tcp(server_handle)
        .listen(SERVER_PORT)
        .expect("server can listen");
    client_stack
        .connect(
            client_handle,
            IpEndpoint::new(IpAddress::Ipv4(SERVER_IP), SERVER_PORT),
            CLIENT_PORT,
        )
        .expect("client can initiate a connection");

    let mut now_ms: i64 = 0;
    let mut established = false;
    for _ in 0..MAX_POLLS {
        client_stack.poll(&mut device_client, now_ms);
        server_stack.poll(&mut device_server, now_ms);
        if client_stack.tcp(client_handle).state() == State::Established
            && server_stack.tcp(server_handle).state() == State::Established
        {
            established = true;
            break;
        }
        now_ms += 10;
    }
    assert!(
        established,
        "two NetStacks over a WireFrame loopback should complete a TCP 3-way handshake"
    );

    client_stack
        .tcp(client_handle)
        .send_slice(CLIENT_MSG)
        .expect("client can queue its message");

    let mut received = [0u8; 64];
    let mut received_len = 0usize;
    for _ in 0..MAX_POLLS {
        client_stack.poll(&mut device_client, now_ms);
        server_stack.poll(&mut device_server, now_ms);
        if server_stack.tcp(server_handle).can_recv() {
            received_len = server_stack
                .tcp(server_handle)
                .recv_slice(&mut received)
                .expect("server can read what the client sent");
            if received_len > 0 {
                break;
            }
        }
        now_ms += 10;
    }
    assert_eq!(
        &received[..received_len],
        CLIENT_MSG,
        "server should read the client's bytes byte-exact"
    );

    server_stack
        .tcp(server_handle)
        .send_slice(SERVER_REPLY)
        .expect("server can queue its reply");

    let mut reply = [0u8; 64];
    let mut reply_len = 0usize;
    for _ in 0..MAX_POLLS {
        client_stack.poll(&mut device_client, now_ms);
        server_stack.poll(&mut device_server, now_ms);
        if client_stack.tcp(client_handle).can_recv() {
            reply_len = client_stack
                .tcp(client_handle)
                .recv_slice(&mut reply)
                .expect("client can read the server's reply");
            if reply_len > 0 {
                break;
            }
        }
        now_ms += 10;
    }
    assert_eq!(
        &reply[..reply_len],
        SERVER_REPLY,
        "the reply should round-trip to the client byte-exact"
    );
}
