# React arm

The TypeScript/React side of the `ui-spec` family
([RFC-0009](../../../design/RFC-0009-comparison-benchmark.md) §1, §2). An authoring agent
is given a component's language-neutral `spec.md` and writes one `.tsx` file. `check.sh`
is its check, the counterpart of `mz check` on the Mzizi arm and `cargo check` on the Rust
arms. The guide is [`../../prompts/react-guide.md`](../../prompts/react-guide.md).

It runs `ui-spec` only. Handed `spec.tsx`, which is a registry React component, it would be
copying the answer (RFC-0009 BM-1), so `arm.toml` does not list `ui-port`, and the runner
refuses to start that episode.

## Pinned versions: from the registry lockfile

Source: `mzizi-dev/mzizi-registry` at commit `3afeb752253a86b5488867078c2238d2cf62b4f0`,
read on 2026-10-07.

- `pnpm-lock.yaml` has one importer (`.`, the registry's root `package.json`). It declares
  `typescript`, `react` and `@types/react` exactly (`6.0.3`, `19.2.8`, `19.2.18`) and
  `class-variance-authority`, `clsx` and `tailwind-merge` by caret range (`^0.7.1`,
  `^2.1.1`, `^3.6.0`).
- The lockfile resolves each of the six to one version, the ones in the table, and
  `@types/react` to `csstype 3.2.3`.
- The registry's `tsconfig.json` has `strict: true`, `jsx: react-jsx`,
  `moduleResolution: bundler`, and `@/*` paths with no `baseUrl`.

| Package                    | Pin       | Why it is here                                   |
| -------------------------- | --------- | ------------------------------------------------ |
| `typescript`               | `6.0.3`   | the checker, `tsc`                               |
| `react`                    | `19.2.8`  | the framework                                    |
| `@types/react`             | `19.2.18` | React's types, which `strict` checks JSX against |
| `class-variance-authority` | `0.7.1`   | `cva`, the registry's variant system             |
| `clsx`                     | `2.1.1`   | used by `cn()`                                   |
| `tailwind-merge`           | `3.6.0`   | used by `cn()`                                   |

`sandbox/package.json` pins all six exactly. `sandbox/package-lock.json` is `npm install`'s
resolution of them: seven packages, `csstype` the seventh. Each of the seven has the same
version and the same `integrity` hash as in the registry's `pnpm-lock.yaml`. `setup.sh`
installs it with `npm ci`, which fails rather than change the lockfile. The versions were
copied in by hand. Nothing here reads the registry, and CI never fetches them (`AGENTS.md`,
"Repo boundaries").

`typescript` 6.0 is still the JavaScript `tsc`, whose output shape RFC-0009 §3 documents
(7.0 is the native port). It deprecates `baseUrl` (TS5101), so `sandbox/tsconfig.json` has
none, as the registry's has none. `paths` resolve from the `tsconfig.json`'s directory.

These pins replaced provisional ones on 2026-10-07: `typescript` `5.9.3`, `react` and
`@types/react` `19.3.0`, `tailwind-merge` `3.7.0`, the npm registry's versions on
2026-09-30, when the registry lockfile could not be read.

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

With `typescript` `6.0.3` (2026-10-07), `../../prompts/verify-guide.sh` on the React guide
still matches both of its wrong-on-purpose blocks' output byte for byte.

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
