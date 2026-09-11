# Mzizi primitives

The foundational component set, written in Mzizi. Every file here parses clean under
`mz check`, and every assertion in its `contract` block is evaluated by `mz contract`. Both
are gated in CI, so these are verified source rather than aspirational examples.

One undocumented thing to know while reading them: each root element carries a
`portal = "https://mzizi.dev/components/<name>"` attribute that no RFC mentions. It is
tracked in [`mzizi-dev/mzizi#5`](https://github.com/mzizi-dev/mzizi/issues/5) and is either
owed an RFC-0001 §1.5 amendment or a deletion.

| Primitive     | Shape                    | Why it is here                                                                     |
| ------------- | ------------------------ | ---------------------------------------------------------------------------------- |
| `button`      | variant × size           | The most-composed primitive, and the one carrying the touch-floor contract         |
| `input`       | size                     | Form foundation; the other interactive primitive with a touch floor                |
| `badge`       | variant                  | The simplest real variant table                                                    |
| `alert`       | variant with a11y column | Shows a column that is not styling — `announce` pins the ARIA role to the severity |
| `card`        | container                | Surface + nested slots                                                             |
| `avatar`      | size, with fallback      | Two columns per variant, and `when` on an optional value                           |
| `separator`   | axis                     | Smallest useful primitive                                                          |
| `spinner`     | size, `use motion`       | The capability declaration in practice                                             |
| `confirm_bar` | composition              | Uses `button` and `alert` **with no import line** — see below                      |

## Do we use crates? Three tiers, and the answer differs per tier

This is the load-bearing packaging decision, so it is written down rather than left to
habit.

### 1. Toolchain and runtime — **yes, crates**

`mz`, the compiler, and the runtime host bindings are ordinary Rust software. Cargo is the
best package manager in existence and RFC-0002 §3 already names its UX as the bar to hit;
there is nothing to gain by inventing a second one. `cargo install mz` is how you get the
toolchain, and an app depends on the runtime crate for its target (web / native / edge).

### 2. Primitives and components — **no, not crates**

The primitives in this directory are **`.mz` source, content-addressed, copied into your
project** — the registry model, not a linked dependency. Four reasons, in order of weight:

1. **Content addressing and semver are opposed.** RFC-0002 §2.1 commits to a
   content-addressed IR, where a component _is_ its hash. A crate dependency is a name
   resolving to a mutable version range. Shipping primitives as crates would forfeit all
   four wins that decision buys — free renames, perfect caching, semantic patching, and
   incremental compiles keyed on hashes.
2. **The small-model target needs the source in context.** A vendored primitive is in the
   repository, so it is readable, editable, and patchable by whatever model is working. A
   crate dependency lives in `~/.cargo` — invisible to the agent, unpatchable without a
   fork. For the population this language is for, that difference is decisive.
3. **You own the code.** This is the shadcn insight and it is why the existing registry
   works: a primitive you can edit is a primitive you can fix, without waiting on an
   upstream release.
4. **Bandwidth is a real constraint.** One file fetched over a slow link beats resolving
   and downloading a dependency tree. This is not a hypothetical for the people this is
   built for.

### 3. Rust interop — **crates, when genuinely needed**

Candle, `workers-rs`, and anything else from the Rust ecosystem come in as normal crates,
at the runtime boundary. The escape hatch stays open; it is just not the default path.

## No import lines, and why that is possible

`confirm_bar` uses `button` and `alert` without a single import statement. That is not
sloppiness — it follows from two facts:

- **The namespace is flat and names are globally unique.** The existing corpus already
  enforces one component, one name. So a bare `button` in a view is unambiguous.
- **Resolution is by hash, recorded outside the source.** Which `button` a component got
  is pinned in the project manifest, not written into the file. The source says _what_, the
  manifest says _which_.

The result is that every import line in every component — pure ceremony carrying no
decision (RFC-0001's FM-6) — simply does not exist. `use` in Mzizi is reserved for
capabilities (`use motion`), which _is_ a decision, and one worth reading.

## What contracts buy here

`button.mz` ends with:

```mz
contract
  every button_size height at_least 48
  button_size.default height is 56
end
```

That first line is the rule the corpus's own doctrine calls non-negotiable, and which the
TypeScript violated in five separate components — because there, the heights lived only
inside Tailwind class strings where nothing could check them. Here the height is data on
the variant, so one line holds the whole table to the floor.

And since [RFC-0006](../design/RFC-0006-contracts.md) it is _run_: `mz contract
../primitives/button.mz` evaluates that line against the table and exits 1 if any variant
falls below 48. Drop `sm` to 40 and `mz check` still reports zero errors while
`mz contract` reports the defect — which is precisely the shape CHARTER.md §6 calls a Phase
0 defect.

Note also `alert`'s `announce` column. The ARIA role and the colour are one decision per
severity, so they live in the same row and cannot drift apart — the same structural fix
that killed the parallel-`Record` drift, applied to accessibility.

## Status

Every file here parses under `mz check` and every assertion in it is evaluated by
`mz contract` — 33 assertions across these nine files and the corpus example, all of them
holding, all of them gated in CI. A clause the evaluator cannot apply is an error, so a
green run means the assertions ran, not that they were counted.

What that is **not** is a measurement against the charter. `mz contract` checks whether a
component keeps its own promises; CHARTER.md §6's defect metric compares an agent-authored
component against an independent reference implementation read from disk, and that
comparison has no toolchain yet ([RFC-0006](../design/RFC-0006-contracts.md) §10.1). These
files remain verified syntax plus verified self-consistency — which is what Phase 0 needs
so far, and no more than it claims.
