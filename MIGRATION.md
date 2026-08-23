# Migration to the `mzizi-dev` org — plan and handoff

**Audience:** whoever (human or agent) picks this up in the new org. This file lives inside
`mzizi-lang/`, so a `git subtree split` carries it to the root of the new repository
automatically — you should be reading it there.

**State at time of writing:** `nyuchi/mzizi-tools@97cc341`. Everything described in
"what exists" is merged, CI-green, and measured.

---

## 1. Repositories to create

| Repo                  | Visibility | Contents                                                                                             | When                     |
| --------------------- | ---------- | ---------------------------------------------------------------------------------------------------- | ------------------------ |
| `mzizi-dev/framework` | public     | Everything in `mzizi-lang/` — language, compiler, primitives, RFCs, charter                          | First                    |
| `mzizi-dev/benchmark` | public     | Phase 0 harness: runner, metric definitions, scoring code, fixture format, a worked example task set | Second                   |
| `mzizi-dev/docs`      | public     | mzizi.dev content, moved out of `nyuchi/mzizi`                                                       | After `framework` exists |

Named `framework` rather than `mzizi` deliberately: it is the charter's own word ("Mzizi — A
Rust Framework for the Agentic Web"), and `mzizi-dev/mzizi` would collide with the existing
`nyuchi/mzizi` — same clone directory, same shorthand, different owners.

`mzizi-dev/roadmap` already exists. Either fold it into `framework/design/` or keep it as a
public GitHub Projects surface — but do not leave a roadmap living apart from the code it
plans. Three of the stale-doc defects fixed on 2026-08-23 existed for exactly that reason.

### 1.1 Creation spec — copy-paste ready

**Create `framework` completely empty** — no README, no LICENSE, no `.gitignore`. The
history-preserving push in §3 writes the first commit, and an initialised `main` turns that
into a merge of unrelated histories or a force-push. `benchmark` and `docs` start empty
anyway, so initialising them is harmless.

#### `mzizi-dev/framework` — public

| Field       | Value                                                                                                                                                                                                                                                         |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Description | `Mzizi — a Rust framework for the agentic web. A language, compiler and runtime designed for machine authorship: syntax tuned for small open-weight models, a content-addressed IR, and contracts the toolchain checks. A Bundu Foundation research project.` |
| Homepage    | `https://mzizi.dev`                                                                                                                                                                                                                                           |
| Topics      | `rust` `compiler` `programming-language` `language-design` `agentic-ai` `llm-tooling` `dioxus` `wasm` `bundu-foundation` `africa`                                                                                                                             |
| Licence     | Apache-2.0 — see the note below                                                                                                                                                                                                                               |
| Initialise  | **Nothing.** Empty repo                                                                                                                                                                                                                                       |

#### `mzizi-dev/benchmark` — public

| Field       | Value                                                                                                                                                                                                                                                         |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Description | `Phase 0 benchmark for the Mzizi language — the public harness, metric definitions, scoring code and fixture format. Measures tokens, iterations to a clean compile, and defect rate for agents authoring components in Mzizi versus raw Rust UI frameworks.` |
| Homepage    | —                                                                                                                                                                                                                                                             |
| Topics      | `benchmark` `llm-evaluation` `rust` `mzizi` `agentic-ai` `small-language-models` `reproducible-research` `bundu-foundation`                                                                                                                                   |
| Licence     | Apache-2.0                                                                                                                                                                                                                                                    |
| Initialise  | README                                                                                                                                                                                                                                                        |

#### `mzizi-dev/docs` — public

| Field       | Value                                                                                                                                              |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Description | `Documentation for the Mzizi framework — mzizi.dev. Language reference, RFC index, runtime guides, and the agent-facing surfaces (llms.txt, MCP).` |
| Homepage    | `https://mzizi.dev`                                                                                                                                |
| Topics      | `documentation` `mzizi` `rust` `mcp` `llms-txt` `bundu-foundation`                                                                                 |
| Licence     | Apache-2.0                                                                                                                                         |
| Initialise  | README                                                                                                                                             |

Deliberately no framework/stack topics on `docs` — the stack is whatever moves out of
`nyuchi/mzizi`, and a topic asserting Astro or Next.js before that is decided is a guess.

