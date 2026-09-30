# React arm

The TypeScript/React side of the `ui-spec` family
([RFC-0009](../../../design/RFC-0009-comparison-benchmark.md) §1, §2). An authoring agent
is given a component's language-neutral `spec.md` and writes one `.tsx` file. `check.sh`
is its check, the counterpart of `mz check` on the Mzizi arm and `cargo check` on the Rust
arms. The guide is [`../../prompts/react-guide.md`](../../prompts/react-guide.md).

It runs `ui-spec` only. Handed `spec.tsx`, which is a registry React component, it would be
copying the answer (RFC-0009 BM-1), so `arm.toml` does not list `ui-port`, and the runner
refuses to start that episode.

## Pinned versions: provisional

| Package                    | Pin      | Why it is here                                   |
| -------------------------- | -------- | ------------------------------------------------ |
| `typescript`               | `5.9.3`  | the checker, `tsc`                               |
| `react`                    | `19.3.0` | the framework                                    |
| `@types/react`             | `19.3.0` | React's types, which `strict` checks JSX against |
| `class-variance-authority` | `0.7.1`  | `cva`, the registry's variant system             |
| `clsx`                     | `2.1.1`  | used by `cn()`                                   |
| `tailwind-merge`           | `3.7.0`  | used by `cn()`                                   |

**These are not yet the registry's pins.** RFC-0009 §1 says the React arm pins from
`mzizi-dev/mzizi-registry`'s lockfile at the task commit (`3afeb752`), as the Dioxus arm
does. That lockfile could not be read when this arm was built (2026-09-30). The versions
above are exact, and are what the npm registry served that day: the latest release of each
package, except `typescript`. There `7.0.2` is the latest, but it is the native port, and
RFC-0009 §3 documents the output shape of the JavaScript `tsc`, so `5.9.3` (the last 5.x
release) is pinned instead. Before a gating run these pins must be replaced with the
registry lockfile's versions, and this section rewritten as the Dioxus arm's is:
[`../../READINESS.md`](../../READINESS.md), "What remains", item 1.

`sandbox/package-lock.json` is `npm install`'s resolution of those six pins: seven
packages, each with its `integrity` hash, and `csstype` (a dependency of `@types/react`)
is the seventh. `setup.sh` installs it with `npm ci`, which fails rather than change the
lockfile.

## Support stubs: `cn()` only

A registry `.tsx` imports `cn` from `@/lib/utils`. `sandbox/src/lib/utils.ts` is the
registry's usual definition (`twMerge(clsx(inputs))`), and `@/*` maps to `sandbox/src/*`
in `sandbox/tsconfig.json`. Nothing else is provided. `radix-ui` (the registry's `Slot`
for `asChild`) is not installed, because no `spec.md` asks for `asChild`. A candidate that
imports it fails with TS2307, which the guide warns about.

## Candidate → project mapping

```text
sandbox/
  package.json       the six exact pins
  package-lock.json  npm's resolution of them, with integrity hashes
  tsconfig.json      strict: true, jsx: react-jsx, moduleResolution: bundler, @/* paths
  src/lib/utils.ts   cn()
  src/component.tsx  the candidate, written by check.sh and removed on exit (gitignored)
  node_modules/      installed once by setup.sh (gitignored)
```

## `check.sh <candidate.tsx>`

| Exit | Meaning                                                                                                         |
| ---- | --------------------------------------------------------------------------------------------------------------- |
| 0    | No type errors                                                                                                  |
| 1    | Type errors against the candidate, printed to stdout                                                            |
| 2    | Usage or setup error (bad arguments, `node_modules` not installed, or tsc failing with no candidate diagnostic) |

It runs `tsc --noEmit --pretty false -p tsconfig.json` in the sandbox, the check RFC-0009
§3 names for this arm, and prints tsc's output with `src/component.tsx` shown as the
candidate's file name. Nothing else is changed. On a candidate with two errors (run here on
2026-09-30):

```text
candidate.tsx(2,28): error TS2322: Type 'string' is not assignable to type 'number'.
candidate.tsx(2,72): error TS2304: Cannot find name 'y'.
```

tsc exits 2 on that file. `check.sh` exits 1, because the diagnostics name the candidate.

**Not a CI requirement.** Node is needed only on the machine that runs benchmark episodes.
The runner's tests use in-process fakes, and CI never runs `setup.sh` or `check.sh`
(RFC-0009 §9).

## Scoring

`mzizi-benchmark-harness score --arm react` reads the candidate's `cva(…)` calls and its
`data-slot` attributes (`benchmarks/harness/src/react.rs`). The registry's own `button.tsx`
and `badge.tsx`, scored that way against their Rust references, give 0 defects on 9 and 2
facts. The naming sentence tells the agent which call and keys to use
(`buttonVariants.size` for the reference's `ButtonSize`). A task whose enum has no classes
in the reference, like `card`'s size, is still read from a `cva` key, so the guide asks for
every variant group to be one, even one whose classes are empty.
