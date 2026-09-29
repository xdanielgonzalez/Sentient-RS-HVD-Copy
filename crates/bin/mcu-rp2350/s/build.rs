//! Copies `memory.x` into `OUT_DIR` and wires the linker args `cortex-m-rt` expects (mirrors
//! `ns/build.rs`, minus RP2350 IMAGE_DEF handling — `s` has no `embassy-rp` dep for that). Also
//! compiles `../veneers/veneer.c` (the frozen `sv_*` CMSE entry points, `-mcmse`) by hand via
//! `cc`'s resolved `Tool` (not `cc::Build::compile()`) using `arm-none-eabi-gcc`; links the object
//! into `rp2350-s` and requests the CMSE import library at that link step via `--cmse-implib
//! --out-implib=<path>` — the S image's link step emits a relocatable object of only the
//! NS-callable veneer trampoline symbols, which the NS image links against to resolve `sv_*` calls.
//! Frozen contract with `ns/build.rs` (do not rename either side alone): artifact path
//! `bin/mcu-rp2350/target/veneer-implib/libveneer_implib.o` (workspace-relative, survives profile
//! changes), env var `SENTIENT_VENEER_IMPLIB` — `ns/build.rs` reads it, falling back to that path.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
    fs::write(out.join("memory.x"), include_bytes!("memory.x")).expect("write memory.x to OUT_DIR");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");

    // --- compile the frozen veneer C shim, -mcmse ---
    // `cc`'s built-in compiler-inference table has no entry (no `arm-none-eabi-gcc` guess) for the
    // bare-metal thumbv8m.main-none-eabihf triple, so the COMPILER is named explicitly. Its
    // per-target FLAG table, however, DOES recognize this triple (`cc` reads `TARGET` from the
    // cargo-set build-script environment independent of `.compiler()`) and already adds
    // `-mthumb -march=armv8-m.main -mfloat-abi=hard -mfpu=fpv5-sp-d16` on its own — confirmed by
    // reading `cc`'s source (`target.full_arch.starts_with("thumbv8m.main")` /
    // `target.abi == "eabihf"` branches) after this build's own `-mcpu=cortex-m33` (which
    // implies the same architecture) collided with `cc`'s auto-added `-march`, producing a real
    // (if non-fatal) `cc1` warning: `switch '-mcpu=cortex-m33' conflicts with switch
    // '-march=armv8-m.main+fp'`. Only `-mcmse` (CMSE support, which `cc` has no target-inference
    // rule for at all) and `-std=c11` are added manually here.
    //
    // NOT using `cc::Build::compile()` (which would `ar`-archive the object into `libveneer.a` and
    // link it via `-lveneer`): confirmed by actually building this way first that `rust-lld`
    // rejects the ARCHIVED object's `cmse_nonsecure_entry` veneers — `cmse special symbol
    // '__acle_se_sv_seal' is not a Thumb function definition` (etc., for all four veneers) — even
    // though `arm-none-eabi-readelf` on the freshly-compiled (pre-archive) `.o` shows those exact
    // symbols correctly typed `FUNC` with the Thumb bit set. This is an archive/direct-object
    // linking difference in how `rust-lld`'s CMSE-veneer scan finds these symbols, not a
    // miscompilation — worked around by compiling straight to a `.o` and linking THAT object
    // directly (no archive, no `-l`), which the next block does by hand via `cc`'s resolved
    // `Tool` (`get_compiler()`) rather than `Build::compile()`.
    let veneer_obj = out.join("veneer.o");
    let compiler = cc::Build::new()
        .compiler("arm-none-eabi-gcc")
        .include("../veneers")
        .flag("-mcmse")
        .flag("-std=c11")
        .warnings(true)
        .get_compiler();
    let status = compiler
        .to_command()
        .arg("-c")
        .arg("../veneers/veneer.c")
        .arg("-o")
        .arg(&veneer_obj)
        .status()
        .expect("run arm-none-eabi-gcc to compile veneer.c");
    assert!(
        status.success(),
        "arm-none-eabi-gcc failed to compile veneer.c"
    );

    // `cmse_check_address_range` (which `veneer.c` calls) is not a compiler-inlined intrinsic in
    // this GCC — confirmed by the first attempt at this link step failing with `undefined symbol:
    // cmse_check_address_range`. It is a real function GCC's ARM CMSE support ships in a
    // multilib-specific `libgcc.a` (verified: `arm-none-eabi-nm` on the file below lists
    // `T cmse_check_address_range`). `rustc`'s own link step never pulls this in on its own (Rust
    // links `compiler_builtins`, not the ARM toolchain's `libgcc`), so it must be added by hand,
    // resolved to the EXACT multilib variant matching this object's compile flags (`-mthumb
    // -march=armv8-m.main -mfloat-abi=hard -mfpu=fpv5-sp-d16` — the same flags `cc` auto-selected
    // above; a mismatched multilib `libgcc.a` would either fail to resolve the symbol or, worse,
    // silently link an ABI-incompatible one) via `gcc -print-libgcc-file-name` rather than a
    // hand-guessed path.
    let libgcc = compiler
        .to_command()
        .arg("-mthumb")
        .arg("-march=armv8-m.main")
        .arg("-mfloat-abi=hard")
        .arg("-mfpu=fpv5-sp-d16")
        .arg("-print-libgcc-file-name")
        .output()
        .expect("run arm-none-eabi-gcc -print-libgcc-file-name");
    assert!(
        libgcc.status.success(),
        "arm-none-eabi-gcc -print-libgcc-file-name failed"
    );
    let libgcc_path = String::from_utf8(libgcc.stdout)
        .expect("libgcc path is UTF-8")
        .trim()
        .to_string();

    println!("cargo:rustc-link-arg-bins={}", veneer_obj.display());
    println!("cargo:rustc-link-arg-bins={}", libgcc_path);
    println!("cargo:rerun-if-changed=../veneers/veneer.c");
    println!("cargo:rerun-if-changed=../veneers/veneer.h");

    // `-mcmse` places every `cmse_nonsecure_entry` veneer's SG (Secure Gateway) trampoline into a
    // `.gnu.sgstubs` output section — but plain `cortex-m-rt`/`link.x` (written for non-TrustZone
    // firmware) has no rule placing that section anywhere, which the first attempt at this link
    // step surfaced directly: `no address assigned to the veneers output section .gnu.sgstubs`.
    // Fixed with `--section-start`, a generic GNU-ld/lld flag that pins any named output section to
    // a fixed address, rather than a `memory.x` `SECTIONS`/`INSERT` block (`ns/memory.x`'s own
    // `.start_block` pattern) — simpler here since this section needs no size accounting relative
    // to neighboring sections, only a fixed start address matching the NSC subregion `sau.rs`
    // configures. NOTE: this address is a literal here, not computed from `mem_layout::NSC_BASE`
    // (build scripts compile and run standalone, before `src/` exists as a linkable crate) — it
    // MUST stay in lockstep with `mem_layout::NSC_BASE` and `memory.x`'s NSC comment; all three
    // name the same `0x1001F000` and are cross-referenced in each other's doc comments.
    println!("cargo:rustc-link-arg-bins=--section-start=.gnu.sgstubs=0x1001F000");

    // --- emit the CMSE import library at the rp2350-s binary's own link step ---
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by cargo"));
    let implib_dir = manifest_dir.join("..").join("target").join("veneer-implib");
    fs::create_dir_all(&implib_dir).expect("create bin/mcu-rp2350/target/veneer-implib/");
    let implib_path = implib_dir.join("libveneer_implib.o");

    // NOTE: no `-Wl,` prefix — cargo's `rustc-link-arg-bins` passes these straight to whatever
    // linker cargo actually invokes for this target, which (confirmed by the link error hit while
    // developing this build.rs) is `rust-lld` directly in `-flavor gnu` (ld.lld) mode, NOT a gcc
    // driver — `-Wl,` stripping is a gcc-frontend convention that does not apply here; passing it
    // through made lld see the literal string `-Wl,--cmse-implib` and reject it as unknown.
    println!("cargo:rustc-link-arg-bins=--cmse-implib");
    println!(
        "cargo:rustc-link-arg-bins=--out-implib={}",
        implib_path.display()
    );
    println!(
        "cargo:warning=rp2350-s: CMSE import library will be written to {} (frozen path; see \
         SENTIENT_VENEER_IMPLIB in build.rs's doc comment)",
        implib_path.display()
    );
}
