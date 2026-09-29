# lib-security — Integration Map & Boundary Design

**Project:** HVD → Sentient METEOR comms module (Seplos BMS → TMS/MIL-STD-3071 → MQTT).
**Purpose of this doc:** reconnaissance + design guidance for a new `lib-security` crate that abstracts
auth tokens, certificates, and cryptography behind two compile-time toggles — **FIPS** (FIPS 140-3 validated
module vs standard crypto) and **TrustZone/secure-computing** (hardware secure element / ARM TrustZone /
TPM-backed key storage vs software key storage).
**Status:** MAP ONLY. No crate is implemented here. This is a placement + boundary + test plan.
**Companion:** [`docs/security.md`](README.md) — the Scope-E posture doc. This map is *subordinate* to it and
must not contradict it. Where security.md says "key protection, NOT attestation" on RPi (§2, §3), this map
inherits that honesty constraint verbatim.

---

## 0. Current crypto reality (what exists today, cited)

There is **no cryptography in the Rust tree today.** Every security primitive currently lives *outside* the
Rust binary:

- **MQTT TLS to cloud:** entirely in mosquitto — `bridge_cafile`/`bridge_insecure false` are
  emitted by `bin-rpi/src/mosquitto_bridge.rs`'s `render_bridge()` into the mosquitto
  fragment file (`distro/deploy/mosquitto.conf` itself only has a static `include_dir`, no bridge block —
  mosquitto 2.x can't expand `${...}` env vars, so nothing is templated in a static file anymore).
  The Rust binary never opens a TLS socket to the cloud; it only *renders config for mosquitto*.
- **Embedded broker:** unauthenticated plaintext — `lib-mqtt/src/broker.rs:71-72`
  (`auth: None`, `external_auth: None`), `:83` (`tls: None`), loopback default `:35`
  (`host: "127.0.0.1", port: 1883`).
- **Binary integrity:** external `openssl cms` shell-out — `distro/deploy/verify-binary.sh`
  (`openssl cms -verify -binary -content "$BIN" -in "$SIG" -inform DER -CAfile "$CAFILE"`, an X.509
  CMS detached signature against the self-signed CA), gated by systemd `ExecStartPre`
  (`distro/deploy/bin-rpi.service:29`). No in-binary verification.
- **Config carries no password field:** the `Config` struct has device/serial/poll/broker/publish
  plus an optional `[cloud]` section (`bin-rpi/src/config.rs:14`, `:34` `CloudConfig`) —
  host/port/username/ca_file/remote_clientid/topic/qos only, **no password field**. Module doc:
  "Nothing (endpoints, credentials, ports) is hardcoded" (`config.rs:4-6`). The password is read
  only as the `CLOUD_MQTT_PASS` env var, only by the `--emit-mosquitto-bridge` render path
  (`mosquitto_bridge.rs`'s `read_password_secret()`), and never stored on a struct.
- **Zephyr MQTT client TLS:** enabled in Kconfig — `bin/zephyr/prj.conf:21`
  (`CONFIG_MQTT_LIB_TLS=y`) — but the client is a **weak stub returning NULL**
  (`bin/zephyr/src/zephyr_shim.c:86-89`, `sentient_mqtt_client`), so no TLS creds are wired.
  The only randomness in-tree is `sys_rand32_get()` for an MQTT **message-id** (`zephyr_shim.c:106`) —
  **not** a security primitive; do not conflate it with the DRBG requirement.
- **Lints that constrain the new crate:** `#![forbid(unsafe_code)]` on the four core crates
  (`lib-mqtt/src/lib.rs:10`, `lib-io/src/lib.rs:11`, `lib-mapping/src/lib.rs:12`,
  `lib-parse/src/lib.rs:12`); `#![deny(unsafe_code)]` + one scoped `#[allow(unsafe_code)] pub mod zephyr;`
  in the app (`impl-app/src/lib.rs:10`, `:22-24`); workspace clippy `unwrap_used/expect_used/panic = "deny"`
  (`Cargo.toml:52-55`); release `panic = "abort"` (`Cargo.toml:61`).

**Design consequence:** `lib-security` is greenfield. It does not refactor existing crypto — there is none
to refactor. It must slot in *beside* the mosquitto/`openssl cms` posture, not replace it wholesale, and every place
it plugs in is currently a gap or an external tool. This is a green-field opportunity to get the toggles right,
and a trap to oversell (see §5).

---

## 1. Integration points — WHERE crypto/auth/cert handling must plug in

Ranked table first, then detail. "Tier" = realistic (must be real for ship) vs speculative (documented stub OK).

| # | Integration point | File:line anchor | Primitive needed | Toggle sensitivity | Reality |
|---|---|---|---|---|---|
| 1 | Cloud bridge TLS client key (mutual TLS) | `mosquitto_bridge.rs`'s `render_bridge()` (renders `bridge_cafile`/`bridge_insecure`); `mosquitto.conf:59-60` (commented certfile/keyfile template) | X.509 client cert + non-exportable private key; server-CA validation | **TrustZone** (key custody: TPM vs file) | realistic |
| 2 | Zephyr MQTT client TLS + DRBG | `prj.conf:21`, `:22-29`; `zephyr_shim.c:78-89` | TLS 1.2/1.3 stack, CA store, client cert/key, **hardware TRNG-seeded DRBG** | **FIPS** (DRBG) + **TrustZone** (key in secure element) | realistic |
| 3 | Binary / firmware signature verification | `verify-binary.sh`; `bin-rpi.service:29`; security.md §2, §5 | X.509 CMS (RSA/ECDSA) app binary / RSA-2048 (RPi boot) / ECDSA-P256 (MCUboot) signature verify | **FIPS** (algorithm choice must stay approved — see §2) | realistic |
| 4 | Embedded broker auth+TLS *iff bind widens* | `broker.rs:71-72`, `:83`, `:35`; `config.rs:233-248` | TLS server cert/key + listener auth (username/token) | **FIPS** + **TrustZone** | conditional (gated by security.md §1.2 rule) |
| 5 | At-rest LUKS data-key sealing | security.md §3 use 1 | Key sealed to TPM (clevis/tpm2 pin), not a Rust primitive per se | **TrustZone** | realistic (with TPM) |
| 6 | Device identity key / provisioning | security.md §3 use 3 | Keypair generation + non-exportable storage; enrollment CSR | **FIPS** (keygen) + **TrustZone** (custody) | realistic (with TPM) |
| 7 | Entropy / DRBG source | `prj.conf:22-29` (Zephyr TRNG); host `getrandom` | CSPRNG with health tests; FIPS DRBG (SP 800-90A/B) | **FIPS** (primary) | realistic |
| 8 | Auth tokens (bearer/JWT/device) | *(none today)* — cloud auth is mosquitto user/pass, `username` from `[cloud]` config (`config.rs:34`) + password from `CLOUD_MQTT_PASS` | JWT/PASETO verify: signature, exp, aud, iss | **FIPS** (verify alg) | **mostly speculative** — see §1.8 |
| 9 | microSD logger at-rest (Slice 6) | *(not built)* — security.md notes overlay/mutable state | Authenticated encryption (AES-GCM) if it holds sensitive data | **FIPS** + **TrustZone** | speculative (feature not built) |

### 1.1 Cloud bridge TLS client key — the highest-value real integration
Today the cloud relay authenticates with **username/password over server-authenticated TLS** —
username/CA path from the typed `[cloud]` config, password from the `CLOUD_MQTT_PASS` secret,
rendered into the bridge fragment by `mosquitto_bridge.rs`'s `render_bridge()` (`remote_username`,
`bridge_cafile`, `bridge_insecure false`). The hardening path (security.md §3 use 2, §4) is
**mutual TLS** with a client cert whose private key is **non-exportable in the TPM via
`tpm2-pkcs11`**.
- The commented Greengrass bridge already shows the client-cert shape: `distro/deploy/mosquitto.conf:59-60`
  (`bridge_certfile`, `bridge_keyfile`).
- **Where `lib-security` plugs in:** *not* into mosquitto's TLS (that stays C/mosquitto). The crate's role
  here is (a) a `KeyStore` that can **provision/reference** the client key as a PKCS#11 handle, and (b) a
  `CertVerifier`/enrollment helper that produces the CSR and validates the issued chain. The actual TLS
  handshake remains mosquitto's. **Do not move the cloud TLS handshake into Rust** — that would duplicate a
  hardened, audited mosquitto path and widen the attack surface for no gain. Map the crate to *key custody +
  cert lifecycle*, not to being a TLS terminator on the cloud path.

### 1.2 Zephyr MQTT client TLS + DRBG — the real embedded crypto surface
On the MCU forward path the Rust/C boundary *is* the TLS client. `CONFIG_MQTT_LIB_TLS=y` (`prj.conf:21`) but
`sentient_mqtt_client()` is a weak NULL stub (`zephyr_shim.c:86-89`) and TLS creds are "elided — wire it during
T&V" (`zephyr_shim.c:5`, `:78`). Two crypto obligations here:
- **DRBG seeding:** `prj.conf:22-29` is explicit that the SoC hardware TRNG (`CONFIG_ENTROPY_*_RNG`) is the ship
  gate and `CONFIG_TEST_RANDOM_GENERATOR` must never ship. This is the **FIPS DRBG entropy source** on the MCU.
- **Cert/key + CA store:** the client cert, its key, and the trust anchor must be provisioned. TrustZone-capable
  MCUs (STM32 with TZ, nRF5340 with CryptoCell/KMU) can hold the key in the secure world.
- **Where `lib-security` plugs in:** it defines the *trait seam* (`CryptoProvider`, `KeyStore`,
  `CertVerifier`) that the Zephyr platform (`bin-zephyr-platform`, `bin/zephyr/platform/`) implements over the C shim. The FFI to
  the secure world is the *one* place `unsafe` is permitted (mirrors the existing `zephyr` module carve-out,
  `impl-app/src/lib.rs:22-24`).

### 1.3 Binary / firmware signature verification
Three distinct verify paths — **this is where the FIPS toggle bites:**
- **App binary (today):** external `openssl cms -verify` = **X.509 CMS** over an RSA/ECDSA signer cert chaining
  to our self-signed CA (`verify-binary.sh`). Both RSA (2048+) and ECDSA (P-256/P-384) are FIPS 140-3 approved,
  so the *current* app-binary path is FIPS-friendly as long as the signer cert stays on an approved algorithm —
  see §2. An in-binary attestation/verify path is **optional** and mostly redundant with the systemd
  `ExecStartPre` gate (`bin-rpi.service:29`); if added, it belongs as a `SignatureVerifier` trait so FIPS
  builds can require an approved algorithm (ECDSA-P256/RSA-PKCS1v15).
- **RPi signed boot (security.md §2 Tier 2):** RSA-2048, OTP-fused. Not a Rust primitive — bootloader-level.
- **MCUboot (security.md §5):** ECDSA-P256 or RSA-2048 via `imgtool`. Not a Rust primitive — bootloader-level.
- **Where `lib-security` plugs in:** only the *optional in-app* verify path (defense-in-depth for
  update payloads, config bundles, or a self-check). Boot/firmware signing stays in the boot chain. Do **not**
  present an in-app Rust verify as equivalent to verified boot (security.md §2 is emphatic about this).

### 1.4 Embedded broker auth + TLS — conditional, gated by an existing rule
The rumqttd broker is plaintext/no-auth (`broker.rs:71-72`, `:83`) and safe **only** while loopback-bound
(`broker.rs:35`, security.md §1.2). The config *permits* a wide bind but the code does *not* gate it
(`config.rs:233-248` proves `host = "0.0.0.0"` is accepted). security.md §1.2 states the hard rule:
**widening `broker.host` beyond loopback requires adding auth + TLS first**, and flags a future startup
assertion (`host != loopback ⇒ require tls+auth`).
- **Where `lib-security` plugs in:** (a) supply the rumqttd `tls` config (server cert/key via `KeyStore`)
  and a listener `auth` (token/credential `TokenVerifier`) when a LAN subscriber must connect; (b) provide the
  **startup assertion** as a reusable guard so the operational rule becomes a code-enforced invariant.
  This is the cleanest single win the crate can add to the *existing* Rust surface.

### 1.5 At-rest — LUKS data-partition key
security.md §3 use 1: seal the LUKS unlock key to the TPM (clevis/tpm2 pin) so an exfiltrated SD card won't
decrypt. This is an OS/initramfs concern, **not** a Rust primitive. `lib-security`'s only role is
*documentation/handle reference* — it should NOT reimplement disk encryption. Map it as: the `KeyStore` trait
may expose a "sealed blob" concept, but LUKS sealing itself stays in clevis. Avoid scope creep here.

### 1.6 Device identity key / provisioning
security.md §3 use 3: a TPM-resident identity keypair for enrollment/rotation, private key never on the
filesystem. **This is the strongest fit for the `KeyStore` + provisioning surface.** `lib-security` owns:
keygen-in-secure-element, CSR emission, handle storage, rotation. Under TrustZone=off this becomes a
software keypair — which is a **different security tier** and must be labelled as such (§5).

### 1.7 Entropy / DRBG
- **Host (RPi):** `getrandom`/`/dev/urandom`; FIPS build must route through the validated module's DRBG.
- **MCU:** hardware TRNG (`prj.conf:22-29`). The `sys_rand32_get` at `zephyr_shim.c:106` is message-id only —
  **not** to be used for keys/nonces.
- **Where `lib-security` plugs in:** a single `CryptoProvider::fill_random(&mut [u8])` seam so every
  consumer draws from one audited source, and the FIPS build can enforce DRBG health tests (§2, §3).

### 1.8 Auth tokens — realistic vs speculative
**Realistic surfaces (thin):**
- If Sentient ever issues **device bearer/JWT tokens** for a direct-publish path (bypassing mosquitto), a
  `TokenVerifier` would validate signature/exp/aud/iss. Today there is no direct-publish path — mosquitto owns
  cloud auth (username/CA from `[cloud]` config, password from `CLOUD_MQTT_PASS`).
- A future **local management/health API** on the Pi (there is none today; the app is a headless poll loop,
  `bin-rpi/src/main.rs`) would need request auth — a token or mTLS.
**Speculative (document as stub, do not build eagerly):** OAuth2/OIDC flows, refresh tokens, session
management. There is no interactive user, no browser, no multi-tenant surface. Building a JWT stack now would be
**speculative complexity**. Recommendation: define the `TokenVerifier` trait for seam-completeness, ship a
`Deny`/`Unconfigured` default that refuses all tokens, and only implement a concrete verifier when a real token
surface is contracted. Flag any "we support JWT" claim as premature.

---

## 2. Cryptographic boundaries — WHERE the lines are drawn

### 2.1 FIPS boundary (FIPS 140-3)
**The cryptographic boundary is the validated module.** Everything cryptographic — key generation, DRBG,
signature verify/sign, TLS record crypto, AEAD — must execute *inside* a FIPS 140-3 validated module when
FIPS=on. What crosses the boundary: plaintext-to-be-protected, ciphertext, public keys, signatures, and
**opaque key handles** — never raw private/secret key bytes in a FIPS-relevant path.

**Approved-algorithm discipline (FIPS=on):**
- **Approved:** AES (GCM/CBC/CTR), SHA-2/SHA-3, HMAC, ECDSA (P-256/P-384), RSA (2048+, PKCS1v15/PSS),
  ECDH, SP 800-90A DRBG (CTR_DRBG/Hash_DRBG) seeded per SP 800-90B.
- **NOT approved / must be excluded when FIPS=on:** Ed25519/Curve25519 (not in the CMVP-approved set the way
  ECDSA is — so if the X.509 CMS signer cert at `verify-binary.sh` is ever issued on an Ed25519 key, a FIPS
  build must reject it; keep the signer on RSA/ECDSA), ChaCha20-Poly1305,
  BLAKE2/BLAKE3, non-approved DRBGs, MD5/SHA-1 for security.
- **Self-tests / KATs:** FIPS 140-3 requires **power-on self-tests** — Known-Answer-Tests per approved algorithm
  and **DRBG health tests** (SP 800-90B) — run *before* any crypto is served. The abstraction must fail closed
  if a KAT fails: `CryptoProvider::new()` returns `Err` and the process refuses to start (consistent with the
  no-panic posture, `Cargo.toml:52-55` — return an error, do not `unwrap`).

**How the abstraction prevents a non-FIPS backend at build when FIPS=on:** feature-flag *mutual exclusion* +
a compile-time guard. `fips` and the software/`ring`-style backend are **mutually exclusive Cargo features**;
a `compile_error!` fires if `fips` is enabled alongside a non-approved backend, or if `fips` is enabled with
*no* validated backend selected. The provider constructor is the runtime second gate (KAT-on-startup). Net rule:
**it must be impossible to `cargo build --features fips` and link a non-FIPS crypto provider.**

**One-liner:** *FIPS=on ⇒ all key-gen, DRBG, sign/verify, and record crypto run inside the FIPS 140-3 validated
module with power-on KATs and an approved-only algorithm set; a non-approved primitive (e.g. Ed25519) cannot be
selected at build (feature mutual-exclusion + `compile_error!`) or at runtime (constructor KAT fails closed).*

### 2.2 TrustZone / secure-computing boundary
**The secure-world boundary is the custody line for key material.**
- **Must NEVER leave the secure world / secure element:** private keys, sealing keys, symmetric master keys,
  the DRBG state if hardware-backed. These exist only as **opaque handles** in the normal world.
- **Normal world MAY hold:** public keys, certificates, ciphertext, signatures, key *handles*, and the results
  of operations (a signature, a decrypted-then-reencrypted payload) — never the private key bytes.
- **Call gateway shape:** the normal world calls *into* the secure world with (handle, operation, input) and
  receives (output) — a narrow, non-generic gateway (NSC / secure-monitor call on ARM TZ; PKCS#11 C_Sign on
  TPM). The gateway is the *only* FFI/`unsafe` surface and must be a dedicated module (mirrors the existing
  scoped `zephyr` carve-out, `impl-app/src/lib.rs:22-24`).
- **Software fallback (TrustZone=off) is a DIFFERENT security tier — documented, never silently equivalent.**
  With TZ=off the "handle" is backed by an in-process key. The API surface is identical (so parity tests pass
  for *public* operations) but the *custody guarantee is gone*. This must be surfaced: a
  `KeyStore::custody_tier()` returning `Hardware | Software`, logged at startup, and a build-time banner. A
  software provider that *looks* like a secure element but isn't is the single biggest false-security trap (§5).

**One-liner:** *TrustZone=on ⇒ private/sealing keys are born and used inside the secure element and cross the
boundary only as opaque handles through a narrow sign/decrypt gateway; TrustZone=off is an explicitly-labelled
lower tier (software custody), never presented as equivalent, and every key API returns a handle, never plaintext
key bytes.*

### 2.3 Trait/seam design implications
Four trait seams keep the toggles honest and key material opaque:
- **`CryptoProvider`** — stateless primitives: `fill_random`, `hash`, `hmac`, `verify_signature`, AEAD
  seal/open. Backend chosen by `fips` feature. This is the FIPS boundary object; construction runs KATs.
- **`KeyStore`** — custody of key material. **Returns `KeyHandle` (opaque), never `[u8]` private key.**
  `generate`, `import_public`, `sign(handle, msg)`, `sign_csr`, `custody_tier()`. Backend chosen by
  `trustzone` feature (TPM/PKCS#11/TZ vs software). This is the TrustZone boundary object.
- **`TokenVerifier`** — `verify(token) -> Result<Claims, TokenError>`; default `Deny` (§1.8).
- **`CertVerifier`** — chain build + validation, hostname check, expiry, revocation posture; used by the
  enrollment/CSR path and any Rust-side TLS config (broker listener, §1.4).

**Invariants the seams enforce:** (1) no method returns a plaintext private/secret key — keys are handles;
(2) the FIPS toggle and the TrustZone toggle are *orthogonal* (4 combinations, §3.6); (3) all fallible, no
panics (`Cargo.toml:52-55`); (4) `#![forbid(unsafe_code)]` on the crate root, with a single
`#[allow(unsafe_code)]` secure-world FFI submodule (§4).

---

## 3. Testing — WHERE and WHAT

Testing splits across: **unit** (in-crate `#[cfg(test)]`, `#[allow]`-scoped for unwrap per existing convention
`config.rs:151-152`), **integration** (`lib-security/tests/`), the **compile gate**
(`cargo clippy --workspace --all-targets -- -D warnings`, `Cargo.toml:51`), and **on-target** (Zephyr qemu +
hardware T&V, per security.md §5).

### 3.1 FIPS tests
- **Power-on self-test / KAT-per-algorithm:** for each approved primitive, a Known-Answer-Test vector; assert
  `CryptoProvider::new()` runs them and returns `Err` on any KAT mismatch (fail-closed).
- **DRBG health tests:** SP 800-90B repetition-count + adaptive-proportion tests; assert the DRBG refuses output
  before instantiation/reseed health passes.
- **Negative: FIPS=on rejects non-approved algorithm.** The load-bearing test. Assert that under `--features
  fips`, requesting Ed25519/ChaCha20/BLAKE3 is a **compile error** (trybuild-style `compile_error!` assertion)
  *and* that any runtime algorithm-id path returns `Err(Unapproved)`. Include the concrete Ed25519 case tied to
  the signature reality (§1.3): a FIPS build must not verify with Ed25519 (keep the X.509 CMS signer on RSA/ECDSA).
- **Build-guard test:** assert `fips` + non-FIPS-backend fails to compile (trybuild).

### 3.2 TrustZone / secure-element tests
- **Key-never-leaves-boundary:** assert the `KeyStore` API has **no method that returns private key bytes**
  (this is partly a type-level guarantee — verified by the absence of such a signature — plus a runtime test
  that `sign` works via handle while no export path exists). A trybuild test that attempts to read raw key
  bytes must fail to compile.
- **Handle opacity:** assert `KeyHandle` does not expose/`Deref`/`Debug`-print secret material; `Debug` shows a
  non-sensitive id only.
- **Provider parity (public ops):** hardware and software providers must produce identical results for *public*
  operations (verify a signature, hash, public-key import) — a parametrized test over both backends.
- **Deliberate divergence (custody):** assert `custody_tier()` returns `Hardware` for the secure backend and
  `Software` for the fallback, and that the divergence is *asserted*, not glossed — a test that the software
  provider is labelled a lower tier.

### 3.3 TLS tests (broker listener §1.4; cert lifecycle §1.1)
- Cert-chain validation matrix: **valid** (accept), **expired** (reject), **wrong-CA** (reject),
  **self-signed** (reject unless explicitly pinned), **hostname mismatch** (reject).
- **Downgrade / insecure-flag:** assert there is no code path equivalent to `bridge_insecure true`; a
  `CertVerifier` "insecure/allow-invalid" mode, if it exists at all, must be test-gated and refused in release.
- Broker-widen guard test: assert the startup assertion (§1.4) rejects `host != loopback` unless TLS+auth are
  configured — directly exercising the rule from security.md §1.2 and the config at `config.rs:233-248`.

### 3.4 Token tests
- Signature verify (good), tampered-token reject, expiry (`exp`) reject, audience/issuer (`aud`/`iss`) mismatch
  reject, `nbf`/clock-skew handling, and a replay consideration (nonce/jti if the surface warrants — else
  documented as out-of-scope per §1.8). Default `Deny` verifier rejects everything until configured.

### 3.5 Signature-verify (binary) tests
- Good binary → OK; tampered binary → reject; wrong-key → reject; missing sig/pubkey → reject (mirrors the
  shell logic at `verify-binary.sh:26-41`). If an in-app verifier is added (§1.3), test it against the same
  vectors and assert the FIPS build uses an approved algorithm.

### 3.6 Provider-swap matrix — the 4 combinations
Each of **(FIPS on/off) × (TrustZone on/off)** must **build** and **pass its boundary tests**:

| FIPS | TrustZone | Must build | Boundary asserts |
|---|---|---|---|
| off | off | yes | software crypto + software keystore; custody_tier=Software logged |
| off | on | yes | software/standard crypto + hardware custody |
| on | off | yes | validated crypto + software custody (label the tier loudly — odd but legal) |
| on | on | yes | validated crypto + hardware custody (the target production posture) |

Property/negative tests per cell: no-plaintext-key-export (all cells), KAT-on-startup (FIPS cells),
custody_tier correctness (all cells), unapproved-algorithm-rejected (FIPS cells). Run the matrix in CI as a
feature-permutation build (a small xtask or CI matrix), gated by the workspace clippy deny.

### 3.7 Where tests live
- **Unit:** `lib-security/src/**` `#[cfg(test)]`, unwrap `#[allow]`-scoped like `config.rs:151-152`.
- **Integration + trybuild + feature matrix:** `lib-security/tests/`.
- **Compile gate:** the existing `cargo clippy --workspace --all-targets -- -D warnings` (`Cargo.toml:51`)
  extended to the feature permutations.
- **On-target (Zephyr):** DRBG/TRNG and secure-element custody validated in qemu smoke + hardware T&V
  (security.md §5); the test RNG must be absent (`prj.conf:26`).

---

## 4. Recommended crate module structure

```
lib-security/
├── Cargo.toml            # features: default = ["software"]; fips; trustzone; software (fallback)
│                         # fips XOR software enforced via compile_error!; no_std-friendly deps only
└── src/
    ├── lib.rs            # #![forbid(unsafe_code)]  (crate-wide floor, matches core crates)
    │                     # #![cfg_attr(not(feature="host"), no_std)]  (MCU path, matches impl-app)
    │                     # compile_error! guards for illegal feature combos
    ├── error.rs          # SecurityError enum (thiserror on host; core::fmt on no_std) — NO panics
    ├── provider.rs       # trait CryptoProvider  (FIPS boundary object; new() runs KATs, returns Result)
    ├── keystore.rs       # trait KeyStore + KeyHandle (opaque) + CustodyTier { Hardware, Software }
    │                     #   RULE: no fn returns private key bytes — handles only
    ├── token.rs          # trait TokenVerifier + Claims + default Deny verifier
    ├── cert.rs           # trait CertVerifier (chain/hostname/expiry) + broker-widen startup guard
    ├── selftest.rs       # KAT vectors + DRBG health tests (FIPS)
    ├── backends/
    │   ├── mod.rs        # cfg-gated re-export of the selected (crypto × keystore) pair
    │   ├── software.rs   # standard-crypto CryptoProvider + in-process KeyStore (Software tier)
    │   ├── fips.rs       # validated-module CryptoProvider (approved-only; KAT-gated)
    │   └── secure/       # #[allow(unsafe_code)] ONLY here — the secure-world FFI gateway
    │       ├── mod.rs    #   TPM/PKCS#11 (host) or TrustZone NSC (MCU); narrow sign/decrypt gateway
    │       └── ffi.rs    #   extern "C" gateway calls (the sole unsafe surface, audited)
    └── config.rs         # SecurityConfig: which cert/handle/CA, custody expectations, token policy
```

**Non-negotiables baked into the structure:**
- `#![forbid(unsafe_code)]` at the crate root; the *only* `#[allow(unsafe_code)]` is `backends/secure/`
  (identical pattern to `impl-app/src/lib.rs:22-24`).
- No `unwrap`/`expect`/`panic` in logic — every fallible op returns `Result` (matches `Cargo.toml:52-55`);
  KAT/DRBG failure ⇒ `Err`, caller refuses to start (fail-closed, no panic).
- `fips` and `software` crypto backends are **mutually exclusive**; `compile_error!` on both-or-neither.
- `KeyStore` never yields plaintext private keys — `KeyHandle` is opaque, `Debug` prints an id only.
- `no_std`-compatible so the same crate serves the Zephyr MCU path (matches `impl-app/src/lib.rs:8`).
- `custody_tier()` + startup log line make the TrustZone-off tier *visible*, never silent.

---

## 5. Risk register / phasing

### 5.1 Must be REAL before ship
- **R1 — Broker-widen guard (§1.4).** If any deployment widens `broker.host` (config permits it,
  `config.rs:233-248`), the plaintext no-auth broker (`broker.rs:71-72`, `:83`) is exposed on the network. The
  startup assertion (`host != loopback ⇒ require tls+auth`) must be real before any non-loopback deployment.
  This is the one gap in *existing* code, not a new feature.
- **R2 — Zephyr DRBG entropy (§1.2, §1.7).** Shipping `CONFIG_MQTT_LIB_TLS=y` (`prj.conf:21`) with the test RNG
  or an unseeded DRBG defeats the transport encryption. The SoC hardware TRNG is a hard ship gate
  (`prj.conf:22-29`, security.md §5). Must be real, not stubbed, before the MCU path carries traffic.
- **R3 — FIPS approved-algorithm enforcement (§2.1).** If FIPS is contracted, the build-time mutual-exclusion +
  KAT-on-startup must be real, and any signature-verify path must use an approved algorithm in FIPS builds. The
  app-binary X.509 CMS gate (`verify-binary.sh`) already uses RSA/ECDSA, which is approved — just keep the signer
  cert off Ed25519. A "FIPS mode" that still allows a non-approved primitive is a compliance failure, not a stub.
- **R4 — Non-exportable key custody for the cloud client key (§1.1)** *if* mutual TLS is contracted — the TPM
  `tpm2-pkcs11` handle (security.md §3 use 2). Software custody here is a documented lower tier, not ship-grade
  for a "non-exfiltrable credential" claim.

### 5.2 Can be a documented STUB
- Auth tokens / JWT (§1.8) — `TokenVerifier` seam + `Deny` default; implement only against a contracted token
  surface. Building it now is speculative.
- In-app binary attestation (§1.3) — the systemd `ExecStartPre` gate (`bin-rpi.service:29`) already covers
  the app-integrity threat; an in-app verifier is defense-in-depth, deferrable.
- microSD logger at-rest (§1.9) — feature not built (Slice 6); document the AEAD requirement, stub the seam.
- LUKS sealing (§1.5) — stays in clevis/OS; the crate only references the sealed-blob concept, does not
  implement disk crypto.

### 5.3 Highest-risk FALSE-SECURITY trap
**A software `KeyStore`/secure-element stub that presents the same API as the hardware one and is silently
treated as equivalent.** With `trustzone` off, keys live in process memory but the call sites, logs, and demos
look identical to the TPM/TrustZone-backed build. An operator (or a customer-facing claim) could believe the
device credential is "non-exfiltrable / hardware-bound" (the exact framing security.md §3 uses) when it is in
fact a plain file/heap key that a compromised rootfs copies off the box. **Mitigations, mandatory:**
`custody_tier()` in the API, a startup log line stating `key custody = Software (NOT hardware-backed)`, a
build banner, and tests asserting the tier is reported and never labelled equivalent (§3.2). This mirrors the
honesty discipline security.md already enforces for "verified boot ≠ attestation" (§2) — the same discipline
must apply to "software keystore ≠ secure element."

**Second-order traps to avoid:**
- Presenting an in-app Ed25519 verify as FIPS-compliant (it is not, §2.1).
- Treating `sys_rand32_get` (`zephyr_shim.c:106`, a message-id source) as a crypto RNG.
- Moving the hardened mosquitto cloud-TLS handshake into Rust and widening attack surface (§1.1) — keep the
  crate to key custody + cert lifecycle, not TLS termination on the cloud path.
- A `CertVerifier` "insecure/allow-invalid" mode leaking into release (the Rust analogue of
  `bridge_insecure true`, which `mosquitto_bridge.rs`'s `render_bridge()` always renders as
  `false`, unconditionally — there is no config knob that can flip it).

---

## 6. Cross-reference to security.md

| This map | security.md anchor | Relationship |
|---|---|---|
| §1.1 cloud client key custody | §3 use 2, §4 | Implements the "TPM-held client key" hardening path as a `KeyStore` seam |
| §1.3 / §5.3 Ed25519-not-FIPS | §2, §6 | Adds the FIPS caveat to the X.509 CMS signature posture (keep the signer on RSA/ECDSA) |
| §1.4 broker-widen guard | §1.2 | Turns the operational rule into a code invariant |
| §1.5 LUKS sealing | §3 use 1 | Stays in clevis; crate only references |
| §1.6 device identity key | §3 use 3 | Primary `KeyStore` + provisioning fit |
| §2 FIPS DRBG / §1.7 entropy | §5 ship-blocker 1 | Same hardware-TRNG ship gate |
| §5.3 software-keystore trap | §2 (verified-boot ≠ attestation) | Same honesty discipline, applied to key custody |

*All Rust citations are `path:line` against the repo at audit time. This document is a map, not an
implementation; no crate code was written.*
