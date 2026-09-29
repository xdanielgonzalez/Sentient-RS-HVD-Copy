# 0014 — Device identity and config provenance: what the certificate carries, and what it does not

**Status:** Proposed (2026-07-27) — EPIC #350. **Design record; no code written yet.**

Extends ADR [0013](./0013-transport-security-trust-store.md) (the `TrustStore`/`device_credential`
seam this record puts content into) and ADR [0011](./0011-mcu-security-boundary.md) (the S/NS
boundary the identity key never leaves). Depends on a provisioning-CA ADR that does not exist yet
(see Open questions).

## Context

The owner's original instinct, stated directly: **any configuration the device needs should come
from the certificate** — one signed artifact, one provable-origin chain, config and identity
issued together. That instinct is right about the *property* it wants (config with a provable
issuer) and wrong about the *mechanism* (the X.509 certificate itself as the config carrier). This
ADR records why, and what to do instead.

**The binding constraint is RFC 5280 §4.2, not a risk judgement.** It states plainly: *"A
certificate-using system MUST reject the certificate if it encounters a critical extension it does
not recognize."* ADR 0013 §3 requires our server certificate to verify against a pinned CA using
**stock mosquitto and paho** — tooling we do not control and cannot patch. Any custom extension we
add is, by construction, unrecognized by that tooling. If we mark it critical, RFC 5280 obligates
stock clients to reject our certificate outright. So every custom extension we define **MUST** be
non-critical. This is not a judgement call weighing convenience against purity; it is what "verify
with stock tooling" and "MUST reject unrecognized critical extensions" jointly require.

**There is real precedent for identity-shaped content in a certificate — IEEE 802.1AR and BRSKI
(RFC 8995).** 802.1AR defines two device identities: IDevID (manufacturer-installed, long-lived)
and LDevID (locally significant, operator-rotatable) — a distinction this repo's per-unit identity
already matches conceptually. BRSKI §2.3.2 defines `id-pe-masa-url`: one small, purpose-built,
non-critical extension carrying a single config-shaped value (the Manufacturer Authorized Signing
Authority URL) under its own OID, alongside the identity fields. `subjectAltName otherName` carries
`HardwareModuleName` (RFC 4108) — vendor + serial, again identity, not site config. **Read that
precedent honestly and it cuts both ways.** It establishes that *one small OID per semantically
distinct value* is a legitimate, standards-track pattern — that is the mechanism this ADR adopts.
It does **not** establish that *mutable site configuration* belongs in an identity certificate:
`id-pe-masa-url` is manufacturer-fixed and rotates on the same cadence as the certificate itself,
which is precisely the property site config (broker address, hostname override) does not have. We
looked for an 802.1AR/BRSKI precedent for mutable per-site config carried in identity certs and did
not find one. Also worth noting: 802.1AR conventionally issues IDevIDs with
`notAfter = 99991231235959Z`, because long-lived device identity and calendar validity are already
in tension in the standard everyone points to — the same tension ADR 0013 §5 resolved by pinning
SPKI and ignoring validity windows entirely.

**Three things are true at once, and the decision below has to hold all three:**

1. The owner wants config to have a provable issuer, not to be an unauthenticated file dropped on
   the device.
2. RFC 5280 forbids the broad reading (arbitrary mutable config as critical or even non-critical
   cert content) from surviving contact with stock TLS tooling in the way that would matter for
   trust decisions — and even where non-critical extensions are technically legal, cramming
   site-mutable state into a certificate collides with certificate re-issuance and SPKI-pin
   invalidation the moment that state changes (ADR 0013 §3, §5).
3. Some of what the owner is reacting to is not a certificate problem at all — several of the
   values in question are spec-mandated constants that must never vary by device or by config, cert
   or otherwise.

## Decision

**Carry in the certificate what is standards-compliant and good practice for a device identity
certificate — and nothing more.** Concretely: one small, non-critical, purpose-built extension per
semantically distinct field, each under our own IANA Private Enterprise Number (PEN) arc
(`1.3.6.1.4.1.<PEN>.…`), modelled directly on BRSKI's `id-pe-masa-url` — **not** one omnibus config
blob. The parser this implies is a flat scan → match-OID → bounds-check → copy loop per extension,
the same shape as the rest of this codebase's `no_std` parsing discipline — not a nested
general-purpose format parser, which would be a second attack surface next to the DER parser it
already needs.

