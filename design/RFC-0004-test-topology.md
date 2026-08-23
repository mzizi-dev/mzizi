# RFC-0004 — Test topology: what stays public, what goes private, and why

**Status:** draft; the public half of the mechanism is implemented in this PR
**Author:** the machine author (Claude)
**Scope:** which test infrastructure lives in the open repository, which lives in a private
one, the dependency-direction rule that keeps the project forkable, and the reporting
mechanism between them.

---

## 1. Correcting the premise first

The prompt for this RFC was SQLite: public-domain code, proprietary test harness (TH3), and
the intuition that this makes a project harder to attack. Two corrections, because building
on the wrong reason would mean under-investing in real security.

**SQLite's split is commercial, not defensive.** TH3 is proprietary because it is sold, and
because it carries DO-178B avionics certification evidence with its own audit and licensing
constraints. SQLite simultaneously ships a very large _public_ test suite; the public
project is not under-tested in the open.

**Private tests are close to worthless as a security control.** An attacker has the source,
a fuzzer, and symbolic execution. A test suite mostly documents what a project _already
handles correctly_ — it is one of the least useful artifacts an attacker could be handed.
Meanwhile hiding it costs real things: contributors cannot verify their own work, downstream
users cannot confirm the guarantees, and coverage gaps become invisible to the people best
placed to point them out. Security that depends on the mechanism being secret is the
failure mode Kerckhoffs named in 1883.

So: **no test is private because privacy makes it stronger.** Four reasons are legitimate,
and one of them is decisive for this project.

### 1.1 Benchmark contamination — the reason that actually applies

Phase 0's entire claim rests on a benchmark: an agent authors corpus components in Mzizi
versus raw Dioxus/Leptos, measured on tokens, iterations to clean compile, and defect rate.
If the task set and its expected outputs are public, they will be scraped into training
data. After that the benchmark measures **memorisation**, not the language, and it will
report a flattering number for exactly the wrong reason.

This is not hypothetical — it is what happened to GSM8K, HumanEval, and most public
LLM benchmarks. It is also the one failure that cannot be detected from inside: a
contaminated benchmark looks like a successful one.

The charter gives Phase 0 a kill criterion. A kill criterion that cannot fire is not a
criterion. **A held-out set is therefore a correctness requirement, not a secrecy
preference.**

### 1.2 The other three

- **Adversarial corpora.** A curated set of inputs that once crashed the compiler is a
  fuzzing head start against forks that have not taken the patch. Narrow, real, and it
  applies only to the seed corpus — never to the fix or the regression test for it, both of
  which belong in public.
- **Embargo windows.** A test for an undisclosed vulnerability stays private until
  disclosure, then moves to the public suite. Standard practice, and the private location is
  temporary by design.
- **Commercial and certification.** If a certification evidence package is ever produced,
  it inherits its own constraints. Not applicable today; named so the boundary is already
  drawn if it becomes applicable.

## 2. The split

**Public** — `nyuchi/mzizi-tools` today, `mzizi-dev` later:

- The language, compiler, primitives, RFCs. All of it.
- The **entire correctness suite**: unit tests, contract tests, the parse gates, the
  measured IR properties. Everything in `mzizi-lang/compiler/tests/` today stays public,
  permanently.
- The **benchmark harness** — the runner, the metric definitions, the scoring code. How the
  measurement works is public even though one input to it is not.
- A published **fixture format**, so anyone can write their own task set and run it.

**Private** — a separate repository:

- The **held-out benchmark task set** and its expected outputs (§1.1).
- Fuzzing seed corpora (§1.2).
- Embargoed security tests, for the length of the embargo only.

The ratio matters and should be stated plainly: this is a small private annex to a large
public suite, not a public shell around private testing. If someone cannot verify Mzizi's
correctness from the public repository alone, the split has been drawn wrongly.

## 3. The dependency-direction rule

This is the part projects get wrong, and getting it wrong quietly kills the open project.

> **Private consumes public. Public never consumes private.**

Public CI must be completely self-contained: a fork with no secrets, no access, and no
relationship to the private repository must be able to run the full public suite and get a
green result. The moment public CI needs a private token, every outside contributor's CI
fails, every fork is broken, and the project is open source in name only.

So the private repository is a **downstream consumer**. It pins a public commit, builds it,
runs the held-out suite against it, and reports a result back.

Three consequences that are non-negotiable:

1. **A missing private result is `neutral`, never `failure`.** On a fork PR, on a Dependabot
   PR, whenever the secret is absent — the check reports "skipped: held-out suite not
   available for this event". A red X that an outside contributor is structurally unable to
   turn green is a wall, not a gate.
2. **The private result is advisory to outsiders, blocking only for maintainers.** Branch
   protection may require it on `main`; it must not be required to _propose_ a change.
