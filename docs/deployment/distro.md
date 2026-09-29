<p class="eyebrow">Deployment · Yocto Image</p>

# Yocto image & OS hardening

The Raspberry Pi shipping target runs a minimal, hardened Yocto Linux image. All BitBake/Yocto
content lives under [`distro/`](https://github.com/High-Velocity-Decisions/sentient-rs/tree/main/distro)
at the repo root — this chapter embeds `distro/README.md` (the layer layout and image build) and
`distro/HARDENING.md` (the hardening checklist that is the compensating control for moving the Pi
off bare Zephyr).

{{#include ../../distro/README.md}}

## Hardening checklist

The following is the canonical hardening checklist from `distro/HARDENING.md`.

{{#include ../../distro/HARDENING.md}}
