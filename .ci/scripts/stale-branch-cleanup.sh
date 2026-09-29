#!/usr/bin/env bash
# .ci/scripts/stale-branch-cleanup.sh — find and clean up stale branches in the repository.
#
# The routine, in code (branch-cleanup.yml runs it daily):
#   1. Identify branches where the associated PR is closed and the branch is older than today.
#   2. For branches older than 48 hours with no recent pushes, delete them.
#   3. For branches older than today but less than 48 hours old, or with recent pushes, leave
#      them in place.
#   4. Post a summary of deleted branches. If no branches were deleted, confirm briefly.
#
# Interpretation notes (deliberately conservative):
#   - "associated PR is closed" = the branch has at least one PR and NO open PR. A branch with no
#     PR at all is NOT touched (it never entered the PR lifecycle, so nothing marks it finished).
#   - Branch age / "recent pushes" both read the TIP COMMIT's committer date — one datum drives
#     both rules. (GitHub does not expose a per-branch push timestamp; a push of old commits would
#     read as old, but such a branch has all-closed PRs and remains restorable — see below.)
#   - "older than today" = tip committed before today's 00:00 UTC.
#   - The default branch and protected branches are never candidates, regardless of PR state.
#   - Every deleted branch had a (now-closed) PR, so it is restorable in one click from that PR's
#     page ("Restore branch") — deletion here is reversible by design.
#
# Env contract:
#   REPO      — owner/repo (from the workflow's github.repository)
#   GH_TOKEN  — token with contents:write + pull-requests:read (the workflow's GITHUB_TOKEN)
#   DRY_RUN   — "true" = report what would be deleted, delete nothing
#   GITHUB_STEP_SUMMARY — where the summary lands (falls back to stdout when unset, for local runs)
set -euo pipefail

: "${REPO:?REPO (owner/repo) must be set}"
: "${GH_TOKEN:?GH_TOKEN must be set}"
DRY_RUN="${DRY_RUN:-false}"
SUMMARY="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
OWNER="${REPO%%/*}"

NOW=$(date -u +%s)
TODAY_MIDNIGHT=$(date -u -d "$(date -u +%F) 00:00:00" +%s)
CUTOFF_48H=$((NOW - 48*3600))

DEFAULT_BRANCH=$(gh api "repos/$REPO" --jq .default_branch)

# One paginated call for the protected set (avoids a per-branch API round-trip AND per-branch
# URL-encoding of names in the path).
PROTECTED=$(gh api --paginate "repos/$REPO/branches?per_page=100" --jq '.[] | select(.protected) | .name')
is_protected() { grep -qxF "$1" <<<"$PROTECTED"; }

# All remote branch tips + committer dates in one blobless fetch (commit objects only — cheap).
git fetch --quiet --prune --filter=blob:none origin '+refs/heads/*:refs/remotes/origin/*'

deleted=()   # "branch|age_h|prs"
kept=()      # "branch|age_h|reason"
failed=()

age_h() { echo $(( (NOW - $1) / 3600 )); }

while IFS=' ' read -r ts ref; do
  branch="${ref#refs/remotes/origin/}"
  case "$branch" in HEAD|"$DEFAULT_BRANCH") continue ;; esac

  # Rule 1 gate a: older than today (tip committed before 00:00 UTC today).
  if [ "$ts" -ge "$TODAY_MIDNIGHT" ]; then
    continue
  fi

  # Rule 1 gate b: has an associated PR, and every PR is closed (none open).
  # gh api -f on GET sends proper, URL-encoded query params (branch names can carry slashes).
  prs=$(gh api -X GET "repos/$REPO/pulls" -f head="$OWNER:$branch" -f state=all -f per_page=100 \
          --jq '[.[] | {number, state}]')
  total=$(jq 'length' <<<"$prs")
  open=$(jq '[.[] | select(.state == "open")] | length' <<<"$prs")
  if [ "$total" -eq 0 ]; then
    continue   # never had a PR — out of scope, leave it alone
  fi
  if [ "$open" -gt 0 ]; then
    kept+=("$branch|$(age_h "$ts")|open PR")
    continue
  fi
  pr_list=$(jq -r '[.[] | "#\(.number)"] | join(" ")' <<<"$prs")

  if is_protected "$branch"; then
    kept+=("$branch|$(age_h "$ts")|protected")
    continue
  fi

  # Rules 2/3: >=48h stale -> delete; younger (or recently pushed) -> leave in place.
  if [ "$ts" -gt "$CUTOFF_48H" ]; then
    kept+=("$branch|$(age_h "$ts")|<48h old / recent push")
    continue
  fi

  if [ "$DRY_RUN" = "true" ]; then
    echo "DRY RUN: would delete $branch (last commit $(age_h "$ts")h ago, PRs: $pr_list)"
    deleted+=("$branch|$(age_h "$ts")|$pr_list")
  elif gh api -X DELETE "repos/$REPO/git/refs/heads/$branch" >/dev/null; then
    echo "deleted $branch (last commit $(age_h "$ts")h ago, PRs: $pr_list)"
    deleted+=("$branch|$(age_h "$ts")|$pr_list")
  else
    echo "::warning::failed to delete $branch — skipping (protected or ref moved?)"
    failed+=("$branch")
  fi
done < <(git for-each-ref 'refs/remotes/origin' --format='%(committerdate:unix) %(refname)')

# ---- Summary (rule 4) --------------------------------------------------------------------------
{
  echo "## Stale-branch cleanup"
  echo
  if [ "$DRY_RUN" = "true" ]; then
    echo "**DRY RUN — nothing was actually deleted.**"
    echo
  fi
  if [ "${#deleted[@]}" -eq 0 ]; then
    echo "No stale branches deleted — nothing met the criteria (closed PR + older than today + ≥48h since last commit)."
  else
    echo "${#deleted[@]} stale branch(es) deleted (all had only closed PRs, so each is restorable from its PR page):"
    echo
    echo "| branch | last commit | closed PR(s) |"
    echo "|---|---|---|"
    for row in "${deleted[@]}"; do
      IFS='|' read -r b a p <<<"$row"
      echo "| \`$b\` | ${a}h ago | $p |"
    done
  fi
  if [ "${#kept[@]}" -gt 0 ]; then
    echo
    echo "<details><summary>${#kept[@]} candidate(s) left in place</summary>"
    echo
    echo "| branch | last commit | reason |"
    echo "|---|---|---|"
    for row in "${kept[@]}"; do
      IFS='|' read -r b a r <<<"$row"
      echo "| \`$b\` | ${a}h ago | $r |"
    done
    echo
    echo "</details>"
  fi
  if [ "${#failed[@]}" -gt 0 ]; then
    echo
    echo "⚠️ Failed to delete: ${failed[*]}"
  fi
} >> "$SUMMARY"

# A failed deletion should surface as a red run (the branch met every criterion but the API
# refused), while an empty candidate list is a healthy green.
[ "${#failed[@]}" -eq 0 ]