PEN registration is free and takes roughly a week from IANA's follow-up, per RFC 9371 and IANA's
own application page — this is a prerequisite with a known, short lead time, not an open-ended
blocker.

Everything else the owner's original instinct reached for is real, but belongs to one of three
*other* places, not the certificate. The four-tier split below is the core of this record.

### The four-tier split

| Tier | Example | Source | Why |
|---|---|---|---|
| **Spec-invariant** | `POLL_INTERVAL_MS`, DDS `DOMAIN_ID`, `PUBLISH_INTERVAL_MS`, `STACK_SOCKETS` sizing floor | compile-time `const` + `const _: () = assert!(...)` — **never** cert-carried, never config-sourced | The standard names one correct value; deviation from it is nonconformance, not a configuration choice. A mis-issued certificate that carried the wrong value would reproduce the defect *with a valid signature attached* — which is a worse failure than today's plain misconfiguration, because downstream consumers would trust the bad value MORE, not less: it would arrive with cryptographic provenance it did not earn. |
| **Per-unit identity** | device ID / serial | the **certificate**, non-critical extension, 802.1AR/BRSKI convention | Set once at provisioning, changes only on hardware replacement — the one tier that actually matches "identity," which is what a certificate is for. |
| **Per-site config** | broker address, hostname override | a **separate signed artifact** — generalising ADR 0013 §3's "rotation is a signed configuration push" | Decouples config churn from certificate re-issuance. Still signed, so it keeps the provable-origin property the owner actually wants, without dragging SPKI-pin invalidation along with every config edit. |
| **Per-build tuning** | buffer sizes, queue depths | `build.rs`, from a checked-in config file | Needs compile-time sizing regardless of where any other value comes from — it shapes `static` allocations before the binary exists. |

**Tier 1 is not a proposal — it is done and verified on this branch.** `POLL_INTERVAL_MS == 1000`
and `STACK_SOCKETS > MAX_CLIENTS + 2` are const-asserted in
`crates/bin/mcu-rp2350/ns/src/main.rs:344` and `:366`; `PUBLISH_INTERVAL_MS == 1000` is const-asserted
in `crates/bin/mcu-rp2350/ns/src/dds_pub/mod.rs:136`; `DOMAIN_ID == 0` is const-asserted in
`crates/bin/mcu-rp2350/ns/src/spdp/mod.rs:143`. The Pi side enforces the same class of constraint at
config-load time rather than compile time — `PollConfig::validate()`
(`crates/bin/rpi/src/config/impl_poll_config.rs:18`) rejects an `interval_ms` that would
under-publish `DcMeasurementUpdate` below its mandated 1 Hz, and `DdsToml::validate()` gates
`domain_id`; both run inside `assert_spec_constraints` (`crates/bin/rpi/src/config/mod.rs:933`),
which every shipped `.toml` under `deploy/` is checked against in a test loop. Each MCU gate was
verified by hand by setting a wrong constant and observing `error[E0080]: evaluation panicked:
...` at build time — the mechanism was watched failing shut, not just read. This tier is the
concrete answer to the part of the owner's instinct that is completely correct: these values must
never be allowed to drift, cert-carried, config-sourced, or otherwise. The fix is that they never
vary at all, which is a stronger property than any signature can provide.

### The MAC address is a deliberate exception, and it goes to flash, not the certificate

`crates/bin/mcu-rp2350/ns/src/boot_w5500.rs:69` fixes `W5500_MAC = [0x02, 0x00, 0x00, 0xaa, 0xbb,
0xcc]` for every unit. That is a real per-unit identity gap — it feeds both the RTPS `GuidPrefix`
and the `fe80::` link-local address, so on real hardware it **must** vary per device. It is
tempting to route it through the same mechanism as the device ID, since both are "per-unit." **We
recommend against that.** There is no security benefit to DER-parsing a value that is never part
of a peer trust decision — the MAC is link-layer plumbing, not an identity a peer authenticates
against — and parsing it out of a certificate only adds cost and a dependency on the certificate
being present before the network stack can even bring up an interface. Route it instead as a
**provisioned flash constant read at boot**, properly sourced from hardware/EEPROM on a real board
(the W5500 has no on-chip factory MAC; a production board would burn one into an EEPROM the
bootloader reads). This decouples **link-layer identity** (MAC, `GuidPrefix`) from **application
identity** (`deviceId`, the certificate) — two different questions that happen to both be called
"identity," and conflating them is exactly the kind of drift this ADR is trying to close off.

### Operational model — costs, stated honestly