#### `mzizi-dev/heldout` — private, **only if** §5 shape 1 is chosen

| Field       | Value                                                                                                                                                                                                                   |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Description | `Held-out Phase 0 task set for the Mzizi benchmark. Consumes the public harness at mzizi-dev/benchmark; never consumed by it. Withheld so the benchmark measures the language rather than memorisation — see RFC-0004.` |
| Topics      | `benchmark` `mzizi` `held-out`                                                                                                                                                                                          |
| Initialise  | README                                                                                                                                                                                                                  |

The description states the dependency direction on purpose. It is the rule most likely to be
broken by someone acting in good faith, so it belongs where they will read it first.

#### Settings for every repo above

Matching the four existing Mzizi-ecosystem repos:

| Setting                   | Value                   | Why                                                                      |
| ------------------------- | ----------------------- | ------------------------------------------------------------------------ |
| Default branch            | `main`                  | House convention                                                         |
| Allow merge commits       | **yes**                 | The ecosystem convention is merge-only; history stays truthful           |
| Allow squash merging      | **no**                  | Squash discards the per-commit reasoning this project depends on         |
| Allow rebase merging      | **no**                  | Same                                                                     |
| Auto-delete head branches | yes                     |                                                                          |
| Require commit signoff    | yes                     | All four existing repos set it                                           |
| Downloads                 | off                     |                                                                          |
| Discussions               | on for `framework` only | Language design attracts questions that are not issues                   |
| Wiki                      | off                     | RFCs are the design record, in-repo and checkable                        |
| Forking                   | allowed                 | Non-negotiable — RFC-0004 §3's forkability rule is meaningless otherwise |

#### On the licence

**Apache-2.0**, matching `nyuchi/mzizi`. The explicit patent grant matters more for a
language and runtime than for a component library, and consistency inside the ecosystem is
worth something on its own.

The alternative worth knowing about is the Rust-ecosystem norm, **dual MIT OR Apache-2.0**,
whose one concrete benefit is that some downstream organisations cannot accept
Apache-2.0-only. If wide adoption by other people's companies is a goal — and per the
charter it is — dual-licensing removes a real barrier at no cost. Either is defensible;
Apache-2.0-only is the lower-effort default and dual is the more adoptable one.

Separately, and unrelated to the choice: RFC-0002 §3's discipline requires a `NOTICE` file
if any Apache-2.0 code is ever copied into the tree, and **GPL/AGPL code must never be**,
whatever licence Mzizi itself carries.

### Not repositories

- **The held-out task set is one directory, not a repo.** See §5.
- **No repo per package** (`compiler`, `primitives`, `runtime`). They version in lockstep and
  share the IR; splitting them turns a checkable invariant into a coordination problem.
- **No `rfcs` repo.** RFCs live next to the code they govern — that is why
  `compiler/tests/ir_measured.rs` can verify RFC-0003's numeric claims. Split them and the
  RFCs become documentation nobody checks.

## 2. What exists, and what it is worth

Merged in `nyuchi/mzizi-tools`, PRs #62–#69:

| Area              | State                                                                                                                                                                                                    |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `design/RFC-0001` | Syntax. Nine agent-authoring failure modes, and the construct answering each: `end <kind> <name>`, one-construct-per-intent, enum data columns, contracts, capability declarations, no ownership surface |
| `design/RFC-0002` | Design target is **small open-weight models**, not frontier ones. Runtime-as-product. 13 languages mined, with licence discipline                                                                        |
| `design/RFC-0003` | Content-addressed IR. Eight agent _reading_ barriers and the single decision that answers them                                                                                                           |
| `design/RFC-0004` | Test topology. What is public, what is withheld, and the dependency rule                                                                                                                                 |
| `compiler/`       | `mz` binary: lexer, recovering parser, agent NDJSON protocol, SHA-256, IR store, `outline`. 75 tests                                                                                                     |
| `primitives/`     | Nine primitives written in Mzizi itself                                                                                                                                                                  |
| `examples/`       | One real corpus component ported by hand                                                                                                                                                                 |

