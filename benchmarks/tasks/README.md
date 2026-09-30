# Phase 0 pilot task set

This directory is the task set for the Phase 0 benchmark _pilot_: an LLM agent ports a
registry component into Mzizi (`.mz`) and, separately, into raw Dioxus (Rust), and each port
is measured on tokens consumed, iterations to a clean compile, and defect rate. A defect is
code that compiles cleanly but disagrees with the hand-written Rust reference
([`CHARTER.md`](../../CHARTER.md) §6, [`RFC-0006`](../../design/RFC-0006-contracts.md) §10.1).

It is a pilot, and it is small: four tasks (`card` was added on 2026-09-29, with the `slot_set`
fact). Results on it are reported whichever way they come out.

## Fixture format

One directory per task, named after the component, holding three copied files and one
written here (`spec.md`, below):

```text
benchmarks/tasks/<name>/
  task.toml      metadata and provenance
  spec.tsx       byte-identical copy of the registry .tsx
  spec.md        the same component, restated language-neutrally (written here)
  reference.rs   byte-identical copy of the registry .rs
```

`task.toml`:

```toml
name = "button"
spec = "spec.tsx"
reference = "reference.rs"
[source]
repo = "mzizi-dev/mzizi-registry"
commit = "3afeb752253a86b5488867078c2238d2cf62b4f0"
spec_path = "components/registry/n2-primitives/button.tsx"
reference_path = "components/registry/n2-primitives/button.rs"
```

- `spec` is the _only_ component source a `ui-port` agent is shown, and `spec.md` (or
  `spec_md = "…"`) the only one a `ui-spec` agent is shown.
- `reference` is never shown to the authoring agent. It is read only by the scorer.
- `spec` and `reference` are paths relative to the task directory. `[source]` records where
  both files came from, so anyone can re-derive them with `git show <commit>:<path>`.
- Optional keys: `enums = [...]` (the enum-naming sentence), `allow_variant_renames` with its
  `rename_reason` (below), and `score_slots = true`, which adds the `slot_set` fact: the set
  of `data-slot` values the port renders against the reference's
  (`mzizi-benchmark-harness score --slots`). Each key is off unless a task sets it, so a
  task keeps the facts it was scored on.

Neither file is edited, reformatted or annotated. That is why neither carries a provenance
header: `task.toml` carries it instead, and a copy that is not byte-identical is not a
fixture. This is also why `spec.tsx` is not formatted to this repository's Prettier config —
it is formatted the way the registry formatted it.

### `spec.md`: the language-neutral input (RFC-0009 §2.1)

_Added 2026-09-29._ Each task also has a `spec.md`, the input for the `ui-spec` family
(`mzbench … --family ui-spec`). `spec.tsx` remains the `ui-port` input, the default, and
is what the pilots ran. `spec.md` is **not** a registry file. It is written for this
repository, by hand, from `spec.tsx` alone (never from `reference.rs`), and it restates
the component without React idiom: prose, one table per variant group with its variants,
classes and default, the base classes, and each rendered element with its `data-slot`,
attributes and classes. A React agent given `spec.tsx` would be copying (RFC-0009 BM-1).
Given `spec.md`, every arm starts from the same statement.

