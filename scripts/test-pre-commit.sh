#!/usr/bin/env bash
# Tests .githooks/pre-commit and scripts/install-hooks.sh in a throwaway repository under
# $TMPDIR, never in this clone (it does not change this clone's git config).
#
#   scripts/test-pre-commit.sh
#
# Needs git and cargo (with rustfmt); no network. Exit 0 when every case behaves.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/mz-hook-test.XXXXXX")
trap 'rm -rf "$work"' EXIT

fails=0
pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; fails=$((fails + 1)); }

# expect <ok|fail> <description> <command...>: run a commit and check its outcome.
expect() {
  local want=$1 what=$2
  shift 2
  if "$@" >"$work/out" 2>&1; then got=ok; else got=fail; fi
  # A refusal has to come from the hook, not from git failing for some other reason.
  if [[ $got == fail ]] && ! grep -q '^pre-commit: ' "$work/out"; then got=error; fi
  if [[ $got == "$want" ]]; then pass "$what"; else
    fail "$what (wanted $want, got $got)"
    sed 's/^/       /' "$work/out"
  fi
}

repo=$work/repo
mkdir -p "$repo/.githooks" "$repo/scripts" "$repo/compiler/src"
cp "$here/.githooks/pre-commit" "$repo/.githooks/pre-commit"
cp "$here/scripts/install-hooks.sh" "$repo/scripts/install-hooks.sh"
chmod +x "$repo/.githooks/pre-commit" "$repo/scripts/install-hooks.sh"
cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = ["compiler"]
resolver = "2"
EOF
cat >"$repo/compiler/Cargo.toml" <<'EOF'
[package]
name = "hooktest"
version = "0.0.0"
edition = "2021"
EOF
printf 'fn main() {}\n' >"$repo/compiler/src/main.rs"
printf '# Changelog\n\n## [Unreleased]\n' >"$repo/CHANGELOG.md"

g() { git -C "$repo" -c user.name=hook-test -c user.email=hook-test@example.invalid "$@"; }
g init -q -b main
g add -A
g commit -q --no-verify -m base
g update-ref refs/remotes/origin/staging HEAD

(cd "$repo" && scripts/install-hooks.sh >/dev/null)
if [[ $(g config --get core.hooksPath) == .githooks ]]; then
  pass "install-hooks.sh sets core.hooksPath to .githooks"
else
  fail "install-hooks.sh did not set core.hooksPath"
fi

g checkout -q -b feature

echo one >"$repo/notes.txt"
g add notes.txt
expect fail "a commit without CHANGELOG.md on a fresh branch is refused" g commit -q -m no-entry

expect ok "MZ_NO_CHANGELOG=1 lets it through" env MZ_NO_CHANGELOG=1 \
  git -C "$repo" -c user.name=hook-test -c user.email=hook-test@example.invalid commit -q -m skip
if [[ $(g log -1 --format=%s) == skip ]]; then g reset -q --soft HEAD~1; fi

printf -- '- an entry\n' >>"$repo/CHANGELOG.md"
g add CHANGELOG.md
expect ok "a commit that stages CHANGELOG.md passes" g commit -q -m with-entry

echo two >>"$repo/notes.txt"
g add notes.txt
expect ok "a later commit passes, because the branch already touched CHANGELOG.md" \
  g commit -q -m later

# The repo's real layout: main carries release commits that touch CHANGELOG.md and that
# staging does not have. A branch cut from main must not be credited with them.
g checkout -q -b release main
printf -- '- released\n' >>"$repo/CHANGELOG.md"
g commit -q --no-verify -am release
g update-ref refs/remotes/origin/main HEAD
g checkout -q -b from-main refs/remotes/origin/main
echo x >"$repo/from-main.txt"
g add from-main.txt
expect fail "a branch cut from a main that has diverged from staging still needs an entry" \
  g commit -q -m no-entry-from-main
g reset -q --hard

g checkout -q -b other refs/remotes/origin/staging
g update-ref -d refs/remotes/origin/staging
g update-ref refs/remotes/origin/main HEAD
echo three >"$repo/more.txt"
g add more.txt
expect fail "with only origin/main, a branch without an entry is still refused" \
  g commit -q -m no-entry-main
g update-ref -d refs/remotes/origin/main
expect fail "with neither ref, the staged changes alone decide (refused)" \
  g commit -q -m no-entry-noref
printf -- '- another\n' >>"$repo/CHANGELOG.md"
g add CHANGELOG.md
expect ok "with neither ref, staging CHANGELOG.md passes" g commit -q -m entry-noref

wt=$work/wt
g worktree add -q -b in-worktree "$wt" main
echo four >"$wt/wt.txt"
git -C "$wt" add wt.txt
expect fail "a linked worktree runs the hook too (refused without an entry)" \
  git -C "$wt" -c user.name=hook-test -c user.email=hook-test@example.invalid commit -q -m wt

if command -v cargo >/dev/null 2>&1; then
  printf 'fn   main( ) {   }\n' >"$repo/compiler/src/main.rs"
  printf -- '- rust\n' >>"$repo/CHANGELOG.md"
  g add -A
  expect fail "unformatted Rust under compiler/ is refused by cargo fmt" g commit -q -m bad-fmt
  printf '// formatted\nfn main() {}\n' >"$repo/compiler/src/main.rs"
  g add -A
  expect ok "formatted Rust under compiler/ passes" g commit -q -m good-fmt
else
  fail "cargo is not on PATH, so the rustfmt cases did not run"
fi

if ((fails)); then
  echo "$fails case(s) failed"
  exit 1
fi
echo "every case passed"