Measured, not asserted (`compiler/tests/ir_measured.rs`): structural sharing 127 shared vs
141 isolated nodes across 10 files; outline worst case 38% of source; parse+lower of the
whole set ~4.3ms; renaming every component changes zero nodes. The sharing figure is
small-corpus — the registry-scale claim is labelled a prediction, and should stay labelled
that until measured.

**Not done, and load-bearing:** contract bodies parse but are **not evaluated**. See §4.

## 3. Moving `mzizi-lang/` without losing history

Seven PRs of design history is the research record — why the syntax is what it is. A
copy-paste move discards it.

```bash
# in a clone of nyuchi/mzizi-tools
git subtree split -P mzizi-lang -b mzizi-lang-only
git push git@github.com:mzizi-dev/framework.git mzizi-lang-only:main
```

**Two things the split will not carry, because they live outside the directory.** The
self-contained-directory discipline held for source; CI necessarily lives at the repo root.

1. **The `mzizi-lang` CI job**, currently a job in `nyuchi/mzizi-tools/.github/workflows/ci.yml`.
   Recreate it as the new repo's own `ci.yml`, dropping the `mzizi-lang/` path prefix — the
   crate is now at `compiler/`. It runs, in order: `cargo fmt -- --check`,
   `cargo clippy --all-targets -- -D warnings`, `cargo test`, `mz check` on the example, and
   a loop `mz check`ing every primitive. That last pair matters: it proves the _shipped
   binary_ accepts the corpus, not merely that the test harness does.
2. **`.github/workflows/mzizi-lang-benchmark-dispatch.yml`**. Move it as-is. It resolves its
   target from the `MZIZI_HELDOUT_REPO` variable rather than a hardcoded org, so an org
   change costs nothing. Keep the two properties it was built for: inert without both
   `MZIZI_HELDOUT_REPO` and `MZIZI_DISPATCH_TOKEN`, and it can never fail the build.

Then, in `nyuchi/mzizi-tools`: delete `mzizi-lang/`, delete the `mzizi-lang` CI job and the
dispatch workflow, and leave a one-line pointer in the README. Do not leave a copy behind —
two copies of a language definition is the same defect class as the `source_code` column
this ecosystem spent PRs #204–#215 removing.

## 4. Work queue, in dependency order

### 4.1 Contract evaluation — do this first

`contract` blocks parse and are stored; nothing checks them. Until they are checked, the
charter's defect metric — "compiles cleanly but behaviourally wrong" — is measured by
hand-written Rust tests standing in for the toolchain. That makes the Phase 0 number a
measurement of something adjacent to the claim.

Scope: evaluate the assertion forms the primitives already use — `every <enum> <column>
at_least <n>`, `<enum>.<variant> <column> <value>` — over the enum data columns the parser
already produces, and emit results through the existing `Diagnostic`/NDJSON surface so an
agent needs no second protocol. `primitives/button.mz` and `primitives/alert.mz` are the
worked cases; `button.mz`'s 48px touch-floor contract is the canonical one.

This is the highest-value piece of work available. It is what turns the whole Phase 0
apparatus from plausible into measured.

### 4.2 The benchmark harness

Depends on 4.1 for its most important metric. Three measurements:

| Metric                      | Source                                              |
| --------------------------- | --------------------------------------------------- |
| Tokens consumed             | The agent adapter reports it                        |
| Iterations to clean compile | Count `mz check` rounds until zero errors           |
| Defect rate                 | `mz contract` after a clean compile — **needs 4.1** |

Design constraints, all from RFC-0004:

- The harness is **public**, including the fixture format and a worked example task set.
- The task set is a **path argument**, never a baked-in location. This is what keeps the
  visibility decision (§5) open instead of compiled in.
- Per RFC-0002 §1, the benchmark **must include a small open-weight arm**. A frontier-only
  result does not test the thesis — the design target is the 7B local model, and a syntax
  that helps only large models has failed its own premise.
- Public CI must stay green with no secrets and no task set present.

### 4.3 After that

`mz patch` / `mz refs` / `mz diff`; IR store persistence; local state in the language;
then Phase 1, the runtime.

## 5. The held-out set — decision still open

