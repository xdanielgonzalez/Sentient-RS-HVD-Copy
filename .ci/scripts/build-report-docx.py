#!/usr/bin/env python3
"""Build a docs/*.md report as a .docx (native Word tables + heading styles) via pandoc.

The docx imports cleanly into Google Docs / Word for further editing. Defaults to the
Scope-E security report; pass a different Markdown path to build any docs report.

For the Scope-E report only, the readable ASCII trust-model block in the Markdown (kept
legible on GitHub) is swapped for the tracked PNG (docs/assets/trust-model.png) at build
time. Reports without that block are converted as-is.

Usage:  python .ci/scripts/build-report-docx.py [docs/<report>.md]   (run from the repo root)
Deps:   pypandoc-binary  (bundles the pandoc binary; `pip install pypandoc-binary`)
"""
import re
import sys
import pathlib

import pypandoc

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
DIAGRAM_REL = "assets/trust-model.png"  # relative to docs/ (pandoc resolves against the md dir)

# Source Markdown: CLI arg (path or docs-relative name), else the Scope-E report.
arg = sys.argv[1] if len(sys.argv) > 1 else "security/scope-e-tpm-signing-report.md"
src_md = pathlib.Path(arg)
if not src_md.is_absolute() and not src_md.exists():
    src_md = DOCS / arg
if not src_md.exists():
    sys.exit(f"error: source Markdown not found: {src_md}")
src_md = src_md.resolve()
out_docx = src_md.with_suffix(".docx")

md = src_md.read_text(encoding="utf-8")

# Title = first H1, so the docx carries a document title in its metadata.
m = re.search(r"^# (.+)$", md, re.MULTILINE)
title = m.group(1).strip() if m else src_md.stem

# Swap the fenced ASCII trust-model diagram for the rendered PNG, when present. Optional:
# reports without that block convert unchanged.
pattern = re.compile(r"(### Trust Model at a Glance)\s*\n+```.*?```", re.DOTALL | re.IGNORECASE)
replacement = (
    r"\1" + "\n\n"
    f"![Trust model: build-time signing vs device-time key protection]({DIAGRAM_REL})"
)
md, _ = pattern.subn(replacement, md, count=1)

# pandoc reads images relative to the input file's directory; write the temp md into docs/.
tmp_md = DOCS / ".report_img.tmp.md"
tmp_md.write_text(md, encoding="utf-8")
try:
    pypandoc.convert_file(
        str(tmp_md),
        "docx",
        outputfile=str(out_docx),
        extra_args=[
            f"--resource-path={DOCS}",
            "--from", "gfm+tex_math_dollars",
            "--metadata", f"title={title}",
        ],
    )
finally:
    tmp_md.unlink(missing_ok=True)

print(f"wrote {out_docx.relative_to(ROOT)} ({out_docx.stat().st_size} bytes)")