- **Config in the certificate means config change = certificate re-issuance.** Under ADR 0013 §3's
  SPKI-pinning model, a new certificate generally changes the pinned SPKI hash, which means every
  peer's pin list needs updating on every config change. That coupling is the direct, mechanical
  reason site config is **not** cert-carried — it is not a style preference, it falls out of the
  pinning model we already committed to.
- **Fail-closed on absence, extending the pattern already enforced in this seam.**
  `TrustStore::verify_peer` fails closed with `SecurityError::UntrustedChain`
  (`crates/lib-security/src/traits/types/trust_store.rs:62`); `extract_spki`'s doc comment is
  explicit that a caller must treat failure "as 'this credential cannot be verified', never as 'no
  SPKI, allow'" (`crates/lib-security/src/impl_spki_extract.rs:55`). A per-unit identity extension
  that is absent or malformed gets the same posture: "cannot establish identity," never "fall back
  to a default." Worth naming directly: the current `DEVICE_HOSTNAME` default of `"meteor-pico"`
  (`crates/bin/mcu-rp2350/ns/src/boot_w5500.rs:51`, baked in at build time via `option_env!` when the
  env var is unset) reads as a plausible real hostname rather than an obviously-unprovisioned
  placeholder. That is itself a small honesty gap in the existing code, in the same family this ADR
  is closing — a future per-unit identity path should not repeat it.
- **Identical parse rigor for our own certificate as for a peer's.** It is the same function called
  from two call sites, and our own certificate arrives to us through a provisioning channel that
  can itself corrupt it — there is no reason to trust it more than an inbound peer credential. The
  bounded, alloc-free, non-recursive DER `SubjectPublicKeyInfo` extractor
  (`crates/lib-security/src/impl_spki_extract.rs`, currently a stub returning
  `SecurityError::NotImplemented` pending ADR 0013 WP-1) is the contract any custom-extension
  extractor extends — the same bounded-parse, fuzzed, Miri-checked discipline, not a second, laxer
  parser written because "it's our own data."
- **The veneer has room.** `SV_MAX_PAYLOAD = 4096` (`crates/bin/mcu-rp2350/abi/src/lib.rs:53`) is the
  natural ceiling for a future `sv_device_credential` veneer call: a typical EC leaf certificate is
  400–700 bytes, so a handful of small extensions fits comfortably inside the existing frozen
  payload cap. That call **does not exist yet** — ADR 0013's WP-4 veneer-ABI-extension list
  (`sv_sign`, `sv_agree`, `sv_derive`, `sv_record_seal`, `sv_record_open`, `sv_verify_peer`,
  `sv_trusted_time`) does not include it, and this ADR does not add it to that list; a future
  implementer needs to.

### Signing — correcting a premise, not answering the question

The owner asked whether config and firmware binaries should share a signer. **The premise needs
correcting before that question is answerable: there is no device-identity CA in this repo at
all today.** `TrustStore::device_credential` is unimplemented in every backend but the
`unimplemented` stub (`crates/lib-security/src/backends/unimplemented/impl_unimplemented_trust_store.rs:38`),
and ADR 0013's WP-5 says only "provision the identity key" — it names no issuer, no CA hierarchy,
no issuance process. So this is a constraint on a process that does not exist yet, not a decision
between two existing signers.

With that corrected, the recommendation: **one shared offline root, with separate,
purpose-scoped intermediates** for release/CMS signing and for device-identity issuance. They
should not share an *operational* key. `docs/security/signing-and-verification.md` already
describes release signing as using `SIGNING_KEY`, a GitHub Actions secret touched by CI on every
tagged release — high-automation, low value-per-signature. Device-identity issuance is the
opposite: low-automation, high-value-per-signature, one signature per unit ever manufactured, ever.
Sharing the *operational* key would mean a CI secret leak also grants device-identity forgery — the
same failure mode a security review would flag immediately, and for good reason. A shared **root**
still gives the common-provable-origin property the owner wants (both intermediates chain to one
anchor); shared **operational keys** would not survive scrutiny. `docs/security/signing-and-verification.md`
already frames release signing (cosign) and on-device integrity (X.509 CMS) as *"two independent,
non-overlapping signature mechanisms"* that answer different questions. Device-identity issuance
would be a **third**, answering a third question — "who is this specific unit" rather than "was
this release published by CI" or "is this binary on disk what was signed." It should be recorded
alongside those two once the provisioning-CA ADR exists, not folded into either.

## Consequences

**What this gives us.**

