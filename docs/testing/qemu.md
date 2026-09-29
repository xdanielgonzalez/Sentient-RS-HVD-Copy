<p class="eyebrow">Testing · Image Emulation</p>

# QEMU image testing

The Yocto Pi image is exercised end-to-end under QEMU (`qemuarm64`) — possible because the Linux
guest brings a real, emulatable network stack. This chapter embeds the canonical harness
documentation from `distro/qemu/README.md`.

{{#include ../../distro/qemu/README.md}}
