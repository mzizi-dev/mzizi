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
