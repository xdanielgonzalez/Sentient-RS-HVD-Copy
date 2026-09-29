<p class="eyebrow">Deployment · MCU Firmware</p>

# MCU firmware: RP2350 build & flash

The MCU shipping target is an Adafruit Feather RP2350 (8 MB flash, 520 KB SRAM) running the
Embassy firmware in [`crates/bin/mcu-rp2350/`](https://github.com/High-Velocity-Decisions/sentient-rs/tree/main/crates/bin/mcu-rp2350),
split across a TrustZone-M Secure world (`s`), a Non-Secure world (`ns`), and the frozen NS↔S
veneer ABI (`abi`). It is its **own excluded cargo workspace** — its own `Cargo.lock` and its own
`thumbv8m.main-none-eabihf` toolchain pins — so it is never reached by a `cargo build --workspace`
from the repo root. This chapter embeds `crates/bin/mcu-rp2350/README.md`, the canonical build order,
memory split, pinout, and bring-up reference for that firmware.

## Scope of this chapter

Two gaps are worth knowing before you rely on anything below.

**There is no boot verification in CI.** The MCU lane (`_mcu-build.yml`, called from `uat.yml` and
`production.yml`) cross-compiles the feature matrix, links it, and uploads the `ns` and `rp2350-s`
ELFs as artifacts. Nothing boots them. The Renode boot-smoke lane was retired with the
Zephyr-to-Embassy pivot ([ADR 0010](../decisions/0010-mcu-runtime-embassy.md)) and remains deferred
under #191; no Renode or QEMU model for this chip exists to replace it. The only boot evidence is a
manual bench capture over SWD/RTT, and it covers the **standalone** `ns` image (default features,
flash base `0x1000_0000`) — boot through W5500 link-up and DHCP. The Secure-to-Non-Secure `veneers`
handoff is proven to compile and link only; see the README's "Known gaps" below.

**The UF2 packaging path is documentation, not automation.** `just mcu-rp2350-uf2` prints the
manual picotool combine/sign/flash steps and executes nothing — same for `just mcu-rp2350-rtt`.
Neither is run in CI, and the picotool flags are version-sensitive enough that the recipe tells you
to check them against your installed `picotool version` rather than trusting the printed example.
The executable bench flow is the `MCU HIL bench` recipe group (`mcu-flash-device`, `mcu-rtt`),
which drives OpenOCD over the bench Pi's SWD wires.

{{#include ../../crates/bin/mcu-rp2350/README.md}}
