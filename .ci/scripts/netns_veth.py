#!/usr/bin/env python3
"""netns_veth.py — dependency-free `ip link add veth0 type veth peer name eth0` + `... up`.

Replaces the `ip` invocations `_dds_interop.yml`'s nextest step used inside `unshare -r -n`, because
the CI job's container image (sentient-builder) has no iproute2 and the runner is non-root (no apt
to add it). Two facts are already proven empirically before this script existed:

  * userns + IPv6 multicast both work fine in that container (the job's "ipv6 multicast self-probe"
    step: `unshare -r -n true` -> USERNS-OK, and the SO_REUSEADDR ff02::1234%eth0 probe -> MCAST-OK).
  * an `lo`-only netns does NOT work — `lo` lacks IFF_MULTICAST, so `dds_interop_e2e`'s real RTPS SPDP
    multicast fails even though `lo` trivially has an address. A veth pair (with a real Ethernet-class
    link, `eth0`) is genuinely required; there is no shortcut through `lo`.

So the only missing piece is veth-pair creation + bringing links up — pure `iproute2` CLI convenience
over two syscalls this script does directly with the stdlib: `RTM_NEWLINK` over an `AF_NETLINK`/
`NETLINK_ROUTE` socket (create the veth pair), and `SIOCGIFFLAGS`/`SIOCSIFFLAGS` ioctls (bring `lo`,
`veth0`, `eth0` up). Runs *inside* the already-unshared netns (as the child of `unshare -r -n bash -c
"python3 netns_veth.py && ..."`), so the created links land directly in the right namespace — no
`ip link set netns` hand-off needed.

Netlink attribute encoding follows RTM_NEWLINK's veth-kind convention (see iproute2's
`ip/iplink_vlan.c`-style IFLA_LINKINFO nesting): `IFLA_LINKINFO` nests `IFLA_INFO_KIND="veth"` +
`IFLA_INFO_DATA` -> `VETH_INFO_PEER` -> a nested `ifinfomsg` + the peer's own `IFLA_IFNAME`. All
netlink attributes are 4-byte aligned (`NLMSG_ALIGNTO`/`RTA_ALIGNTO` are both 4 regardless of word
size), which every `rta_raw`/`rta_str` call below pads for.

Usage: netns_veth.py [--veth NAME] [--peer NAME] [--timeout SECONDS]
Exit 0 iff the pair is created, all three links (lo/veth0/eth0) are brought up, and the peer link
acquires a kernel-assigned fe80 (link-local) address within the timeout (proof IPv6 is live on it).
Nonzero exit with a message on stderr on any failure.
"""

import argparse
import os
import socket
import struct
import sys
import time
from fcntl import ioctl

# ---- netlink (RTM_NEWLINK / rtnetlink) constants, from linux/rtnetlink.h + linux/if_link.h --------
RTM_NEWLINK = 16
NLMSG_ERROR = 2

NLM_F_REQUEST = 0x01
NLM_F_ACK = 0x04
NLM_F_EXCL = 0x200
NLM_F_CREATE = 0x400

AF_UNSPEC = 0

IFLA_IFNAME = 3
IFLA_LINKINFO = 18
IFLA_INFO_KIND = 1
IFLA_INFO_DATA = 2
VETH_INFO_PEER = 1

# ---- ioctl (SIOCxIFFLAGS), from linux/sockios.h + net/if.h -----------------------------------------
SIOCGIFFLAGS = 0x8913
SIOCSIFFLAGS = 0x8914
IFF_UP = 0x1
IFNAMSIZ = 16


def die(msg: str) -> None:
    print(f"netns_veth: FAILED: {msg}", file=sys.stderr)
    sys.exit(1)


def _pad4(n: int) -> int:
    return (4 - (n % 4)) % 4


def rta_raw(rta_type: int, data: bytes) -> bytes:
    """One netlink attribute (rtattr header + raw payload), padded to a 4-byte boundary."""
    rta_len = 4 + len(data)
    return struct.pack("=HH", rta_len, rta_type) + data + b"\0" * _pad4(rta_len)


def rta_str(rta_type: int, s: str) -> bytes:
    """A NUL-terminated string attribute (IFLA_IFNAME / IFLA_INFO_KIND convention)."""
    return rta_raw(rta_type, s.encode("ascii") + b"\0")