Settled: the harness, the fixture format, the metric definitions, the scoring code and an
example task set are all public. The **entire** correctness suite is public, permanently.

Open: where the scoring answers for reported runs live. Two viable shapes —

1. A private repository in `mzizi-dev`, consuming the public harness (RFC-0004 §4 as
   written).
2. One withheld directory: encrypted in the public repo, or a private annex. Cheaper, no
   second repo, no cross-repo dependency direction to police.

Owner leaned toward public in principle, which (2) satisfies. Whichever is chosen, the
constraint is not negotiable and is _not_ about secrecy: **if the answers for the tasks you
report numbers from are public, they enter the next training corpus, the benchmark then
measures memorisation, and the charter's kill criterion can no longer fire.** A number that
can only come out favourable is not a measurement. That is the whole of §1.1's argument.

And whatever is chosen, RFC-0004 §3 holds absolutely: **private consumes public; public
never consumes private.** A fork with no secrets must get a fully green CI run, or the
project is open source in name only.

## 6. Do not rename

Everything here is installed somewhere already; renaming orphans it.

| Identifier        | Current value                                                                                  | Note                                                                                                                                                                                            |
| ----------------- | ---------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| MCP registry name | `io.github.nyuchi/mzizi-mcp`                                                                   | **Hard constraint.** `io.github.*` is namespace-verified against the `nyuchi` GitHub org. It cannot move to `mzizi-dev` by rename — it needs a new registry entry plus a deprecation on the old |
| npm               | `@nyuchi/mzizi-mcp` 0.9.1 (bin `mzizi-mcp`)                                                    | Renaming breaks existing MCP client configs                                                                                                                                                     |
| npm               | `@nyuchi/mzizi-cli` 0.4.0 (bin **`fundi`**)                                                    | The bin is `fundi`, not `mzizi`. Deliberate — do not "fix" it                                                                                                                                   |
| npm               | `@nyuchi/mzizi-skills` 0.5.1, `@nyuchi/mzizi-console-app` 0.2.0, `@nyuchi/bushtrade-mcp` 0.4.1 | Published and versioned                                                                                                                                                                         |
| binary            | `mz`                                                                                           | Keep                                                                                                                                                                                            |
| crates            | `mzizi-lang-compiler`, lib `mzizi_lang_compiler`                                               | Not yet published; free to change _before_ first publish, fixed after                                                                                                                           |
| public API        | `mzizi.dev`, `/api/v1/ui/{name}`, all 571 component names                                      | `npx shadcn add` URLs exist in the wild                                                                                                                                                         |

## 7. What stays in the Nyuchi orgs

Per the charter's ownership line: the framework is 100% Bundu Foundation IP; the Fundi
console and the React design system are Nyuchi's.

- **`nyuchi/mzizi`** — the React/TSX component registry and mzizi.dev. The charter names it
  explicitly as the "Nyuchi-owned Mzizi design system/registry", distinct from the framework.
  Stays. **It is also the benchmark corpus** — the ground-truth implementations tasks are
  scored against — so the new session needs it in scope (§8).
- **`nyuchi/mzizi-tools`** — fundi, the console app, the MCP server, the CLI, the skills
  bundle. Nyuchi console tooling. Stays. Only `mzizi-lang/` leaves.

One tension to be deliberate about rather than let drift: mzizi.dev's content is now
entirely about the Foundation framework while the site lives in a Nyuchi-owned repo. Fine
today; a question the moment the Foundation wants editorial control of its own front page.
Creating `mzizi-dev/docs` (§1) is how that gets resolved.

## 7.1 The wider org topology (owner's plan, 2026-08-23)

The move is one part of a three-way split along ownership and revenue lines:

| Org         | Repo                                    | Holds                                                               | Visibility   |
| ----------- | --------------------------------------- | ------------------------------------------------------------------- | ------------ |
| `mzizi-dev` | `framework`, `benchmark`, `docs`        | The Foundation framework, open                                      | public       |
| `mzizi-dev` | `mzizi-tools` (moved)                   | Non-revenue tooling                                                 | public       |
| `nyuchi`    | `mzizi-agents` (new)                    | Proprietary Nyuchi tools built **on** Mzizi — the revenue surface   | private      |
| `nyuchi`    | `mzizi-registry` (renamed from `mzizi`) | The component registry — `mzizi.dev`, `/api/v1/ui/{name}`           | public       |
| `nyuchi`    | `mzizi-console` (new)                   | The console app, extracted from `mzizi-tools` → `console.mzizi.dev` | owner's call |

