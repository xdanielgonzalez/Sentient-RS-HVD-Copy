// `DiskTap`-specific forwarding (offering `TapEvent::Published` to `handle.tx`) lives in
// `HostTap::offer` (`impl_host_tap.rs`), grouped with the other tap variants' dispatch — nothing
// tap-specific here.
