# Security policy

## What this project is, in security terms

Mzizi is a **compiler and a research prototype**. It is not a service, it handles no user
data, it makes no network connections, it has no accounts, no sessions and no secrets. It
has never been released: there is no published crate, no binary distribution, and no
version anyone is running in production. Treating it as though it were a web application
would produce a policy that is mostly theatre, so this one does not.

What it does do — the only thing that makes it security-relevant at all — is **read
untrusted input**. An agent or a person points `mz` at a `.mz` file that may have been
written by anyone, and the compiler lexes it, parses it, and lowers it to an IR. Everything
interesting is downstream of that.

It is also worth stating what the compiler does **not** do today, because it bounds the
threat model: it does not execute anything. Contract bodies parse but are not evaluated,
there is no lowering to Rust, no code generation, no runtime and no rendering. Compiling a
`.mz` file today runs no code from that file. When contract evaluation and lowering land,
that changes, and this document changes with them.

## Supported versions

**`main`, and only `main`.** There are no releases and no tags, so there is nothing to
backport to. Fixes land on `main`; if you are running Mzizi you are running a commit, and
the answer to "is my version affected" is "check whether the fix is an ancestor of it".

## What we consider a vulnerability

In rough order of how much we want to hear about it.

### Crashes and hangs on untrusted input

The compiler must terminate with a diagnostic on **every** input, including deliberately
malformed, adversarial or nonsensical ones. Any of the following is a bug worth reporting,
regardless of how mangled the input that triggers it:

- **A panic** — `unwrap`/`expect` on attacker-influenced data, slice index out of bounds,
  integer overflow in a span or offset, an `unreachable!` that is reachable.
- **Stack exhaustion.** The parser is hand-written recursive descent, so deeply nested
  input is the obvious attack on it. A crash from nesting depth is in scope.
- **Non-termination or unbounded memory.** The parser recovers from errors and resynchronises
  per line; recovery loops that fail to consume input are a classic way for a recovering
  parser to spin forever or allocate without bound on a tiny file. A hundred-byte file that
  hangs `mz check` is a good report.

These matter more than they would for a compiler people invoke by hand, because the whole
point of the design is a machine driving `mz` in a tight loop, often over code the machine
itself just generated and nobody has read.

### Corruption of the agent protocol

`mz check --agent` emits NDJSON, one diagnostic per line, and an agent consumes it as
instructions about what to change. That makes the framing of that output a trust boundary
of its own. In scope:

- Source content that escapes the JSON string encoding, or that injects a newline and so
  forges an extra diagnostic line — an attacker-controlled identifier that causes the agent
  to be handed a fabricated `fix` is the sharp version of this.
- A `fix` payload with `confidence: "exact"` that is not, in fact, a safe mechanical
  substitution. `mz fix` does not exist yet, but RFC-0001 §4.3 specifies it as applying
  every `exact` fix in one shot without review, so the tag is a promise about what may be
  applied unattended.
- Any path in a diagnostic that escapes the project root, or any write by a future
  `mz fix` / `mz build` outside the directory it was pointed at.

### Unsoundness

A program that `mz check` accepts but that violates a guarantee the language claims. Today
the claims are narrow — there is no type checker to be unsound, and contracts are not
evaluated — so this category is small and will grow. What exists now:

- **IR identity collisions.** The IR is content-addressed and RFC-0004 §4.1 leans on that
  for cross-repository references it calls tamper-evident. Two structurally different
  programs that produce the same root hash would break that, whether through an ambiguity
  in the canonical serialization (RFC-0003 §2.1) or a defect in the SHA-256 implementation.
- **The SHA-256 implementation itself.** It is hand-rolled and verified against the NIST
  vectors; a deviation from the standard is in scope and we want it. Note RFC-0003 §2.2's
  recorded limit: this is a content-identity function, not a security boundary — collision
  resistance matters, side channels do not. If the hash ever becomes a trust boundary
  (signed releases, a shared public store), the RFC commits to swapping in a reviewed crate,
  and a report arguing that moment has arrived is a legitimate report.
