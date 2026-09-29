# wolfCrypt backend (`feature = "wolfcrypt"`)

`wolfcrypt` is the **concrete cryptographic backend** behind the `lib-security` provider seam
(the boundary described in [`security-integration-map.md`](integration-map.md) §2.3). It is
**OFF by default** and lives in `lib-security/src/backends/wolfcrypt.rs`, gated
`#[cfg(feature = "wolfcrypt")]`.

## What it provides

It implements the seam traits by delegating to wolfCrypt through its **RustCrypto trait surface**
(`rand_core`, `digest::Digest`, `signature::Verifier`) rather than wolfCrypt-internal types:

| Seam trait | Impl | Notes |
|---|---|---|
| `Rng` | `WolfRng` | wolfCrypt CSPRNG via `rand_core`; a draw failure ⇒ `EntropyFailure` (never silent weak output). |
| `Hasher` | `WolfHasher` | SHA-256/384/512 via `digest::Digest`; `finish` is bounded (`BufferTooSmall` if `out` is short). |
| `Verifier` | `WolfVerifier` | Ed25519 / ECDSA-P256 / ECDSA-P384 / RSA-PKCS1v15 via `signature::Verifier`. Fail-closed: `Ok(())` only on a proven-good signature; bad sig ⇒ `VerificationFailed`, parse failure ⇒ `Malformed`. |
| `KeyStore` | `WolfKeyStore` | `custody()` = **`SoftwareMemory`** (honesty accessor — software keys are NOT hardware-backed). `generate`/`import_private`/`export_public`/`sign` return `NotImplemented` (fail closed) until a key arena is wired. |

## Composition with the posture toggles

- **`fips`** selects wolfCrypt **FIPS mode** — the FIPS 140-3 validated wolfCrypt module, approved
  algorithms only, power-on KATs. The approved-algorithm gate (`Provider::check_approved`) and the
  self-test hook (`provider::run_power_on_self_tests`) stay in the seam; wolfCrypt supplies the
  validated primitives underneath. Building the FIPS tier requires a **FIPS build of wolfSSL**.
- **`trustzone`** is orthogonal. wolfCrypt software keys are software custody; hardware custody
  needs the secure-world tier, which this backend does not provide.

Enabling `wolfcrypt` does **not** change the fail-closed default behavior — `Provider` does not yet
route to the backend; the feature only makes the concrete implementations available.

## Build requirement — a SEPARATE BUILD LEDGER

`wolfssl-wolfcrypt` 2.0 **does not vendor wolfSSL** — its `build.rs` runs `bindgen` over the wolfSSL
headers and links a **prebuilt wolfSSL library**, located via `WOLFSSL_PREFIX` (`{prefix}/include` +
`{prefix}/lib`), an in-tree `wrapper/rust` checkout, or the system loader path. (This is unlike the
separate `wolfssl` TLS-wrapper crate — used by `echelon-mesh` via `wolfssl-rs` — which *does* vendor
wolfSSL through `wolfssl-src`. The two crates are different: this backend binds the wolfCrypt
**RustCrypto trait surface**, not the TLS wrapper.) Because a prebuilt wolfSSL is not present in the
default CI/dev environment, **this feature does not link there** — exactly analogous to the Zephyr
`west` target, which is excluded from the default workspace build. The default
`cargo build --workspace` leaves `wolfcrypt` off and compiles nothing from it.

CI builds a linkable wolfSSL **5.9.1** from the `High-Velocity-Decisions/wolfssl-fork`
(`./autogen.sh && ./configure --enable-all && make && make install` into `/usr/local`), then points
the crate at it with `WOLFSSL_PREFIX=/usr/local`. See `.github/workflows/wolfcrypt.yml` (a
`push`/`workflow_dispatch` job on the `wolfcrypt-std-bringup` branch — **NOT** part of the main CI
gate). This bring-up is **std / non-FIPS tier only**; the fork also carries `fips-check.sh` for a
later FIPS ledger.

Build and test on a wolfSSL-equipped host (with a prebuilt wolfSSL 5.9.1+ at `$WOLFSSL_PREFIX`):

