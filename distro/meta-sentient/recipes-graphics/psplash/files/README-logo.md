# Sentient METEOR branded boot splash

The boot splash (blue brand mark + `SENTIENT INDUSTRIES` wordmark + release version) is **generated**,
not hand-committed as a fixed image — so the version on the boot screen always matches the shipped
build.

## Files
- `mark.png` — the blue brand mark (transparent), the one static asset.
- `gen-splash.py` — the generator. Composes `mark.png` + the wordmark + `--version <v>` onto a black
  16:9 canvas via **Cairo** (`cairosvg`), matching how psplash scales a fullscreen image.
- `sentient-logo.png` — a **committed default** (rendered at the distro version) so a plain
  `kas build` is self-contained. The pipeline overwrites it with the real release version.

## How it's wired
`psplash_%.bbappend` sets `SPLASH_IMAGES = "file://sentient-logo.png;outsuffix=default"`. The stock
`psplash_git.bb` `do_compile` runs `make-image-header.sh` on that PNG and compiles it into the binary
as `psplash-poky-img.h` (verified against the pinned recipe — no patch or committed C header needed).

## Regenerate (pipeline + local)
```sh
python3 gen-splash.py --version 1.2.3 --out sentient-logo.png
# fonts: uses Inter (wordmark) + Roboto Mono (version) if installed, else generic sans/monospace.
```
The release/image build runs exactly this with the real version before `kas build` (the yocto builder
image carries `cairosvg` + the Inter/Roboto-Mono fonts). Deps: `cairosvg` (+ system Cairo/librsvg).

## Provenance
`mark.png` is the customer's own brand mark (from sentient.industries), isolated + trimmed for the
splash; the wordmark is set as text in the brand font. Both are the customer's own assets for their
own device.
