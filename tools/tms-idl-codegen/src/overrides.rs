//! The `tms-overrides.toml` loader — the tiny honest hand layer (bound narrowings + MCU encoder
//! subset). Everything else is derived from the IDL.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Overrides {
    /// IDL const name -> narrowed value (SRAM budget). Narrowing a const narrows every sequence /
    /// string bound that resolves through it.
    #[serde(default)]
    pub bounds: HashMap<String, u64>,
    #[serde(default)]
    pub subset: Subset,
    #[serde(default)]
    pub typedefs: Typedefs,
    /// Full IDL enumerator (wire spelling) -> the Rust variant IDENT to use. For the handful of
    /// golden variant names that are hand-curated semantic labels, not a mechanical de-prefix of the
    /// wire token (e.g. `FMI_HIGH_MOST_SEVERE_HIGH` -> `AboveNormalMostSevere`). The `#[serde(rename)]`
    /// (the wire token) is UNCHANGED — this touches only the Rust identifier, so wire bytes are
    /// identical. Reproduces golden's exact spelling so consumer call sites do not churn.
    #[serde(default)]
    pub variant_idents: HashMap<String, String>,
}

/// The KEPT primitive typedefs (`Identity`, `String32`, …) emitted into `lib-tms` as named `pub type`
/// aliases. Only the SELECTION is hand-authored here — the alias RHS is resolved from the IDL, so it
/// cannot drift. The golden hand-written schema exposed exactly these as named aliases (the rest of
/// the IDL's ~50 typedefs are `*Sequence` helpers golden inlined and never named).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Typedefs {
    /// IDL typedef names to emit as `pub type` aliases (in this order).
    #[serde(default)]
    pub emit: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Subset {
    /// The `@topic` roots that get a flat MCU encoder emitted.
    #[serde(default)]
    pub mcu_encoder_topics: Vec<String>,
    /// The `@topic` roots whose field CHAINS (Src accessor traits + `Field` ZSTs + `Chain` node
    /// lists) are emitted into `lib-tms/src/generated_fields.rs` (transitive closure of referenced
    /// struct types is computed automatically; enums emit via `wire_str()` and need no module).
    #[serde(default)]
    pub chain_topics: Vec<String>,
    /// The `@topic` roots whose XCDR2 CDR chains (`<T>CdrSrc` accessor traits + `CdrField` ZSTs +
    /// `CdrChain` node lists) are emitted into `lib-tms/src/generated_cdr.rs` (transitive closure of
    /// referenced struct types computed automatically). Every published topic + the two request
    /// types — the DDS transport carries these as XCDR2 (parallel to the JSON `chain_topics`).
    #[serde(default)]
    pub cdr_topics: Vec<String>,
}


impl Overrides {
    pub fn load(path: &str) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read overrides {path}: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("cannot parse overrides {path}: {e}"))
    }
}
