#!/usr/bin/env bash
# Does a pull request need a CHANGELOG.md entry, and does it have one?
# The rule is AGENTS.md, "Changelog".
#
#   changelog-entry.sh <base sha> <head sha>
#
# Every pull request needs an entry (owner rule, 2026-10-07: "Change log in every PR").
# Exit 0 when the PR touches CHANGELOG.md, when NO_CHANGELOG=true (the `no-changelog`
# label, a person's explicit call), or when PR_AUTHOR is dependabot[bot] (its version bumps
# are listed by GitHub's generated release notes). Exit 1 otherwise, naming the files the
# PR changes. Exit 2 on a usage or git error.
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <base sha> <head sha>" >&2; exit 2; }
base=$1 head=$2

if [[ ${NO_CHANGELOG:-false} == true ]]; then
  echo "changelog: skipped, the pull request is labelled no-changelog"
  exit 0
fi

if [[ ${PR_AUTHOR:-} == 'dependabot[bot]' ]]; then
  echo "changelog: skipped, a Dependabot version bump"
  exit 0
fi

# The files the pull request changes: head against its merge base with the base branch.
changed=$(git diff --name-only "$base...$head") || exit 2

if grep -qx 'CHANGELOG.md' <<<"$changed"; then
  echo "changelog: CHANGELOG.md is updated"
  exit 0
fi

needs=$(grep -v '^$' <<<"$changed" || true)

if [[ -z $needs ]]; then
  echo "changelog: not needed, the pull request changes no files"
  exit 0
fi

echo "changelog: every pull request needs a CHANGELOG.md entry under"
echo "## [Unreleased] (AGENTS.md, \"Changelog\"), and CHANGELOG.md is not changed. It changes:"
while IFS= read -r f; do echo "  $f"; done <<<"$needs"
echo "If it genuinely has nothing to record, label it no-changelog."
exit 1
