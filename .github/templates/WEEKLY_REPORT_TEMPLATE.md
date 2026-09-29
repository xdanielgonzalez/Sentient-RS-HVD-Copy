---
# ──────────────────────────────────────────────────────────────────────────────
# HOW TO USE THIS TEMPLATE
#
# 1. Copy this file. Name the copy after the week_ending date, e.g.:
#      Weekly-Report-2026-07-13.md
#
# 2. Fill in every field in the FIELDS section below.
#
# 3. Complete each section in the report body (placeholders marked <!-- -->).
#    Follow the per-section instructions in the SECTION INSTRUCTIONS block.
#
# 4. When complete, publish to the wiki (see PUBLISHING below).
# ──────────────────────────────────────────────────────────────────────────────
# FIELDS
# week_ending : ISO-8601 date of the Sunday closing the reporting period.
# author      : GitHub username of the person filing the report.
# version     : Semantic version of sentient-rs shipped or targeted this week.
# sprint      : Sprint or iteration number/name, if applicable.
# status      : Overall program health — Green | Yellow | Red.
# ──────────────────────────────────────────────────────────────────────────────
# SECTION INSTRUCTIONS
#
# Executive Summary
#   - 2–4 sentences maximum.
#   - Lead with the single most important delivery or decision of the week.
#   - Call out any active blockers and their severity.
#   - Close with the team’s confidence level heading into next week.
#
# Goals Tracked
#   - List every goal committed to at the start of this sprint/week.
#   - Do not delete rows — mark incomplete ones Blocked or carry forward with a note.
#   - Status key:
#       🟢 Done        — fully complete, acceptance criteria met
#       🟡 In Progress — work underway, on track
#       🔴 Blocked     — cannot proceed; blocker must appear in Risks & Blockers
#       ⚪ Not Started — not yet begun (explain in Notes if intentional)
#
# Stories / Epics
#   - Source rows directly from GitHub Issues / PRs.
#   - Type values: Story | Epic | Bug | Chore | Spike
#   - Link every issue/PR number to its GitHub URL.
#   - “Closed This Week” = merged/closed within the report window.
#   - “Open / In Progress” = actively worked or closing within two weeks.
#   - Est. Close: ISO-8601 date; use “—” if unknown.
#
# Compliance
#   - Update every Status cell weekly — no stale entries.
#   - Status values: ✅ Met | 🔄 In Progress | ❌ Gap
#   - Evidence / Notes: CI run URL, PR number, or one-line explanation of gaps.
#   - Add rows for new compliance requirements introduced this sprint.
#   - Any ❌ Gap requires a mitigation row in Risks & Blockers.
#
# Release Artifacts
#   - One row per tag/release published during the report window.
#   - Type: Stable | Pre-release | Hotfix
#   - Links must include Release notes; add Changelog and SBOM where available.
#   - Update the cosign binary name in the verification snippet per release.
#   - Remove placeholder rows before publishing.
#
# Risks & Blockers
#   - Every 🔴 Blocked goal and every ❌ Compliance Gap must have a row here.
#   - Severity: High | Medium | Low
#   - Mitigation: a concrete next action, not a vague description.
#   - Resolved risks from prior weeks may be struck through or removed.
#
# Next Week
#   - List 3–5 committed action items only — not a wish list.
#   - Each item must be specific enough to verify as done/not-done.
#   - Prefix with the GitHub issue/PR number where applicable.
# ──────────────────────────────────────────────────────────────────────────────
# PUBLISHING
#   1. Wiki page name: Weekly-Reports/YYYY-MM-DD
#      where YYYY-MM-DD matches week_ending. Example: Weekly-Reports/2026-07-13
#
#   2. Clone the wiki, add the file, and push:
#        git clone https://github.com/High-Velocity-Decisions/sentient-rs.wiki.git
#        cp <completed-report>.md sentient-rs.wiki/Weekly-Reports/YYYY-MM-DD.md
#        cd sentient-rs.wiki
#        git add Weekly-Reports/YYYY-MM-DD.md
#        git commit -m "Weekly report: YYYY-MM-DD"
#        git push
#
#   3. Update the wiki sidebar/index to link the new entry:
#        [[Weekly-Reports/YYYY-MM-DD|Week ending YYYY-MM-DD]]
#
#   4. Strip this entire front matter block before committing to the wiki —
#      GitHub Wiki renders it as raw text.
#
#   5. Do not publish drafts. All ❌ Gaps and 🔴 Blocked goals must have
#      a corresponding Risks & Blockers row before the report goes up.
# ──────────────────────────────────────────────────────────────────────────────
title: "sentient-rs Weekly Engineering Report"
week_ending: "YYYY-MM-DD"
author: "@<!-- github-username -->"
repository: "High-Velocity-Decisions/sentient-rs"
version: "v0.x.y"
sprint: ""
status: "Green"  # Green | Yellow | Red
---