3. **A private failure must be reportable in public without leaking the test.** The check
   reports the _shape_ of the failure — which metric regressed, by how much, against which
   component — not the task input. A maintainer with access reads the detail; everyone else
   sees enough to know something real broke.

## 4. Mechanism

The public side, implemented in this PR —
`.github/workflows/mzizi-lang-benchmark-dispatch.yml`:

- On a push to `main` touching `mzizi-lang/**` (or on manual dispatch), it POSTs a
  `repository_dispatch` to the private repository carrying the commit SHA, ref, source
  repository and run id. That is the entire payload: four public facts.
- It is **inert unless two things are configured** — the `MZIZI_HELDOUT_REPO` repository
  variable (`owner/name`) and the `MZIZI_DISPATCH_TOKEN` secret (`contents:write` on the
  private repository, nothing else). Neither exists yet, so today the job runs and reports
  "not configured". That is the intended steady state, not a failure.
- It is additionally guarded on `github.repository`, so a fork skips it even if a secret of
  the same name happens to exist there.
- It **never** fails the build. A dispatch error is a `::warning::`, because a missed
  notification does not mean the commit is bad, and a red `main` that nobody can act on only
  teaches people to ignore the signal.

That last point needs a companion on the other side, or a dropped webhook becomes a silent
coverage gap: the private runner must also **poll** for public commits it has not yet
measured. The dispatch is a latency optimisation; the poll is the correctness guarantee.

The private side (to be created in `mzizi-dev` — not in this PR; see §6 for when, and why
not yet):

1. Receives the dispatch — or notices the commit on its own poll — checks out the public
   repo at that SHA, and builds `mz`.
2. Runs the held-out suite through the **public** harness, so the measurement path is the
   audited one.
3. Posts a check run back to the public commit via the Checks API, with a
   `MZIZI_CHECKS_TOKEN` scoped to `checks:write` on the public repository and nothing else.
4. Uploads full detail as a private artifact; the public check carries only the summary.

### 4.1 What the content-addressed IR contributes here

The IR (RFC-0003) makes the cross-repository reference precise in a way a file path cannot.
A private test can pin an **exact root hash**:

```text
button @ 1d19dec58bc9  must satisfy: every button_size height at_least 48
```

That reference is tamper-evident and unambiguous across repository boundaries, and it does
not leak the test — a hash reveals nothing about the assertion attached to it. It also means
a private suite can state precisely which version of a component it verified, which a
branch name or a line number cannot.

## 5. What this deliberately does not claim

- It does not make Mzizi harder to attack. Security comes from the public suite, the
  fuzzing, the `-D warnings` gate, and review — all of which stay public.
- It does not hide the compiler's behaviour. Every assertion about what Mzizi _does_ is
  public; only a held-out measurement of _how well an agent uses it_ is not.
- It is not permanent for security tests. Embargoed tests move to public at disclosure, and
  a test still private after its embargo has expired is a bug in the process.

## 6. Open questions

1. **~~Where the private repository lives.~~** Settled: **`mzizi-dev`**, the dedicated
   Foundation-governed Mzizi org, which already exists — earlier drafts of this RFC said no
   such org existed, then named `bundu-labs`; both were wrong. `bundu-labs` is the
   Foundation's general org and carries unrelated work, so it is the wrong boundary here.

   A dedicated org is the better home for exactly the reason this RFC exists. The public
   repository and the private held-out repository both live in `mzizi-dev`, so read access
   to the held-out set is governed by one org's membership — which is the Mzizi maintainer
   set and nothing wider. Putting the private half in a general-purpose org would tie the
   most access-sensitive artifact in the project to a membership list maintained for
   unrelated reasons, and §1.1's whole argument is that the held-out set stops being worth
   anything the moment it leaks.

   **It should not be created yet**, and the reason is a design constraint rather than a
   scheduling one. Its whole job is to run the _public_ harness against a private input, so
   if it exists before the harness does, the harness ends up shaped around the private
   runner — the exact inversion §3 forbids. There is also nothing to put in it: the held-out
   task set does not exist, and the harness mechanics are still open (charter §7).

   And an empty private repository is an attractive nuisance. Today every test in this
   project is public, which is correct. The moment the repository exists, the marginal cost
   of filing a test there drops to zero; each individual "this one is easier to keep
   private" is defensible, and the aggregate is the public-shell-around-private-testing
   outcome §2 rules out. Creating it only when a held-out task set needs somewhere to live
   means every file in it has to justify itself against a rule that already exists.

   **Trigger:** the first held-out task. Not before.

2. **Held-out set rotation.** A held-out set leaks slowly through published results. It
   needs a refresh policy — probably a fraction rotated per reported run.
3. **Third-party verification.** If an outside party needs to reproduce a benchmark claim,
   there has to be a path: most likely a time-limited grant to the private set under an
   agreement not to publish it. Unsolved, and worth solving before any number is published.