This is a good boundary: revenue vs non-revenue is simultaneously a licence line, a
visibility line and an ownership line, which is exactly when a repo beats a directory.

**The rule that makes it hold, and it is the same rule as RFC-0004 §3:**

> `mzizi-agents` consumes `mzizi-dev/*`. `mzizi-dev/*` never consumes `mzizi-agents`.

If the open framework ever depends on the proprietary layer, the open project is hollow — a
fork gets a tree that cannot build. This is the identical failure mode RFC-0004 guards
against for tests, and it deserves the identical treatment: the framework's CI must stay
green with no access to `mzizi-agents` whatsoever.

### 7.2 Four consequences to handle before executing

**1. Moving `mzizi-mcp` out of the `nyuchi` org requires republishing under a new registry
name.** `.github/workflows/publish-mzizi-mcp.yml` authenticates with
`mcp-publisher github-oidc`, which proves the workflow is running from a repo owned by
`nyuchi`. The namespace `io.github.nyuchi/mzizi-mcp` can therefore only be published from a
`nyuchi`-owned repo.

**Owner's decision: accept the rename and notify customers** with new links and connect
instructions. That makes this a planned migration rather than a blocker. What it requires:

- Publish `io.github.mzizi-dev/mzizi-mcp`, and mark the `io.github.nyuchi/*` entry
  **deprecated rather than deleted** — a deleted entry gives a client an error, a deprecated
  one gives it a pointer.
- **The old name does not redirect.** MCP registry names are identifiers, not URLs; there is
  no 308 equivalent. Every existing client config breaks the day the old entry stops
  resolving, which is what makes the notification load-bearing rather than courteous.
- Keep the old entry resolving for an overlap window after the announcement. Customers
  configure an MCP server once and forget it; a rename with no overlap is indistinguishable
  from an outage.
- The same applies if `@nyuchi/mzizi-mcp` becomes `@bundu/*` or `@mzizi/*`. npm _does_
  support `npm deprecate` with a message — the cheapest notification channel available, and
  it should carry the new name.

**2. ~~`mzizi-console` collides.~~ Resolved by separating the two products.** The console
app moves out of `mzizi-tools` into its own repo, deployed at **`console.mzizi.dev`**. So
`mzizi-console` names the console, and `nyuchi/mzizi` — the component registry — becomes
**`mzizi-registry`**, which says what it is.

That leaves one live question. **If `mzizi.dev` becomes the framework's site, where does the
registry API live?** Today `mzizi.dev` serves both the site and `/api/v1/ui/{name}`, which
571 components and every `npx shadcn add` URL in the wild depend on. Either a
`registry.mzizi.dev` subdomain, or keep `/api/v1/*` on the apex and give the framework docs
its own host. Decide it **before** the docs move: moving the site first and the API second
breaks installs in the gap.

**3. `mzizi-tools` is currently private, and going public needs an audit first.** It holds
`fundi/` (WorkOS auth), `supabase/`, and a `docs/` directory covering service bindings and
self-healing. `gitleaks` runs in CI and is green, which covers credentials — it does not
cover internal hostnames, account IDs, org structure, or the WorkOS/Supabase project
identifiers in config. Audit those before flipping visibility, and remember git history is
public too: a secret removed in a later commit is still public once the repo is.

**4. Renames redirect, but not everything follows.** GitHub redirects git remotes and web
URLs after a repo rename, so clones and existing `git remote`s keep working. What does not
automatically follow: any `raw.githubusercontent.com/nyuchi/mzizi/...` URL baked into
`registryDependencies` or install instructions (redirects exist but are fragile as a
contract), and the Cloudflare Workers Builds / Vercel project bindings should be verified
rather than assumed — they bind by repo ID and normally survive, but "normally" is not
"verified", and `mzizi.dev` plus `/api/v1/ui/{name}` are live public surfaces.

