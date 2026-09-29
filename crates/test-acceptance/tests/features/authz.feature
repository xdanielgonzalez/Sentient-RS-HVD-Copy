# Verifier-level MQTT CONNECT authorization matrix — the `lib-security` `TokenVerifier` seam
# `lib_mqtt::broker::seam_auth` gates every CONNECT through, exercised directly against the
# verifier (no broker, no network I/O). Two verifiers are exercised, on purpose:
#
#   * `SoftwareTokenVerifier` (real ECDSA P-256-signed tokens, minted with
#     `lib_security::backends::software::sign_token` against a real `SoftwareKeyStore`-held
#     key) — the accept/reject matrix docs/security-integration-map.md §3.4 ("Token tests") lists:
#     signature verify (good), tampered-token reject, wrong-key reject, audience/issuer mismatch
#     reject, expiry reject, malformed/short reject.
#   * `test_kit::mqtt::FakeTokenVerifier` (a shared-secret test double — see its own
#     `@fake-verifier-shared-secret` note below) — used only to exercise the `seam_auth` rule that
#     the verified claims' subject must equal the CONNECT username
#     (`lib-mqtt/src/broker.rs:75`: `claims.subject.as_str() == username`), mirrored here as a
#     plain Rust-level equality check on the returned `TokenClaims`, never by calling `seam_auth`
#     or a broker.
#
# NOTE: the actual CONNACK-level behavior (a denied CONNECT never leaks a CONNACK, publishes flow
# only after a real accepted CONNECT, etc.) lives in
# `test-e2e/tests/{broker_auth,token_auth_software}.rs`, which drive a real embedded broker over
# a real network client — that broker I/O is NOT duplicated here.

Feature: Verifier-level token authorization matrix and the seam_auth subject/username rule

  Scenario: A validly signed, in-window token verifies and returns the signed claims
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    When the token is verified against issuer "sentient-test-issuer" audience "lib-mqtt-broker" at time 1000
    Then the token verification succeeds with subject "device-1"

  Scenario: A tampered token signature is rejected
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    And the token's signature is tampered with
    When the token is verified against issuer "sentient-test-issuer" audience "lib-mqtt-broker" at time 1000
    Then the token verification fails with VerificationFailed

  Scenario: A token verified against an untrusted (wrong) key is rejected
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    And the trusted key is swapped for an untrusted key
    When the token is verified against issuer "sentient-test-issuer" audience "lib-mqtt-broker" at time 1000
    Then the token verification fails with VerificationFailed

  Scenario Outline: A wrong issuer or audience is an identity mismatch, never a signature failure
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    When the token is verified against issuer "<issuer>" audience "<audience>" at time 1000
    Then the token verification fails with IdentityMismatch

    Examples:
      | issuer                | audience               |
      | someone-else          | lib-mqtt-broker   |
      | sentient-test-issuer  | someone-elses-broker   |

  Scenario: A token past its expiry is rejected
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    When the token is verified against issuer "sentient-test-issuer" audience "lib-mqtt-broker" at time 9999999999
    Then the token verification fails with Expired

  Scenario: A malformed, too-short token is rejected
    Given a software-issued token for subject "device-1" issued by "sentient-test-issuer" for audience "lib-mqtt-broker" expiring at 9999999999
    And the token is truncated to 10 bytes
    When the token is verified against issuer "sentient-test-issuer" audience "lib-mqtt-broker" at time 1000
    Then the token verification fails with Malformed

  @provisional
  @fake-verifier-shared-secret
  Scenario: A FakeTokenVerifier token with the wrong shared secret is rejected
    # PROVISIONAL: test_kit::mqtt::FakeTokenVerifier (its own module doc) is a shared-secret
    # compare (`subject|issuer|audience|expires_at|shared_secret`), NOT real cryptography — a
    # lightweight test double for exercising broker wiring, used here ONLY to set up the seam_auth
    # subject/username scenarios below without minting real ECDSA tokens for every case. It is not a
    # security boundary.
    Given a fake-verifier token with the wrong shared secret for subject "device-1"
    When the fake-verifier token is verified
    Then the token verification fails with VerificationFailed

  Scenario: seam_auth permits a valid token whose verified subject matches the CONNECT username
    Given a fake-verifier token for subject "device-1"
    When the fake-verifier token is verified
    Then applying the seam_auth subject rule against username "device-1" permits the connection

  Scenario: seam_auth refuses a valid token whose verified subject does not match the CONNECT username
    Given a fake-verifier token for subject "device-1"
    When the fake-verifier token is verified
    Then applying the seam_auth subject rule against username "impersonator" refuses the connection

  # docs/security.md §1.2: `auth: None` is the default and is safe ONLY because the broker is also
  # loopback-bound by default (§1.1) — widening `host` beyond loopback without also configuring
  # `auth` would expose an unauthenticated plaintext feed on the network (an operational rule, not
  # code-enforced today).
  Scenario: The broker's default configuration carries no CONNECT authorizer
    Then the broker's default configuration carries no CONNECT authorizer
