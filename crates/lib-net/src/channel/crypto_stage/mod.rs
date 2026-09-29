//! [`CryptoStage`]: the real seal/open leg of the `[crypto/TLS]` pipeline placeholder (see
//! `crate::channel`'s module doc), riding `lib-security`'s [`lib_security::SessionCipher`] seam.
//!
//! `CryptoStage<C>` is a thin [`crate::channel::stage::Stage`] adapter: it does no cryptography
//! itself, only maps a [`crate::channel::message::CryptoUnit`]'s `op`/`context`/`data` onto
//! `SessionCipher::seal`/`open` calls (in place, on the boundary message's own bounded buffer) and
//! reports the cipher's [`lib_security::SecurityError`] back through a stage-local
//! [`CryptoStageError`]. It never sees key bytes — only the opaque `KeyHandle` derived from
//! `CryptoUnit::context` — so swapping the cipher backend (software AES-GCM today, wolfCrypt or a
//! secure-world store later) never touches this module.
//!
//! Feature `crypto` (no_std-clean: pulls in `lib-security`'s default `software` backend only, not
//! its `std` feature — independent of the `tls`/`tls_identity` record-layer modules, which are
//! host/std-only).

mod prelude;
mod types;

mod impl_crypto_stage;

pub use types::*;
