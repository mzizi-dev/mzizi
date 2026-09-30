#!/usr/bin/env bash
# Does a pull request need a CHANGELOG.md entry, and does it have one?
# The rule is AGENTS.md, "Changelog".
#
#   changelog-entry.sh <base sha> <head sha>
#
# Exit 0 when the PR touches CHANGELOG.md, when every file it changes is exempt, or when
# NO_CHANGELOG=true (the `no-changelog` label). Exit 1 otherwise, naming the files that need
# the entry. Exit 2 on a usage or git error. Exempt paths: anything under `.github/`, and
# the lint configuration at the repo root.
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: $0 <base sha> <head sha>" >&2; exit 2; }
base=$1 head=$2

if [[ ${NO_CHANGELOG:-false} == true ]]; then
  echo "changelog: skipped, the pull request is labelled no-changelog"
  exit 0
fi

# The files the pull request changes: head against its merge base with the base branch.
changed=$(git diff --name-only "$base...$head") || exit 2

if grep -qx 'CHANGELOG.md' <<<"$changed"; then
  echo "changelog: CHANGELOG.md is updated"
  exit 0
fi

needs=$(grep -Ev '^(\.github/|\.prettierrc$|\.prettierignore$|\.markdownlint\.jsonc$|\.yamllint\.yaml$)' \
  <<<"$changed" | grep -v '^$' || true)

if [[ -z $needs ]]; then
  echo "changelog: not needed, every changed file is CI or lint configuration"
  exit 0
fi

echo "changelog: this pull request changes files that need a CHANGELOG.md entry under"
echo "## [Unreleased] (AGENTS.md, \"Changelog\"), and CHANGELOG.md is not changed:"
while IFS= read -r f; do echo "  $f"; done <<<"$needs"
echo "If it genuinely has nothing to record, label it no-changelog."
exit 1