What each `spec.md` leaves out of its `spec.tsx`, on purpose: the `import` lines, `cn()`,
`asChild` and `Slot`, prop spreading (stated instead as "any other attribute is passed
through"), and the TypeScript types. The pilot tasks keep their `enums` and flags, so a
`ui-spec` episode is scored on the same facts as a `ui-port` one. The spec states the class
strings, defaults and slots the facts are about, just as `spec.tsx` did. `spec.md` is
formatted with this repository's Prettier config, because it is this repository's file.

A held-out `ui-spec` task needs only `spec.md`. The loader accepts a task with either
input, and `kill-criterion/check-task.sh` checks whichever the task has.

### The task set is a path argument

The harness takes the task set as a path argument and never bakes a location in
([`MIGRATION.md`](../../MIGRATION.md) §4.2). This directory is one task set that happens to
live in the public repository. A held-out set elsewhere, on disk or in a private repository,
uses the same format and is passed the same way. Public CI must not depend on any task set
being present.

## Provenance and licence

All eight copied files come from
[`mzizi-dev/mzizi-registry`](https://github.com/mzizi-dev/mzizi-registry) at commit
`3afeb752253a86b5488867078c2238d2cf62b4f0`, copied unmodified. The registry is licensed under
the Apache License, Version 2.0. Its `LICENSE` is byte-identical to this repository's
[`LICENSE`](../../LICENSE) (same SHA-256), so that file is the licence copy §4(a) requires.

The registry ships a `NOTICE` file. Apache-2.0 §4(d) requires its text to travel with
redistributed copies, so it is reproduced unmodified in [`NOTICE`](./NOTICE) in this
directory:

```text
Mzizi
Copyright 2026 The Bundu Foundation

Developed and operated by Nyuchi Africa (Pvt) Ltd under the
governance of The Bundu Foundation.
```

## Selection criterion

A component is in the _scored_ set if and only if its `.rs` has at least one fact the
reference extractor scores. The extractor reads, from every Rust `enum`:

- the variant set;
- the `#[default]` variant, when there is one;
- and, for an enum with an `impl` block containing a `fn classes(` method whose match arms
  return string literals, the touch height of each variant, derived from an `h-N` or
  `size-N` class token in that variant's arm as N×4 px.

With `score_slots = true` it also reads the set of literal `"data-slot": "…"` values the
component renders.

_Corrected 2026-09-29._ This section used to say that the extractor looks only at enums with
a `classes()` method. The scorer never worked that way. It reads every enum's variant set and
`#[default]`, and the survey below was written from the wrong rule. Scoring each reference
against itself at the pinned commit (`score --arm dioxus --slots`) gives these facts for the
components the survey marked "out: 0 facts":

| Component                   | Enum facts | `data-slot` set                                                                      |
| --------------------------- | ---------: | ------------------------------------------------------------------------------------ |
| `card`                      |          2 | 7: `card`, `card-header`, `-title`, `-description`, `-action`, `-content`, `-footer` |
| `mzizi-connectivity-bar`    |          2 | 1                                                                                    |
| `mzizi-deep-link-handler`   |          2 | 1                                                                                    |
| `mzizi-mini-app-runtime`    |          2 | 1                                                                                    |
| `mzizi-toast-provider`      |          2 | 1                                                                                    |
| `mzizi-notification-center` |          1 | 1                                                                                    |
| `mzizi-persistent-player`   |          1 | 1                                                                                    |
| `mzizi-docs-engine`         |          0 | 1                                                                                    |
| `mzizi-bottom-nav`          |          0 | 1                                                                                    |
| `mzizi-command-palette`     |          0 | 1                                                                                    |
| `mzizi-footer`              |          0 | 1                                                                                    |
| `mzizi-update-prompt`       |          0 | 1                                                                                    |
| `mzizi-route-guard`         |          1 | none (no `rsx!`)                                                                     |
| `mzizi-theme-provider`      |          1 | none                                                                                 |
| `mzizi-chaos`               |          2 | none (no `rsx!`)                                                                     |
| `mzizi-platform-health`     |          2 | none (no `rsx!`)                                                                     |
| `rtl-conformity-check`      |          4 | none (no `rsx!`)                                                                     |
| `mzizi-fundi`               |          8 | none (no `rsx!`)                                                                     |

Only `card` was added to this set, because it is the one with more than a single slot. The
others would add one or two facts each, and the pilot tasks are public, so they could not
serve as the kill-criterion set anyway (see "Contamination"). They are listed here as the
public pool to draw on for development runs.

Class-token Jaccard similarity is reported too, but it is a secondary metric and never
counts as a defect.

A component with _zero_ scoreable facts is left out. Such a task cannot produce a defect
however wrong the port is, so every port of it would score zero defects. Including it would
lower the defect rate, and the only thing lowering it would be the choice of tasks. Those
components can still be scored on tokens and iterations. They are not in this set, because
mixing the two kinds would make "defects per task" mean different things for different
tasks.

The candidate pool is every `.rs` under `components/registry/` that has a `.tsx` sibling:
21 of the 37 `.rs` files at the pinned commit.

Components are named here by their current registry names. mzizi-registry #355 later renamed
every `nyuchi-*` component to `mzizi-*`. At the pinned commit, and in the copied files and the
`[source]` paths in `task.toml`, they still carry the `nyuchi-` prefix.

## Chosen tasks

| Task                       | Enum (variants, `#[default]`)  | Heights scored                                         | Scoreable facts |
| -------------------------- | ------------------------------ | ------------------------------------------------------ | --------------- |
| `button`                   | `ButtonVariant` (6, `Default`) | none: no `h-N`/`size-N` in any arm                     | 2               |
|                            | `ButtonSize` (5, `Default`)    | `Default` 56, `Sm` 48, `Lg` 56, `Icon` 56, `IconSm` 48 | 7               |
| `badge`                    | `BadgeVariant` (6, `Default`)  | none: the badge's `h-5` is in `BASE`, not in an arm    | 2               |
| `mzizi-changelog-renderer` | `NodeAccent` (4, `Cobalt`)     | none                                                   | 2               |
| `card`                     | `CardSize` (2, `Default`)      | none: no `classes()`                                   | 2 + 1 slot set  |

Facts are counted as one for each enum's variant set, one for each `#[default]`, one for each
derived height, and one `slot_set` for a task that sets `score_slots`, for a total of 16. Only
`ButtonSize` exercises the height check, and only `card` the slot set; the three pilot tasks
do not set `score_slots`, so they score the 13 facts they were scored on in both pilots.

### Caveat: `card` is seven components in one `.tsx`

`card.tsx` exports `Card`, `CardHeader`, `CardTitle`, `CardDescription`, `CardAction`,
`CardContent` and `CardFooter`. A Dioxus port writes seven components. A Mzizi file holds one
component (RFC-0001 §7.4), so a Mzizi port has to render the seven slots as regions of one
view. That is a real property of the language, not an artefact of the task, and the slot set
is the fact that measures whether the port kept them. This repo's own `primitives/card.mz` is
a three-slot simplification (`card`, `card-header`, `card-body`) and scores 3 defects against
this reference, which is expected: it was never a port of `card.tsx`.

### Caveat: `mzizi-changelog-renderer` is scoreable, but its variant names diverge on purpose

It passes the selection rule, so it is in. One known issue could turn it into a source of
spurious defects, and it affects both arms equally:

- The `.tsx` keys its colour table by the retired axis names `horizontal`, `vertical`,
  `depth` and `outlier` (`AXIS_COLOURS`), and falls back with `?? "horizontal"`.
- The `.rs` renames the variants for the minerals they paint: `Cobalt`, `Tanzanite`,
  `Malachite` and `Gold`, with `#[default] Cobalt`. Its module docs record this as a
  deliberate divergence. The class strings are byte-identical to the `.tsx`.
- An agent that ports the spec faithfully will probably name the variants after the axes.
  A diff that compares variant _names_ would then report a wrong variant set and a wrong
  default, even though the variant count, the classes and the fallback colour all match.

CHARTER §6 treats disagreement with the reference as the new code's fault "unless it's a
documented, deliberate divergence", and this one is documented. Two ways to handle it:
match variants by their class string rather than by name for this task, or report its
name-level result separately from the other two. Either way, do not quietly count it as a
defect.

**What happened (2026-09-27).** The first scored pilot
([`../results/2026-09-27-pilot/RUN.md`](../results/2026-09-27-pilot/RUN.md)) hit exactly
this: both arms named the variants after the axes, as the spec does, and both scored the
same 2 defects (`variant_set`, `default`) and a null jaccard. The harness now takes the first
option, **for this task only**: its `task.toml` sets `allow_variant_renames = true` with a
`rename_reason`, so `mzbench` scores it with `--allow-variant-renames`, and when the name sets
differ the scorer pairs variants by class-token similarity (mutual best, no ties, Jaccard at
least 0.5, a complete bijection), lists each renamed pair in a `variant_names` detail, and
scores the rest through the pairing (`../README.md`, and RFC-0006 §10.1). `variant_names` is
never a defect and is not counted in `facts_checked`, so this task scores the same 2 facts
(`variant_set`, `default`) whether or not the candidate renamed.

**The opt-in, for any task.** `allow_variant_renames` is off unless a task sets it, and a task
that sets it must give a non-empty `rename_reason` (the runner refuses to load it otherwise).
Set it only where the spec and the reference disagree on variant names, so that a candidate
following the spec would otherwise be penalised; never to forgive a candidate that renamed
against its spec. Without it, `button`'s `destructive` renamed to `danger` is a defect.

**Upstream.** The divergence itself — the Rust reference renaming its own spec's
`AXIS_COLOURS` keys to mineral names — lives in `mzizi-dev/mzizi-registry`, and is worth an
issue there: either the `.tsx` should adopt the mineral names too, or the `.rs` should keep
the axis keys. It is noted here, not filed.

## Backend tasks (RFC-0009 §2.3–§2.4)

A backend task is `task.toml`, `spec.md` (the only text an agent sees), `probes.toml` (the
scoreable facts, each one HTTP request) and two references in two languages, each with a
`start.sh` that serves it on `$PORT`. `mzprobe verify <task>` (`benchmarks/probe`) starts each
reference and runs every probe; CI runs it for every backend task.

| Task         | Exercises                                                                                     | Probes | Facts | References                                    |
| ------------ | --------------------------------------------------------------------------------------------- | ------ | ----- | --------------------------------------------- |
| `b1-routing` | three routes, `HEAD`, `OPTIONS` `204`, `405`, the trailing-slash `308`, a JSON `404`, headers | 20     | 59    | Mzizi (lowered by `mz build`), and plain axum |

B1 restates the API gateway's routing facts as RFC-0009 §2.4 and RFC-0010 §3.2 cite them, at
`2468b2a`. It is derived from a public repository, so it is a development task, contaminated
by construction and never gating. Both references passed all 59 facts on 2026-09-30.

## Survey: all 21 `.rs` files with a `.tsx` sibling

_Scoreable_ means scoreable under the extractor rules above. _Unscored but checkable_ lists
behaviour that a test could check but the extractor does not read: ARIA roles, `data-*`
attributes, prop defaults and rendered text. "No `rsx!`" means the `.rs` is a port of the
component's logic only and renders no markup.

| Component (node)                 | Scoreable                                                        | Unscored but checkable                                                                                                    | `.tsx` imports                                   | Pilot           |
| -------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ | --------------- |
| `button` (N2)                    | `ButtonVariant` 6/`Default`; `ButtonSize` 5/`Default`, 5 heights | `data-slot="button"`, `data-variant`/`data-size` slugs (`icon-sm`), `data-portal`; `variant`/`size`/`class` default       | react, cva, radix-ui `Slot`, `cn`                | in              |
| `badge` (N2)                     | `BadgeVariant` 6/`Default`                                       | `data-slot="badge"`, `data-variant`, `data-portal`; `h-5` base height                                                     | react, cva, radix-ui `Slot`, `cn`                | in              |
| `mzizi-changelog-renderer` (N10) | `NodeAccent` 4/`Cobalt`                                          | `role="feed"`, `aria-label`s, `aria-posinset`/`setsize`/`labelledby` (Rust additions), `data-slot`; `+c`/`~c`/`N{n}` text | react, `cn`                                      | in (see caveat) |
| `card` (N2)                      | `CardSize` 2/`Default` (variant set, default); 7 `data-slot`s    | 7 `data-slot`s, `data-size`, `data-loading`, `loading = false`, 3-bar skeleton                                            | react, `cn`                                      | in (2026-09-29) |
| `mzizi-docs-engine` (N10)        | none: no enum                                                    | `data-slot`, `aria-current`, 3 `aria-label`s, 7 prop defaults, search and filter functions                                | react, `cn`                                      | out: 0 facts    |
| `mzizi-bottom-nav` (N7)          | none: no enum                                                    | `aria-label`, `data-slot`, `is_active` path matching                                                                      | react, `next/navigation`, `cn`, `@/lib/icons`    | out: 0 facts    |
| `mzizi-command-palette` (N7)     | none: no enum                                                    | `role` dialog/listbox/option, `aria-modal`, `aria-hidden`, `node_mineral_class`, Ctrl/Meta+K                              | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-connectivity-bar` (N7)    | none: `ConnectionState` 4/`Online` has no `classes()`            | `role="status"`, `aria-live`, `data-state`, labels, `colour()`, `auto_hide_delay_ms = 2000`                               | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-deep-link-handler` (N7)   | none: 2 enums, no `#[default]`, no `classes()`                   | `data-slot`, route template/regex resolution                                                                              | react, `@/lib/harness`                           | out: 0 facts    |
| `mzizi-footer` (N7)              | none: no enum                                                    | `role="contentinfo"`, `aria-label`, default link sections                                                                 | react, `cn`, `@/lib/harness`, 2 brand components | out: 0 facts    |
| `mzizi-mini-app-runtime` (N7)    | none: 2 enums, no `#[default]`, no `classes()`                   | `role` application/status/alert, `data-app`/`data-state`/`data-tier`, state default `Loading`                             | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-notification-center` (N7) | none: `NotificationKind` 4, no `impl`                            | `role="dialog"`, `aria-modal`, `aria-label`s, unread count                                                                | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-persistent-player` (N7)   | none: `MediaKind` 3, no `impl`                                   | `role="region"`, `aria-label`s, progress clamping, `0.0`/`false` defaults                                                 | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-route-guard` (N7)         | none: `AuthRequirement` 4, no `impl`                             | allow/deny logic; no `rsx!`                                                                                               | react, `@/lib/harness`                           | out: 0 facts    |
| `mzizi-theme-provider` (N7)      | none: `ThemeMode` 3, no `impl`                                   | initial-theme resolution order                                                                                            | react, `@/lib/tokens`                            | out: 0 facts    |
| `mzizi-toast-provider` (N7)      | none: class tables are free functions, not `impl … classes()`    | `position_class`/`type_style_class` tables, `role="alert"`, `aria-live`, `aria-atomic`, position default `BottomRight`    | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-update-prompt` (N7)       | none: no enum                                                    | `role="alertdialog"`, `aria-label`, body text, reduced-motion style, defaults `220`/`false`                               | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `mzizi-chaos` (N8)               | none: 2 enums, no `#[default]`, no `classes()`                   | injection and diagnosis logic; no `rsx!`                                                                                  | react                                            | out: 0 facts    |
| `mzizi-platform-health` (N8)     | none: 2 enums, no `#[default]`, no `classes()`                   | status labels, `color_var`, overall roll-up; no `rsx!`                                                                    | react, `cn`, `@/lib/harness`                     | out: 0 facts    |
| `rtl-conformity-check` (N8)      | none: `Direction` 3/`Ltr` and 2 more, no `classes()`             | RTL audit rules and levels; no `rsx!`                                                                                     | react                                            | out: 0 facts    |
| `mzizi-fundi` (N9)               | none: 8 enums, no `#[default]`, no `classes()`                   | healing-plan and approval logic; no `rsx!`                                                                                | react                                            | out: 0 facts    |

`cn` is `@/lib/utils`. "cva" is `class-variance-authority`.

## Contamination

The owner decided that this task set and its results are public, which settles
[`MIGRATION.md`](../../MIGRATION.md) §5 _for this pilot_. The upside is that anyone can
reproduce a result from this directory and the pinned commit. The cost is the one that
[RFC-0004](../../design/RFC-0004-test-topology.md) §1.1 names: these tasks, and the reference
answers in `reference.rs`, can end up in future training data, and once published they can
never again serve as held-out tasks.

So a pilot result from this set is evidence, not a verdict. A later run meant to test the
charter's kill criterion needs _fresh, unpublished_ tasks in this same format, passed to the
harness by path. A kill criterion scored on a set that may have been memorised could never
fire.