```bash
export WOLFSSL_PREFIX=/usr/local                              # {prefix}/include + {prefix}/lib
cargo build   -p lib-security --features wolfcrypt
cargo test    -p lib-security --features wolfcrypt        # runs the SHA-256 KAT + RNG smoke
cargo build   -p lib-security --features "wolfcrypt fips" # FIPS-mode wolfCrypt (needs a FIPS build)
```

## Confirmed API paths (`// WOLFSSL-API: confirmed`)

The concrete `wolfssl_wolfcrypt::...` type paths were **confirmed by reading the `wolfssl-wolfcrypt`
2.0.0 crate source directly** (`random.rs`, `sha.rs`/`sha_digest.rs`, `ecdsa.rs`, `ed25519.rs`,
`rsa_pkcs1v15.rs`) — not docs.rs (which cannot build the crate without a linkable wolfSSL). They are
collected in the private `wolf` module of `wolfcrypt.rs`, each marked `// WOLFSSL-API: confirmed`:

- `wolf::Rng` = `random::RNG`. Constructed via `RNG::new()`; **filled via the native fallible
  `RNG::generate_block` (`Result<(), i32>`)**, NOT the `rand_core::TryRng` adapter — that adapter's
  `Error = Infallible` and its `try_fill_bytes` *panics* on a wolfCrypt draw failure, which would
  violate this crate's fail-closed contract. A DRBG failure maps to `EntropyFailure`.
- `wolf::Sha256/384/512` = `sha::{SHA256,SHA384,SHA512}` with the `digest` trait impls in
  `sha_digest.rs`. These do **not** derive `Clone`, so the non-consuming finalize uses
  `Digest::finalize_reset` (`FixedOutputReset`), not `clone().finalize()`.
- `wolf::Ed25519Key` / `Ed25519Sig` = `ed25519::{VerifyingKey, Signature}` (`impl Verifier`).
- `wolf::P256Key` / `P256Sig`, `wolf::P384Key` / `P384Sig` = `ecdsa::{P256VerifyingKey,
  P256Signature, P384VerifyingKey, P384Signature}` — macro-generated per-curve types at the module
  root (NOT `ecdsa::p256::…`).
- `wolf::RsaKey` / `RsaSig` = `rsa_pkcs1v15::{VerifyingKey<Sha256, 256>, Signature<256>}` — the
  types are **generic** (`VerifyingKey<H: Hash, const N: usize>`), narrowed here to the concrete
  RSA-2048 + SHA-256 instantiation. The key has **no** `TryFrom<&[u8]>` impl (unlike the ECDSA/Ed25519
  keys); it is parsed from a DER public key via `from_public_der`. Semantic narrowing: an RSA key
  whose modulus is not exactly 2048 bits is rejected as `Malformed`.
- Key/signature parse convention: `parse_key`/`parse_sig` use `TryFrom<&[u8]>` for the ECDSA/Ed25519
  keys and all signatures; RSA keys are the sole exception (DER via `from_public_der`).

## Trait-version pins

The optional `digest` / `signature` / `rand_core` deps in `Cargo.toml` are pinned to the versions
`wolfssl-wolfcrypt` 2.0.0 actually depends on (confirmed by reading its `Cargo.toml`): **`digest` 0.11
(with the `block-api` feature), `signature` 2.2, `rand_core` 0.10**. These MUST match wolfCrypt's
trait crates or the delegation will not typecheck.

**The backend does not `use` the `rand_core` adapter** — `WolfRng` binds wolfCrypt's native
`generate_block` instead (see `wolf::Rng` above), because that adapter's `Error = Infallible` and its
`try_fill_bytes` panics on a DRBG failure rather than returning `Err`. `rand_core` is nonetheless kept
as a dependency (and the wolfssl-wolfcrypt `rand_core` feature requested) so the committed
`Cargo.lock` dependency graph is **unchanged from `main`**: this bring-up is a separate, non-gating
ledger and must not perturb the shared lock that the main gate's cargo-audit / cargo-deny evaluate.
For the same reason, the `wolfcrypt.yml` job runs `cargo build/test` **without `--locked`** so the
`--features wolfcrypt` build-time closure (bindgen, clang-sys, …) resolves ephemerally and never
enters the committed lock.
