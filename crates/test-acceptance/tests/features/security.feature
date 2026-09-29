# The `lib-security` software crypto backend — key generation, signing, verification, the
# custody/tier honesty accessors, the FIPS approved-algorithm gate, the self-test, and
# `lib-link`'s `KeyStoreTlsIdentity` seam. Pure in-process: no broker, no real TLS handshake
# (those stay in `test-e2e/tests/{token_auth_software,tls_identity_software}.rs`).
#
# Every operation below is a direct call into the real `lib_security::backends::software`
# module (`SoftwareKeyStore`, `SoftwareVerifier`, `self_test`) and `lib_link::tls_identity`
# (`KeyStoreTlsIdentity`) — nothing here re-derives ECDSA math, the FIPS approved-algorithm rule, or
# rustls' signing contract.
#
# Sources: docs/security.md (posture/recommendations, the software-custody framing in §3);
# docs/security-integration-map.md §2 ("Cryptographic boundaries") and §3.6 (the FIPS x TrustZone
# provider-swap matrix this Scenario Outline mirrors, and `lib-security/tests/matrix.rs`'s own
# `approved_algorithm_grid`, which this Scenario Outline is a Gherkin-level mirror of).

Feature: Software cryptographic backend — keys, signatures, tiers, the FIPS gate, and TLS identity

  Scenario: A generated P-256 key signs a message and the software verifier accepts it
    Given a fresh software key store
    When the software key store generates a P-256 key
    And the software key store signs the message "acceptance-suite-message" with the generated key
    Then the software verifier accepts the signature against the exported public key

  @provisional
  @insecure-test-rng-non-crypto
  Scenario: Every key this suite generates is seeded by a non-cryptographic deterministic RNG
    # PROVISIONAL: InsecureTestRng (lib-security/src/backends/software/rng.rs) is a
    # deterministic splitmix64 generator for tests/examples ONLY — the same seed always reproduces
    # the same key material, which a real entropy source must never do. Every key this suite
    # generates (including every other scenario in this file) is seeded this way; production callers
    # must supply a real CSPRNG/hardware TRNG through the `Rng` seam.
    Given a fresh software key store
    When the software key store generates a P-256 key
    Then the software key store reports SoftwareMemory custody

  Scenario: A tampered signature is rejected as VerificationFailed
    Given a fresh software key store
    When the software key store generates a P-256 key
    And the software key store signs the message "do not trust a flipped bit" with the generated key
    And the software key store signature is tampered with
    Then the software verifier rejects the tampered signature with VerificationFailed

  Scenario: A signature checked against the wrong message is rejected as VerificationFailed
    Given a fresh software key store
    When the software key store generates a P-256 key
    And the software key store signs the message "message A" with the generated key
    Then the software verifier rejects the signature against the message "message B" with VerificationFailed

  @provisional
  @software-custody-exfiltrable
  Scenario: The software key store never exposes private key material and warns about its custody tier
    # PROVISIONAL: `KeyStore` (lib-security/src/traits.rs) deliberately has NO `export_private`
    # method — enforced at the TYPE level (there is simply no method to call), not by a runtime check
    # this scenario can exercise. What this scenario DOES check at runtime is the two honesty
    # accessors that follow from the lower tier that implies: `custody()` must report SoftwareMemory
    # (never claim hardware-backed custody it doesn't have), and — because software-custody keys live
    # in normal-world process memory and are exfiltrable by a compromised host (docs/security.md's
    # framing) — `custody_warning()` must be `Some` to surface that at boot.
    Given a fresh software key store
    When the software key store generates a P-256 key
    Then the software key store reports SoftwareMemory custody
    And the active security profile carries a custody warning
    And the active security profile's tier slug is "standard+software"

  Scenario Outline: The FIPS approved-algorithm gate allows approved algorithms and refuses the rest
    Then algorithm <algorithm> under fips <fips> is <outcome>

    Examples:
      | algorithm        | fips  | outcome  |
      | Ed25519           | false | approved |
      | Ed25519           | true  | refused  |
      | EcdsaP256Sha256    | false | approved |
      | EcdsaP256Sha256    | true  | approved |
      | EcdsaP384Sha384    | false | approved |
      | EcdsaP384Sha384    | true  | approved |
      | RsaPkcs1Sha256     | false | approved |
      | RsaPkcs1Sha256     | true  | approved |

  @provisional
  @fips-selftest-stub
  @self-test-not-kat
  Scenario: The software backend self-test round-trips a generated key end-to-end
    # PROVISIONAL (@self-test-not-kat): self_test() (lib-security/src/backends/software/mod.rs)
    # is NOT a NIST-published Known-Answer-Test — there is no external reference vector checked
    # against here; it is a round-trip health check that the linked p256/ecdsa stack signs and
    # verifies correctly, using a FIXED, non-secret InsecureTestRng seed (deterministic and
    # reproducible, not a real key).
    # PROVISIONAL (@fips-selftest-stub): self_test() is NOT wired into
    # `lib_security::Provider::new`'s FIPS power-on self-test hook — that hook
    # (`run_power_on_self_tests`, provider.rs) remains a fail-closed `NotImplemented` stub until a
    # real FIPS-validated module is bound; this backend's self_test is a separate, non-FIPS-tier
    # sanity check a caller may invoke directly, not the FIPS KAT gate itself.
    When the software backend self-test runs
    Then the software backend self-test succeeds

  Scenario: KeyStoreTlsIdentity signs through a real software KeyStore and the signature verifies
    Given a fresh software key store
    When the software key store generates a P-256 key
    And a KeyStoreTlsIdentity over the software key store signs the message "a real TLS handshake transcript hash would go here"
    Then the KeyStoreTlsIdentity reports SoftwareMemory custody
    And the software verifier accepts the signature against the exported public key
    And the software verifier rejects the signature against the message "a different, unsigned message" with VerificationFailed
