//! TMS IDL code generator CLI. Host-only; excluded from the firmware workspace.
//!
//! Subcommands:
//!   emit    (default)  parse the IDL, (re)write the generated files, print a stats summary.
//!   check              regenerate in-memory and diff against the committed files; nonzero if stale.
//!   stats              parse only; print construct counts (no writes).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use tms_idl_codegen::{generate, Generated};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("emit");
    let root = repo_root();

    let gen = match generate(&root, &overrides_path()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("tms-idl-codegen: {e}");
            return ExitCode::FAILURE;
        }
    };

    match cmd {
        "emit" => emit(&root, &gen),
        "check" => check(&root, &gen),
        "stats" => {
            print_stats(&gen);
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown subcommand `{other}` (use emit | check | stats)");
            ExitCode::FAILURE
        }
    }
}

fn emit(root: &Path, gen: &Generated) -> ExitCode {
    let writes = [
        (
            root.join("crates/lib-tms/src/generated.rs"),
            &gen.generated_tms,
        ),
        (
            root.join("crates/lib-mapping/src/wire/generated_encoders.rs"),
            &gen.generated_encoders,
        ),
        (
            root.join("crates/lib-tms/src/generated_fields.rs"),
            &gen.generated_fields,
        ),
        (
            root.join("crates/lib-tms/src/generated_qos.rs"),
            &gen.generated_qos,
        ),
        (
            root.join("crates/lib-tms/src/generated_cdr.rs"),
            &gen.generated_cdr,
        ),
        (
            root.join("crates/lib-mapping/src/wire/generated_cdr_encoders.rs"),
            &gen.generated_cdr_encoders,
        ),
        (
            root.join("docs/compliance/tms/tms-coverage.json"),
            &gen.report_json,
        ),
        (
            root.join("docs/compliance/tms/tms-coverage.md"),
            &gen.report_md,
        ),
    ];
    for (path, content) in writes {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("cannot create {}: {e}", parent.display());
                return ExitCode::FAILURE;
            }
        }
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("cannot write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
    }
    print_stats(gen);
    ExitCode::SUCCESS
}

fn check(root: &Path, gen: &Generated) -> ExitCode {
    let checks = [
        (
            root.join("crates/lib-tms/src/generated.rs"),
            &gen.generated_tms,
        ),
        (
            root.join("crates/lib-mapping/src/wire/generated_encoders.rs"),
            &gen.generated_encoders,
        ),
        (
            root.join("crates/lib-tms/src/generated_fields.rs"),
            &gen.generated_fields,
        ),
        (
            root.join("crates/lib-tms/src/generated_qos.rs"),
            &gen.generated_qos,
        ),
        (
            root.join("crates/lib-tms/src/generated_cdr.rs"),
            &gen.generated_cdr,
        ),
        (
            root.join("crates/lib-mapping/src/wire/generated_cdr_encoders.rs"),
            &gen.generated_cdr_encoders,
        ),
    ];
    let mut stale = false;
    for (path, expected) in checks {
        match std::fs::read_to_string(&path) {
            Ok(actual) if &actual == expected => {}
            Ok(_) => {
                eprintln!(
                    "STALE: {} differs from freshly generated output",
                    path.display()
                );
                stale = true;
            }
            Err(e) => {
                eprintln!("MISSING: {} ({e})", path.display());
                stale = true;
            }
        }
    }
    if stale {
        eprintln!("\nRun `just tms-codegen` to regenerate.");
        ExitCode::FAILURE
    } else {
        println!("tms-codegen-check OK — generated files are up to date.");
        print_stats(gen);
        ExitCode::SUCCESS
    }
}

fn print_stats(gen: &Generated) {
    println!(
        "parsed IDL: {} structs, {} enums, {} topic bindings ({} model types)",
        gen.struct_count,
        gen.enum_count,
        gen.topic_count,
        gen.model.types.len(),
    );
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn overrides_path() -> PathBuf {
    manifest_dir().join("tms-overrides.toml")
}

/// Repo root = two levels above this crate (`tools/tms-idl-codegen`).
fn repo_root() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
