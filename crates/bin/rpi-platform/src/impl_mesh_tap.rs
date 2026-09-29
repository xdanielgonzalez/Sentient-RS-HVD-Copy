// `MeshTap`-specific forwarding (condensing `TapEvent::Reading` and offering it to `handle.tx`)
// lives in `HostTap::offer` (`impl_host_tap.rs`), grouped with the other tap variants' dispatch —
// nothing tap-specific here.