def create_veth(veth_name: str, peer_name: str) -> None:
    """RTM_NEWLINK a veth pair: `veth_name` in this netns, `peer_name` as its peer."""
    # ifinfomsg: family(B) pad(B) type(H) index(i) flags(I) change(I) = 16 bytes, all zero/AF_UNSPEC
    # for a fresh link — the kernel fills in a real index and lets the veth driver set link type.
    ifi = struct.pack("=BBHiII", AF_UNSPEC, 0, 0, 0, 0, 0)

    peer_body = ifi + rta_str(IFLA_IFNAME, peer_name)
    veth_info_peer = rta_raw(VETH_INFO_PEER, peer_body)
    info_kind = rta_str(IFLA_INFO_KIND, "veth")
    info_data = rta_raw(IFLA_INFO_DATA, veth_info_peer)
    linkinfo = rta_raw(IFLA_LINKINFO, info_kind + info_data)

    body = ifi + rta_str(IFLA_IFNAME, veth_name) + linkinfo
    flags = NLM_F_REQUEST | NLM_F_CREATE | NLM_F_EXCL | NLM_F_ACK
    seq = os.getpid() & 0xFFFF or 1
    header = struct.pack("=IHHII", 16 + len(body), RTM_NEWLINK, flags, seq, 0)
    msg = header + body

    sock = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, socket.NETLINK_ROUTE)
    try:
        sock.bind((0, 0))
        sock.send(msg)
        resp = sock.recv(65536)
    finally:
        sock.close()

    if len(resp) < 16:
        die(f"netlink response too short ({len(resp)} bytes) creating {veth_name}/{peer_name}")
    _r_len, r_type, _r_flags, _r_seq, _r_pid = struct.unpack_from("=IHHII", resp, 0)
    if r_type != NLMSG_ERROR or len(resp) < 20:
        die(f"unexpected netlink response type {r_type} (expected an NLMSG_ERROR ack)")
    (errno_val,) = struct.unpack_from("=i", resp, 16)
    if errno_val != 0:
        die(
            f"RTM_NEWLINK {veth_name}/{peer_name} failed: "
            f"{os.strerror(-errno_val)} (errno {-errno_val})"
        )


def set_link_up(name: str) -> None:
    """SIOCGIFFLAGS then SIOCSIFFLAGS(|= IFF_UP) — the ioctl equivalent of `ip link set <name> up`."""
    ifreq = name.encode("ascii")[: IFNAMSIZ - 1].ljust(IFNAMSIZ, b"\0") + b"\0" * 16
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        try:
            cur = ioctl(sock.fileno(), SIOCGIFFLAGS, ifreq)
        except OSError as e:
            die(f"SIOCGIFFLAGS({name}) failed: {e}")
        (flags,) = struct.unpack_from("=H", cur, IFNAMSIZ)
        newreq = cur[:IFNAMSIZ] + struct.pack("=H", flags | IFF_UP) + cur[IFNAMSIZ + 2 : 32]
        try:
            ioctl(sock.fileno(), SIOCSIFFLAGS, newreq)
        except OSError as e:
            die(f"SIOCSIFFLAGS({name}, +UP) failed: {e}")
    finally:
        sock.close()


def wait_for_fe80(name: str, timeout: float) -> bool:
    """Poll /proc/net/if_inet6 until `name` shows a fe80 (link-local) row, or timeout."""
    deadline = time.time() + timeout
    while True:
        try:
            with open("/proc/net/if_inet6") as f:
                for line in f:
                    parts = line.split()
                    if len(parts) >= 6 and parts[-1] == name and parts[0].startswith("fe80"):
                        return True
        except FileNotFoundError:
            die("/proc/net/if_inet6 missing — IPv6 disabled in this kernel/netns?")
        if time.time() >= deadline:
            return False
        time.sleep(0.1)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--veth", default="veth0", help="local end name (default: veth0)")
    ap.add_argument("--peer", default="eth0", help="peer end name (default: eth0)")
    ap.add_argument("--timeout", type=float, default=5.0, help="seconds to wait for peer fe80 (default: 5)")
    args = ap.parse_args()

    create_veth(args.veth, args.peer)
    for name in ("lo", args.veth, args.peer):
        set_link_up(name)

    if not wait_for_fe80(args.peer, args.timeout):
        die(
            f"{args.peer} did not acquire an fe80 link-local address within {args.timeout}s after "
            "UP — veth creation may have silently landed in the wrong netns, or IPv6 is disabled"
        )

    print(f"netns_veth: OK ({args.veth}/{args.peer} created, lo/{args.veth}/{args.peer} up, "
          f"{args.peer} has fe80)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