# sentient-rs — Weekly Engineering Report

| | |
|---|---|
| **Week ending** | <!-- YYYY-MM-DD --> |
| **Author** | @<!-- github-username --> |
| **Version** | <!-- v0.x.y --> |
| **Overall status** | 🟢 Green *(update to 🟡 Yellow or 🔴 Red as needed)* |

---

## Executive Summary

> _Replace this line with your summary._

---

## Goals Tracked

| # | Goal | Owner | Status | Notes |
|---|------|-------|--------|-------|
| 1 | <!-- e.g. Stabilize MQTT TLS handshake --> | @<!-- user --> | 🟢 Done / 🟡 In Progress / 🔴 Blocked / ⚪ Not Started | |
| 2 | | | | |
| 3 | | | | |

---

## Stories / Epics

### Closed This Week

| Issue / PR | Title | Type | Closed |
|------------|-------|------|--------|
| [#NNN](https://github.com/High-Velocity-Decisions/sentient-rs/issues/NNN) | | Story / Epic / Bug | YYYY-MM-DD |

### Open / In Progress

| Issue / PR | Title | Type | Assignee | Est. Close |
|------------|-------|------|----------|------------|
| [#NNN](https://github.com/High-Velocity-Decisions/sentient-rs/issues/NNN) | | Story / Epic / Bug | @<!-- user --> | YYYY-MM-DD |

---

## Compliance

| Requirement | Standard | Status | Evidence / Notes |
|-------------|----------|--------|-----------------|
| TMS schema conformance | MIL-STD-3071 | ✅ Met / 🔄 In Progress / ❌ Gap | |
| Supply-chain audit (`cargo vet`) | SLSA / internal policy | ✅ Met / 🔄 In Progress / ❌ Gap | |
| Build provenance attestation | SLSA L2 | ✅ Met / 🔄 In Progress / ❌ Gap | |
| MQTT spec-compliance suite | MQTT 3.1.1 | ✅ Met / 🔄 In Progress / ❌ Gap | |
| Security / authz scenarios | Internal policy | ✅ Met / 🔄 In Progress / ❌ Gap | |
| Test coverage gate (`tarpaulin`) | ≥ threshold | ✅ Met / 🔄 In Progress / ❌ Gap | |

---

## Release Artifacts

| Release | Tag | Type | Published | Links |
|---------|-----|------|-----------|-------|
| sentient-rs | [v0.2.3](https://github.com/High-Velocity-Decisions/sentient-rs/releases/tag/v0.2.3) | Stable | 2026-07-09 | [Release notes](https://github.com/High-Velocity-Decisions/sentient-rs/releases/tag/v0.2.3) · [Changelog](https://github.com/High-Velocity-Decisions/sentient-rs/blob/main/CHANGELOG.md) |
| Yocto image (qemuarm64) | [20260713.29261567707](https://github.com/High-Velocity-Decisions/sentient-rs/releases/tag/20260713.29261567707) | Pre-release | 2026-07-13 | [Release notes](https://github.com/High-Velocity-Decisions/sentient-rs/releases/tag/20260713.29261567707) |
| <!-- next release --> | | | | |

> **Cosign artifact verification:**
> ```sh
> cosign verify-blob \
>   --certificate sentient-agent-aarch64.cert \
>   --signature  sentient-agent-aarch64.sig \
>   sentient-agent-aarch64-unknown-linux-gnu
> ```

---

## Risks & Blockers

| Risk / Blocker | Severity | Mitigation | Owner |
|----------------|----------|------------|-------|
| Serial port hard-exit on Pi ([#172](https://github.com/High-Velocity-Decisions/sentient-rs/issues/172)) | High | Tracked; blocks production Yocto image | @<!-- user --> |
| | | | |

---

## Next Week

- [ ] <!-- [#NNN] Priority action item 1 -->
- [ ] <!-- [#NNN] Priority action item 2 -->
- [ ] <!-- [#NNN] Priority action item 3 -->
