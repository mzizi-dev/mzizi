#!/usr/bin/env python3
"""Release notes from CHANGELOG.md (AGENTS.md, "Changelog").

Usage: release_notes.py <previous-ref> <ref>

Prints, as Markdown, every CHANGELOG.md entry present at <ref> but not at
<previous-ref>, under the `### ...` heading it sits under at <ref>. An entry is a
top-level bullet (`- ...`) with its continuation lines. Comparing entries rather
than lines means an entry moved from `[Unreleased]` into a dated section is not
reported twice, and the notes do not depend on anyone having moved anything. Entries
compare with whitespace collapsed, so a reflow is not a change; an entry whose words
were edited counts as new. Entries are counted, so an entry repeated word for word in
a later release is still reported. With an empty <previous-ref> (the first release),
every `[Unreleased]` entry is new.

The CHANGELOG_URL environment variable, if set, is the link the notes give to
CHANGELOG.md (a release page needs an absolute URL). Exit 0 with a one-line notice when
nothing is new; exit 1 when CHANGELOG.md cannot be read at either ref.
"""
import os
import subprocess
import sys
from collections import Counter


def show(ref: str) -> str:
    if not ref:
        return ""
    out = subprocess.run(
        ["git", "show", f"{ref}:CHANGELOG.md"], capture_output=True, text=True
    )
    if out.returncode != 0:
        # A wrong note is worse than a failed release run: the run can be re-tried.
        sys.exit(f"release_notes: cannot read CHANGELOG.md at {ref}: {out.stderr.strip()}")
    return out.stdout


def entries(text: str, unreleased_only: bool = False):
    """Yield (heading, entry) pairs in file order."""
    heading, section, entry = "", "", []
    def flush():
        if entry:
            yield heading, "\n".join(entry).rstrip()
    for line in text.splitlines():
        if line.startswith("## "):
            yield from flush(); entry = []
            section, heading = line, ""
        elif line.startswith("### "):
            yield from flush(); entry = []
            heading = line[4:].strip()
        elif line.startswith("- "):
            yield from flush()
            entry = [line] if (section.startswith("## [Unreleased]") or not unreleased_only) and section else []
        elif entry and (line.startswith("  ") or line == ""):
            entry.append(line)
        else:
            yield from flush(); entry = []
    yield from flush()


def norm(entry: str) -> str:
    return " ".join(entry.split())


def main() -> int:
    if len(sys.argv) != 3:
        sys.exit("usage: release_notes.py <previous-ref> <ref>")
    prev, ref = sys.argv[1], sys.argv[2]
    old = Counter(norm(e) for _, e in entries(show(prev)))
    groups: dict[str, list[str]] = {}
    for heading, e in entries(show(ref), unreleased_only=not prev):
        key = norm(e)
        if old[key] > 0:
            old[key] -= 1
            continue
        groups.setdefault(heading or "Changes", []).append(e)
    if not groups:
        print("No CHANGELOG.md entries were added since the previous release.")
        return 0
    print("## What changed\n")
    url = os.environ.get("CHANGELOG_URL", "CHANGELOG.md")
    print(f"From [`CHANGELOG.md`]({url}). Nothing here is a measured result unless the entry says so.\n")
    for heading, items in groups.items():
        print(f"### {heading}\n")
        print("\n".join(items))
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
