//! Copies `memory.x` into OUT_DIR and puts it on the linker search path, then wires the linker
//! args cortex-m-rt/embassy-rp expect. For RP2350A (Cortex-M33), embassy-rp-0.10.0 auto-generates
//! the IMAGE_DEF block (via lib.rs macro), so no explicit boot2 handling is needed (unlike RP2040).
//! This firmware uses `panic-halt`, not `defmt`/`panic-probe` — no RTT/defmt transport, and UART0
//! is dedicated to the Seplos protocol wire, not logging.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");

    // link-rp.x is only generated (and needed) for RP2040. RP235x uses the default cortex-m-rt linkage.
    if env::var("CARGO_FEATURE_RP2040").is_ok() {
        println!("cargo:rustc-link-arg-bins=-Tlink-rp.x");
    }
}