- A certificate that stock mosquitto/paho can verify, because every custom extension is
  non-critical by construction — RFC 5280 compliance is structural, not a thing to remember.
- A concrete, standards-modelled mechanism (one OID per field, BRSKI-shaped) for the one tier that
  genuinely belongs in the certificate: per-unit identity.
- Spec-invariant values protected by a stronger property than any signature could provide — they
  cannot vary at all, and the MCU side of that is already implemented and was verified failing
  shut.
- A named, honest place for site config's provable-origin requirement (a separate signed artifact)
  that does not couple config churn to SPKI-pin invalidation across every peer.
- The MAC/GuidPrefix per-unit gap is named and routed to the right mechanism (flash provisioning),
  rather than folded into the certificate question it does not actually belong to.

**What this does NOT give us — stated so nobody infers it.**

- **No config-signing mechanism exists yet.** The "separate signed artifact" for per-site config is
  a recommendation, not a shipped path. ADR 0013 §3 uses the phrase "a signed configuration push"
  for certificate rotation, but no implementation of that mechanism was found in this repo. It does
  not exist for site config either.
- **No provisioning CA exists.** Nothing issues per-unit identity certificates today; WP-5 names
  the gap without naming an issuer.
- **No PEN has been obtained.** The mechanism this ADR specifies depends on it; it is a real,
  bounded (about a week), but outstanding prerequisite.
- **`sv_device_credential` is not in the frozen veneer ABI.** This ADR names where it would sit
  (§ above) but does not add it — that is future work against ADR 0013 WP-4.
- **No measured `no_std` parse cost for a custom-extension extractor exists.**
  `impl_spki_extract.rs` is still a stub; the cost of the bounded parser this ADR calls for is
  unmeasured, same open item ADR 0013 already carries for SPKI extraction generally.

## Open questions

1. **A provisioning-CA ADR does not exist and this ADR depends on one.** It needs to name the
   issuer, the intermediate hierarchy, the issuance process (per-unit, at manufacture), and how it
   relates to the shared root recommended above. Add this as a tracked work item; nothing in §
   "Signing" above can be executed without it.
2. **No PEN has been obtained.** File the IANA application; per RFC 9371 and IANA's own process
   this is free and takes on the order of a week — track it as a prerequisite with a known lead
   time, not an open-ended blocker.
3. **`sv_device_credential` is not in the frozen veneer ABI.** Needs its own ABI-version-word change
   against ADR 0013 WP-4's list, sized against the measured 400–700-byte EC leaf certificate and the
   4096-byte `SV_MAX_PAYLOAD` ceiling.
4. **No measured `no_std` parse cost for the per-unit-identity extension extractor.** Same class of
   gap as ADR 0013's open question on `impl_spki_extract.rs`; should be measured together with it
   rather than separately, since both extend the same bounded-parse contract.
5. **The full IEEE 802.1AR-2018 text is paywalled and was not read.** The claims above about
   IDevID/LDevID and the `notAfter = 99991231235959Z` convention come from the IEEE 802.1 Technology
   & Standards working-group summary plus RFC 8995 (BRSKI) and RFC 4108 (`HardwareModuleName`), not
   from the standard's own text. If a future implementer needs a normative citation, the standard
   itself has to be obtained and read.
6. **Whether a signed config-push mechanism strong enough for site config exists in usable form
   elsewhere in this repo is unresolved.** ADR 0013 §3 references "a signed configuration push" as
   the rotation mechanism for SPKI pins but no implementation was found (`git grep` across the repo
   for the concept turns up the reference, not the mechanism). Tier 3 of this ADR assumes that
   mechanism will be built; it is not inventing scope that already exists.

## Revisit trigger

- **The provisioning-CA ADR lands** — turns the "Signing" section above from a recommendation into
  an executable plan, and resolves open question 1.
- **The PEN is obtained** — unblocks defining the actual OID arc and the extension's ASN.1 shape.
- **ADR 0013 WP-4 (veneer ABI extension) is scoped** — `sv_device_credential` should be added to
  that work package's list at that time, not deferred again.
- **A real per-unit MAC provisioning path is designed for production hardware** — the flash-constant
  recommendation above should be checked against whatever EEPROM/factory-programming mechanism that
  hardware actually offers.
- **A signed config-push mechanism is built for any other reason** (e.g. SPKI-pin rotation under ADR
  0013 §3) — Tier 3 of this ADR should point at that mechanism rather than restate the requirement
  for it.
