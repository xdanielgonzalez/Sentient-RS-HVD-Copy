<p class="eyebrow">Testing · MCU Emulation</p>

# Renode MCU emulation

CI boots the `bin-zephyr` build under Renode on the `qemu_cortex_m3` substrate and asserts the
boot banner on the console UART. This chapter embeds the canonical harness documentation from
`bin/zephyr/renode/README.md`.

{{#include ../../bin/zephyr/renode/README.md}}
