#!/usr/bin/env python3
"""Type/impl file-separation audit + smell-test gate (CLAUDE.md "Code Organization").

The rule: a type definition (`struct`/`enum`/`trait`/`union`/alias) goes in its own
file (one primary type per file, colocated under a `types/` sub-module — no `type_`
prefix); its impls go in `impl_<name>.rs` at the module root. A source file must NOT mix
a type with its own impl, nor hold multiple primary types. This tool finds violations
deterministically.

Modes:
  (default)          human breakdown: every file's `type -> [impls]` map, size-routed.
  --check            gate: exit 1 if any CURRENT violation is not in the baseline
                     (i.e. a NEW smell). Existing baselined violations are tolerated so
                     the gate can land before the refactor finishes; shrink the baseline
                     as files are fixed. Also warns about stale (now-fixed) baseline lines.
  --write-baseline   overwrite the baseline file with the current violation set.

`#[cfg(test)]` modules are skipped (their helper types aren't production types).
`impl_*.rs`/`mod.rs` (and any residual `type_*.rs`) are skipped by name; the un-prefixed
type files under `types/` ARE scanned — a single-type file is CLEAN, and this is what
keeps the one-type-per-file rule enforced now that the `type_` prefix is gone.
"""
import os, re, sys

ROOTS = ["crates", "bin-rpi/src", "hil/pico-seplos-emu-core/src", "hil/pico-seplos-emu/src"]
BASELINE = ".ci/scripts/typeimpl_baseline.txt"
EXCLUDE_DIR = re.compile(r"/(tests|benches|examples)/|/target/|/zephyr/|/modules/|/\.west/|/vendor/")
TYPE_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait|union)\s+([A-Za-z_]\w*)')
ALIAS_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_]\w*)\s*[=<]')
IMPL_RE = re.compile(r'^\s*impl(?:\s*<[^>]*>)?\s+(.+?)\s*(?:\{|where\b)')
MOD_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*\{')
MASSIVE, LARGE = 400, 250

def base_type(t):
    t = t.strip().lstrip('&').replace("mut ", "").strip()
    return re.split(r'[<\s]', t, 1)[0].split("::")[-1]

def is_generated(path):
    """A machine-generated file (banner `@generated` on the first line) is EXEMPT: it holds many
    types + their impls by design (e.g. `lib-tms-model/src/generated.rs`, the whole IDL type
    catalogue emitted by `tools/tms-idl-codegen`). The one-type-per-file rule is a human-authoring
    ergonomic; it does not apply to a DO-NOT-EDIT emitted artifact."""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return "@generated" in f.readline()
    except OSError:
        return False

def parse(path):
    types, impls, loc, prev, skip_depth = {}, [], 0, "", 0
    for line in open(path, encoding="utf-8", errors="replace"):
        loc += 1
        if skip_depth > 0:                      # inside an inline submodule body: skip
            skip_depth += line.count('{') - line.count('}')
            prev = line; continue
        m = MOD_RE.match(line)
        if m:                                   # any INLINE `mod NAME { … }` — a `#[cfg(test)]`
            # helper module OR a deliberate nested namespace (e.g. a `mod wolf` path-alias
            # catalogue). Its contents are that submodule's own scope, not this file's top-level
            # types, so the one-primary-type-per-FILE rule does not descend into it. (A file-based
            # `mod foo;` has no `{`, isn't matched here, and stays its own scanned file.)
            skip_depth = line.count('{') - line.count('}')
            prev = line; continue
        m = TYPE_RE.match(line)
        if m: types[m.group(2)] = m.group(1); prev = line; continue
        m = ALIAS_RE.match(line)
        if m: types[m.group(1)] = "alias"; prev = line; continue
        m = IMPL_RE.match(line)
        if m:
            head = m.group(1)
            if " for " in head:
                tr, tgt = head.split(" for ", 1); impls.append((base_type(tr), base_type(tgt)))
            else:
                impls.append((None, base_type(head)))
        prev = line
    return types, impls, loc

def classify(types, impls):
    local = set(types)
    if any(tgt in local for _, tgt in impls):               return "TYPE+IMPL"
    if len([t for t in types if types[t] != "trait"]) >= 2: return "MULTI-TYPE"
    if impls:                                               return "FOREIGN-IMPL"
    return "CLEAN"

def scan(root_dir):
    os.chdir(root_dir)
    files = {}
    for root in ROOTS:
        for dp, _, fns in os.walk(root):
            for fn in fns:
                if not fn.endswith(".rs"): continue
                p = os.path.join(dp, fn)
                if EXCLUDE_DIR.search("/" + p) or fn == "build.rs": continue
                if fn.startswith(("type_", "impl_")) or fn == "mod.rs": continue
                if is_generated(p): continue          # @generated catalogue — exempt (see is_generated)
                files[p] = parse(p)
    return files

def violations(files):
    return {p for p, (t, i, _) in files.items() if classify(t, i) in ("TYPE+IMPL", "MULTI-TYPE")}

def size_flag(loc):
    if loc >= MASSIVE: return " [MASSIVE -> module + behaviour sub-split]"
    if loc >= LARGE:   return " [large]"
    return ""

def main():
    args = sys.argv[1:]
    root = next((a for a in args if not a.startswith("-")), ".")
    files = scan(os.path.abspath(root))
    cur = violations(files)

    if "--write-baseline" in args:
        with open(BASELINE, "w") as f:
            f.write("# type/impl smell-test baseline — files with known violations to work down.\n")
            f.write("# Regenerate: python3 .ci/scripts/typeimpl_audit.py --write-baseline\n")
            for p in sorted(cur): f.write(p + "\n")
        print(f"wrote {len(cur)} baselined violations to {BASELINE}")
        return 0

    if "--check" in args:
        base = set()
        if os.path.exists(BASELINE):
            base = {l.strip() for l in open(BASELINE) if l.strip() and not l.startswith("#")}
        new = sorted(cur - base)
        stale = sorted(base - cur)
        for p in stale: print(f"note: {p} is now clean — drop it from {BASELINE}")
        if new:
            print(f"\nSMELL-TEST FAILED: {len(new)} NEW type/impl violation(s) "
                  f"(a type + its impl, or >=2 types, in one file):")
            for p in new: print(f"  {p}")
            print("\nFix: split into a `foo/` module — one type per file under `types/` (no prefix) "
                  "+ `impl_<name>.rs` at the module root, re-exported from `mod.rs`. "
                  "See CLAUDE.md \"Code Organization\".")
            return 1
        print(f"smell-test OK — no new type/impl violations ({len(cur)} baselined, {len(stale)} now clean).")
        return 0

    # default: breakdown
    buckets = {"TYPE+IMPL": [], "MULTI-TYPE": [], "FOREIGN-IMPL": [], "CLEAN": []}
    for p, (t, i, loc) in files.items(): buckets[classify(t, i)].append((p, t, i, loc))
    for tag in ("TYPE+IMPL", "MULTI-TYPE", "FOREIGN-IMPL"):
        rows = sorted(buckets[tag], key=lambda r: -r[3])
        print(f"\n===== {tag} ({len(rows)}) =====")
        for p, types, impls, loc in rows:
            local = set(types)
            print(f"\n{p}  ({loc} LOC){size_flag(loc)}")
            for name, kind in types.items():
                own = sorted({tr or 'inherent' for tr, tgt in impls if tgt == name})
                print(f"    {kind:6} {name}" + (f"  <- {', '.join(own)}" if own else "   (no impl here)"))
            foreign = sorted({tgt for tr, tgt in impls if tgt not in local})
            if foreign: print(f"    (FOREIGN impls: {', '.join(foreign)})")
    print(f"\nSUMMARY: " + " ".join(f"{k}={len(v)}" for k, v in buckets.items()))
    return 0

if __name__ == "__main__":
    sys.exit(main())
