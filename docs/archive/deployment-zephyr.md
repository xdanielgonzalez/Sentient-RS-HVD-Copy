<p class="eyebrow">Deployment · MCU Forward Path</p>

# MCU: Zephyr build & flash

The MCU forward path ships Zephyr (`no_std`) and reuses the portable core verbatim behind the
`SerialTransport`/`Publisher` seams. This chapter embeds the canonical build/flash guide from
`bin/zephyr/README.md`.

{{#include ../../bin/zephyr/README.md}}