- **`mz outline` losing or misstating an interface**, since RFC-0001 §4.5 makes outline the
  representation an agent loads instead of reading a dependency's source. An outline that
  omits a capability declaration would let a reader conclude a component cannot reach the
  network when it can.

### Supply chain and repository integrity

- Anything that lets a fork, a pull request, or an untrusted workflow obtain a token or
  write where it should not. The benchmark dispatch workflow holds a token and is guarded
  on `github.repository`; a bypass of that guard is a real finding.
- A dependency being introduced into `compiler/` at all is worth a second look. The crate
  has an empty `[dependencies]` table on purpose.

## What is out of scope

- **Secrets or data exposure.** There are none to expose.
- **Denial of service against your own build.** `mz` is a local tool you invoke on your own
  files; a slow compile on a pathological input you wrote is a performance bug (file an
  issue — compile speed is a Phase 0 metric), not a vulnerability. The crash and hang
  categories above are about inputs from elsewhere.
- **"The compiler is not a sandbox."** It is not trying to be. Compiling untrusted `.mz`
  source must not crash `mz`; it is not a claim that any future lowered output is safe to
  run.
- **Missing hardening on an unreleased prototype** — no signed releases, no reproducible
  builds, no SBOM. All true, all known, none of them a vulnerability report. They become
  relevant at first release and not before.
- **Vulnerabilities in the design as designed.** If you think an RFC's design is wrong,
  that is an RFC discussion, not an advisory — open an issue or propose an RFC per
  [`CONTRIBUTING.md`](./CONTRIBUTING.md).

## How to report

**Use GitHub's private vulnerability reporting**, which is enabled on this repository:

> <https://github.com/mzizi-dev/mzizi/security/advisories/new>

That opens a private advisory visible only to maintainers. Include the `.mz` input (or a
generator for it), the exact `mz` invocation, what happened, and what you expected.

Do not open a public issue for anything in the categories above until it has been fixed or
we have agreed with you that it is public. For anything **not** in those categories — an
ordinary parser bug on input you wrote yourself, a bad diagnostic, a wrong outline — a
public issue is the right place and is more useful there.

If GitHub private reporting is unavailable to you, say so in a public issue **without the
details** and a maintainer will arrange another channel.

## What to expect

Honest expectations, because a policy that promises a response time nobody is staffed to
meet is worse than one that does not:

- This is a research project with no on-call rotation. We aim to acknowledge a report within
  a week and will tell you plainly if it will take longer.
- There is **no bug bounty** and no payment. There is credit in the advisory and in the
  commit that fixes it, unless you would rather not be named.
- Nothing here is deployed anywhere, so there is no emergency patch path and no user
  population to protect on a clock. Severity here is mostly about correctness, which is
  exactly the framing this project can act on.

## Disclosure and the public test suite

When a report is fixed, both the **fix and its regression test are public**, permanently and
without exception. RFC-0004 §2 commits the entire correctness suite to the public
repository, and §5 states the position that makes this policy coherent:

> Security comes from the public suite, the fuzzing, the `-D warnings` gate, and review —
> all of which stay public.

Two narrow things may be withheld, both bounded by that RFC:

- **A seed corpus** of inputs that once crashed the compiler may be kept private, because a
  curated crash corpus is a fuzzing head start against forks that have not taken the patch.
  This applies only to the seed corpus — never to the fix or to the regression test for it.
- **An embargoed test** stays private for the length of the embargo and then moves to the
  public suite. RFC-0004 §5 is explicit that "a test still private after its embargo has
  expired is a bug in the process".

Neither of these mechanisms exists yet — there is no private repository today, and RFC-0004
§6 deliberately defers creating one. Until then, every test in this project is public, which
is the correct state.
