# `lib-security` — the crypto/auth provider seam

Auth tokens, certificates, and cryptography behind one provider seam, with two posture toggles
(**FIPS** and **TrustZone**) that are honored by construction rather than by discipline. This crate
**defines and tests the seam and its boundary rules**; concrete backends are wired in through the
enforced pipeline.

> For where each integration point plugs into the running system, see
> [`../security-integration-map.md`](../../security/integration-map.md); for the overall posture and
> boot-integrity story, [`../security.md`](../../security/README.md); for the wolfCrypt backend,
> [`../wolfcrypt-backend.md`](../../security/wolfcrypt-backend.md). This module doc covers the crate's *code
> surface*.

## `no_std` status

`#![cfg_attr(not(feature = "std"), no_std)]` + `#![forbid(unsafe_code)]`. The core (rng/hash/sign
over slices + opaque handles) needs no allocator (the MCU forward path); host-only X.509 chain
verification is gated behind `feature = "std"`. Secret material is zeroized on drop (`zeroize`,
`no_std`, no-alloc).

### Feature tiers (FIPS × TrustZone)

| Feature | Meaning |
|---|---|
| `default = ["std"]` | Host build at the **software** tier — standard crypto, keys in normal-world memory (the *lowest* tier). |
| `fips` | Cryptography runs inside a FIPS 140-3 validated module; power-on self-tests (KATs) run at `Provider::new`; only approved algorithms are reachable (a non-approved primitive is `SecurityError::NotApproved`, never a silent fallback). |
| `trustzone` | Private keys live in the secure world (TrustZone / TF-M / TPM / secure element), referenced by opaque handles only; key bytes never return to normal world. |
| `std` | std/alloc surface for host-side token/cert verification (X.509, JWT). |
| `wolfcrypt` | The concrete wolfCrypt backend — **OFF by default, a separate build ledger** requiring system wolfSSL 5.9.1+ at link time. See [`../wolfcrypt-backend.md`](../../security/wolfcrypt-backend.md). |

The tiers are exercised as a matrix by `just sec-matrix` (see [`../usage.md`](../../deployment/usage.md)).

## Design invariants (encoded in the types)

- **Fail closed.** Every verify returns `Result`; `Ok` means *proven*, any `Err` means *deny*.
  Unbound backends return `SecurityError::NotImplemented`, never a fake success.
- **No private-key export.** `KeyStore` signs *with* a handle and exports only public keys — there
  is deliberately no `export_private`, so the type system forbids leaking key material.
- **Honest custody.** `KeyStore::custody()` is mandatory: a software store must report
  `KeyCustody::SoftwareMemory` so it can never be mistaken for hardware-bound custody.

## Key public types

| Item | What it is |
|---|---|
| `Provider` | The active security provider: holds the resolved `SecurityProfile`, gates algorithm selection (`check_approved`), and is the construction point where a FIPS backend runs KATs (`Provider::new`). |
| `SecurityProfile` (`profile`) | Runtime-readable posture fixed at build time: `active()`, `fips_enabled()`, `secure_world_enabled()`, `custody_warning()`, `tier_slug()` (e.g. `fips+trustzone`, `standard+software`). |
| `KeyCustody` / `CryptoValidation` | `SecureWorld` vs `SoftwareMemory`; `Fips140_3` vs `Standard`. |
| `traits::KeyStore` | Opaque key custody: `custody`, `generate`, `import_private`, `export_public`, `sign` — **no** private export. |
| `traits::KeyHandle` | Opaque, `Copy` reference into a store's custody domain — carries no key material. |
| `traits::Algorithm` / `Digest` | Signature/digest identifiers; `Algorithm::is_fips_approved()` is the single source of truth for the FIPS gate (Ed25519 is the one non-approved variant). |
| `traits::Rng` / `Hasher` / `Verifier` | Fail-closed crypto primitive traits over slices. |
| `traits::TokenVerifier` / `TokenClaims` / `TokenPolicy` | Fail-closed auth-token (JWT-style) verification into bounded claims. |
| `traits::CertVerifier` | X.509 chain verification (host-only, `feature = "std"`). |
| `SecurityError` / `Result` (`error`) | The fail-closed error type (`NotApproved`, `NotImplemented`, `SelfTestFailed`, `EntropyFailure`, …). |
| `backends::wolfcrypt` | The wolfCrypt-backed `WolfRng`/`WolfHasher`/`WolfVerifier`/`WolfKeyStore` (feature `wolfcrypt`). |

The concrete crypto operations in `provider` are currently **fail-closed stubs**
(`NotImplemented`) — the seam compiles and can be integrated without any code mistaking an unbound
backend for a working one. `Provider` does not yet route to the wolfCrypt backend even when
`wolfcrypt` is enabled; the pure-policy parts (profile, approved-algorithm gate, KAT hook) are
correct and testable now.

## Dependencies and dependents

- **Depends on:** `heapless`, `zeroize`; optionally `wolfssl-wolfcrypt` + `digest` + `signature` +
  `rand_core` (feature `wolfcrypt`).
- **Depended on by:** `lib-mqtt` (the broker's CONNECT authorizer routes through `TokenVerifier` —
  see [`lib-mqtt`](./lib-mqtt.md)), so this seam **is** on the shipping RPi binary's dependency
  graph (`bin-rpi` → `lib-mqtt` → `lib-security`). Also `lib-link` (optional, feature `tls` —
  `tls_identity::KeyStoreTlsIdentity`, MCU-only path, see [`lib-link`](./lib-link.md)). The crypto
  *operations* behind the seam are still fail-closed stubs pending a production backend (see
  above); the dependency edges themselves are real, not aspirational. The crypto handoff boundary
  is additionally represented in the owned-network pipeline by `lib_channel::CryptoUnit` (which
  carries an *opaque* key context, never key bytes).

## Testing

- `tests/matrix.rs` — an `rstest` fixture/parameterized matrix over tiers × algorithms ×
  operations (the seam's whole point is that every tier × algorithm combination behaves correctly).
- One in-crate unit test asserts `SecurityProfile::active()` reflects the compile-time toggles.
- `just mutants` runs mutation testing on this crate (alongside `lib-parse`).
- CI: the tier matrix is `.github/workflows/` `_test.yml` via `just sec-matrix`; the wolfCrypt
  ledger is `.github/workflows/wolfcrypt.yml`.
