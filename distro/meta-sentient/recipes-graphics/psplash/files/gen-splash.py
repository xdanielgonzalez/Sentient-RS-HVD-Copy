#!/usr/bin/env python3
"""Generate the Sentient METEOR boot splash (the psplash source PNG).

Renders the brand mark + "SENTIENT INDUSTRIES" wordmark + the release version onto a black
canvas via the Cairo rasterizer (cairosvg), so the boot screen carries the exact version baked
in at image-build/release time. The image pipeline invokes this with --version <release-version>
before `kas build`; psplash then compiles the produced PNG into its splash.

Deps: cairosvg (+ its system Cairo/librsvg). Fonts: uses Inter (wordmark) + Roboto Mono (version)
if installed in the build image, else falls back to the generic sans/monospace families — so it
renders anywhere, and matches the brand exactly once those fonts are baked into the builder image.
"""
import argparse
import base64
import struct
from pathlib import Path

import cairosvg


def png_size(path: Path):
    b = path.read_bytes()
    if b[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError(f"{path} is not a PNG")
    w, h = struct.unpack(">II", b[16:24])
    return w, h


def build_svg(mark_b64: str, mark_w: int, mark_h: int, version: str,
              W: int, H: int, sans: str, mono: str) -> str:
    cx = W // 2
    # Brand mark, centered in the upper half.
    mh = int(H * 0.42)
    mw = round(mark_w * (mh / mark_h))
    mx = cx - mw // 2
    my = int(H * 0.13)
    # Wordmark under the mark.
    word_y = my + mh + int(H * 0.11)
    word_fs = int(H * 0.058)
    # Divider + version under the wordmark (echoes the brand's technical version label).
    rule_y = word_y + int(H * 0.05)
    rule_w = int(W * 0.30)
    ver_y = rule_y + int(H * 0.075)
    ver_fs = int(H * 0.040)
    ver = version if version.lower().startswith("v") else f"v{version}"
    return f"""<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}">
  <rect width="100%" height="100%" fill="#000000"/>
  <image x="{mx}" y="{my}" width="{mw}" height="{mh}" xlink:href="data:image/png;base64,{mark_b64}"/>
  <text x="{cx}" y="{word_y}" fill="#FFFFFF" text-anchor="middle" font-family="{sans}" font-weight="700" font-size="{word_fs}" letter-spacing="{word_fs*0.18:.1f}">SENTIENT INDUSTRIES</text>
  <line x1="{cx-rule_w//2}" y1="{rule_y}" x2="{cx+rule_w//2}" y2="{rule_y}" stroke="#2b2b2b" stroke-width="2"/>
  <text x="{cx}" y="{ver_y}" fill="#8a8a8a" text-anchor="middle" font-family="{mono}" font-size="{ver_fs}" letter-spacing="{ver_fs*0.10:.1f}">{ver}</text>
</svg>"""


def main():
    ap = argparse.ArgumentParser(description="Render the versioned Sentient boot splash.")
    ap.add_argument("--version", required=True, help="release/image version, e.g. 1.0.0")
    ap.add_argument("--mark", default=str(Path(__file__).parent / "mark.png"),
                    help="brand mark PNG (transparent)")
    ap.add_argument("--out", default="sentient-logo.png", help="output PNG path")
    ap.add_argument("--width", type=int, default=1280)
    ap.add_argument("--height", type=int, default=720)
    ap.add_argument("--sans", default="Inter, sans-serif", help="wordmark font family")
    ap.add_argument("--mono", default="Roboto Mono, monospace", help="version font family")
    a = ap.parse_args()

    mark_path = Path(a.mark)
    mw, mh = png_size(mark_path)
    mark_b64 = base64.b64encode(mark_path.read_bytes()).decode("ascii")
    svg = build_svg(mark_b64, mw, mh, a.version, a.width, a.height, a.sans, a.mono)
    cairosvg.svg2png(bytestring=svg.encode("utf-8"), write_to=a.out,
                     output_width=a.width, output_height=a.height, background_color="#000000")
    print(f"wrote {a.out} ({a.width}x{a.height}) version={a.version}")


if __name__ == "__main__":
    main()
