#!/usr/bin/env bash
# Point this clone's git hooks at .githooks/ (the pre-commit hook: a CHANGELOG.md entry on
# every branch, and cargo fmt). Run it once per clone, from anywhere inside it.
#
# core.hooksPath is relative, so git resolves it against the root of whichever working
# tree runs the commit: every worktree of the clone uses its own checkout's .githooks/.
# The setting lives in the clone's shared config, so one run covers all its worktrees,
# including worktrees other people or agents have open on the same clone. A worktree on a
# branch older than .githooks/ then runs no hooks. Hooks already in the clone's hooks
# directory stop running too; the script names any it finds, and what it replaced.
#
# The hook that runs is the checked-out branch's .githooks/pre-commit, so a commit on
# someone else's branch (a fork's pull request included) runs their code. Read changes
# under .githooks/ before committing there, or use `git commit --no-verify`.
set -euo pipefail

top=$(git rev-parse --show-toplevel)
if [[ ! -f $top/.githooks/pre-commit ]]; then
  echo "install-hooks: $top/.githooks/pre-commit is missing; run this inside the mzizi repo" >&2
  exit 2
fi

old=$(git -C "$top" config --get core.hooksPath || true)
if [[ -n $old && $old != .githooks ]]; then
  echo "install-hooks: replacing core.hooksPath = $old"
fi

common=$(git -C "$top" rev-parse --path-format=absolute --git-common-dir)
for h in "$common"/hooks/*; do
  [[ -f $h && -x $h && $h != *.sample ]] || continue
  echo "install-hooks: $h will no longer run (core.hooksPath takes precedence)"
done

git -C "$top" config core.hooksPath .githooks
echo "install-hooks: core.hooksPath = $(git -C "$top" config --get core.hooksPath)"
