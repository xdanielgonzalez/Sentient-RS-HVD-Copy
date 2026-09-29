# Security Policy

sentient-rs is a telemetry adapter for the Sentient METEOR TPM/TEM: it polls a Seplos BMS over
RS-485, maps readings to TMS (MIL-STD-3071), and publishes over MQTT. It ships two runtime
targets — a Raspberry Pi running a Yocto Linux image (`bin-rpi`, std/tokio) and an MCU running
Zephyr (`no_std`) — both handling telemetry from physical hardware and TLS-protected network
traffic. Please report security issues responsibly, as described below.

## Reporting a vulnerability

**Do not open a public GitHub issue for a security vulnerability.**

Preferred: use GitHub's private vulnerability reporting for this repository —
[Report a vulnerability](https://github.com/High-Velocity-Decisions/sentient-rs/security/advisories/new)
(repo **Security** tab → **Report a vulnerability**). This opens a private draft security advisory
visible only to maintainers, where we can discuss and coordinate a fix before any public
disclosure.

If private vulnerability reporting is not enabled on this repository yet, or you're unable to use
it, contact a maintainer directly (see the repository's collaborator list) and ask for a private
channel to share details — do not post reproduction details, exploit code, or affected-version
specifics in a public issue, PR, or discussion.

Please include as much of the following as you can:

- A description of the vulnerability and its potential impact.
- Affected component(s) — e.g. `lib-security`, `lib-mqtt`, `bin-rpi`, `bin-zephyr`/`bin-zephyr-platform`,
  the `distro/` Yocto layer, or a specific dependency.
- Affected target(s) — Raspberry Pi (Yocto/std/tokio) and/or MCU (Zephyr/no_std).
- Steps to reproduce, a proof of concept, or a fault dump (symbolize any Cortex-M fault with
  `addr2line` against the unstripped ELF if applicable).
- Any suggested mitigation, if you have one.

## Scope

In scope:

- The Rust workspace (`crates/`, `bin/zephyr/`) — protocol parsing (`lib-parse`), TMS mapping
  (`lib-mapping`), MQTT dissemination (`lib-mqtt`), auth/crypto (`lib-security`, wolfCrypt
  backend), the network-driver abstraction (`lib-link`), and the platform binaries (`bin-rpi`,
  `bin-zephyr`).
- The Yocto image build (`distro/`) — image hardening posture, systemd unit configuration,
  mosquitto TLS/auth, signing and SBOM generation.
- Release/CI supply chain — signing (cosign, X.509 CMS), SBOM generation, `cargo-deny`/`cargo vet`
  gates, GitHub Actions workflows under `.github/workflows/`.

Out of scope / please use judgment:

- Vulnerabilities that require physical access to already-compromised hardware.
- Issues in third-party dependencies with no demonstrated impact on this project — please report
  those upstream, but let us know if we need to update a pin.
- Denial of service against infrastructure we don't control (e.g. a downstream MQTT broker you
  operate yourself).

## What to expect

This is a small team. We aim to acknowledge new reports within a few business days and will keep
you updated as we investigate and remediate. Fix timelines depend on severity and the affected
target — a firmware fix on a fielded MCU takes longer to roll out than a Pi-side patch. We'll
credit reporters in release notes/advisories unless you ask us not to.

## Safe harbor

We won't pursue or support legal action against anyone who makes a good-faith effort to report a
vulnerability privately per this policy, avoids privacy violations and data destruction, and gives
us reasonable time to remediate before any public disclosure.