### 7.2a Retiring the old naming

**"Nyuchi Design Portal" and `design.nyuchi.com` are retired and must not appear as live
naming anywhere.** `design.nyuchi.com` is already a 308 to `mzizi.dev`, and `nyuchi/mzizi`
already carries a guard proving the intent — `__tests__/metadata-titles.test.ts` has a
`RETIRED` list including `/nyuchi design portal/i`.

That guard only checks page metadata titles. **47 occurrences remain in `nyuchi/mzizi`**, two
of which reach users:

- `app/api/v1/stats/route.ts` returns `"Nyuchi Design Portal — Usage Statistics"` in a
  **public API response**
- `components/registry/n2-primitives/sidebar-01.tsx` **renders**
  `nyuchi design portal v4.0.1`

Plus `supabase/functions/*` headers, `app/api/openapi/route.ts`, the bug-report issue
template, and `content/doctrine/documentation/*.mdx` front matter.

Also live and wrong: the **GitHub repo description and homepage** on `nyuchi/mzizi` still
read "The Nyuchi Design Portal" and `design.nyuchi.com`. That is the most-seen surface of the
lot, and it points at a retired domain.

Two categories, which must not be treated the same:

| Keep                                                                                                                            | Remove                                                                                                                            |
| ------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Historical and doctrinal references — "replaces the legacy `design.nyuchi.com` MCP", the `RETIRED` test list, CHANGELOG entries | Live naming — API responses, rendered UI, page titles, repo metadata, doc front matter, and comments describing current behaviour |

Deleting the historical references would erase the record of _why_ the name is retired, which
is what stops it being reintroduced. Widening the existing guard past page titles is the
change that makes the purge stick rather than recur.

### 7.3 Package mapping — needs the owner's call

Revenue classification is a business fact, not something to infer from code. Proposed
starting point only:

| Package             | Today                            | Proposed home                                         | Confidence |
| ------------------- | -------------------------------- | ----------------------------------------------------- | ---------- |
| `mzizi-lang/`       | `mzizi-tools`                    | `mzizi-dev/framework`                                 | settled    |
| `mzizi-skills`      | `@nyuchi/mzizi-skills`           | non-revenue → moves                                   | high       |
| `mzizi-plugin`      | (unpublished)                    | non-revenue → moves                                   | high       |
| `mzizi-mcp`         | `@nyuchi/mzizi-mcp`              | **stays in `nyuchi`** — see 7.2 §1                    | high       |
| `mzizi-console-app` | `@nyuchi/mzizi-console-app`      | own repo → `console.mzizi.dev`                        | settled    |
| `fundi`             | `@nyuchi/fundi-tester` (private) | `mzizi-agents`                                        | owner      |
| `mzizi-cli`         | `@nyuchi/mzizi-cli`, bin `fundi` | `mzizi-agents`? bin name suggests it pairs with fundi | owner      |
| `bushtrade-mcp`     | `@nyuchi/bushtrade-mcp`          | unrelated to Mzizi — its own repo                     | owner      |

## 8. Starting the next session

Scope it to four repositories:

- `mzizi-dev/framework` — the work
- `mzizi-dev/benchmark` — the harness
- `nyuchi/mzizi` — **required**: the benchmark corpus and ground truth
- `nyuchi/mzizi-tools` — until `mzizi-lang/` is deleted from it

First task: **§4.1, contract evaluation.** Read `design/RFC-0001` §1.6 for what contracts are
meant to do and `primitives/button.mz` for the canonical case.

## 9. Decisions the owner still has to make

1. **The npm scope for anything the framework publishes.** Three candidates are live: the
   charter says `@bundu`, everything shipped today says `@nyuchi/`, and the
   registryDependencies migration task says `@mzizi/`. One-way door once published.
2. **The held-out set's home** — §5.
3. **`mzizi-dev/roadmap`** — fold into `framework`, or keep as a Projects surface.
4. **Whether `nyuchi/mzizi` is eventually renamed** to say what it is (the React design
   system), now that the Mzizi name means the framework. Not urgent; the collision is
   cosmetic until someone has both cloned.
