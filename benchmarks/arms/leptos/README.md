# Leptos arm

The second raw-Rust comparison side of the Phase 0 benchmark ([`CHARTER.md`](../../../CHARTER.md)
§4, §6; [`MIGRATION.md`](../../../MIGRATION.md) §4.2), alongside the [Dioxus
arm](../dioxus/README.md). `CHARTER.md`'s own Phase 0 success criterion names the
comparison as **"vs. raw Dioxus/Leptos"**; Dioxus was built first, and this arm fills in
Leptos so the "best of Rust" side of the comparison is not just one framework. An
authoring agent ports a registry `.tsx` component to Leptos Rust; `check.sh` is its
compile check, the counterpart of `mz check` on the Mzizi arm and `check.sh` on the
Dioxus arm. The authoring agent's guide is
[`../../prompts/leptos-guide.md`](../../prompts/leptos-guide.md).

Dioxus is a primitive rendering layer, not a full framework — the project owner's own
words for why this arm exists. Leptos is a fuller, more widely-regarded Rust web
framework (fine-grained reactivity, SSR, full-stack routing), so it is the fairer "best
of Rust" comparison point. This arm only exercises the client-side-component slice of
Leptos that is comparable to what the Dioxus arm and the Mzizi arm both do: one
component, one file, checked for a clean compile. It says nothing about Leptos's SSR or
routing story, which Phase 0 does not measure on any arm.

## Pinned version: Leptos `=0.8.21`

Source: crates.io, checked 2026-09-27. `0.8.21` is the latest **stable** release; `0.9`
exists only as `0.9.0-beta2` at time of writing, and a benchmark arm should not pin a
pre-release. Unlike the Dioxus arm, there is no hand-written Leptos reference anywhere in
`mzizi-dev/mzizi-registry` to match a version against — Leptos was never ported there —
so this pin tracks upstream stable directly instead of a registry lockfile.

`sandbox/Cargo.toml` pins `leptos = "=0.8.21"` with `default-features = false` and the
`csr` (client-side rendering) feature — the minimal feature set that lets the
`#[component]` macro and `view!` macro expand into something a plain `cargo check`
(host target, no `wasm32` toolchain installed) can type-check. `sandbox/Cargo.lock` was
generated fresh with `cargo generate-lockfile` against this one dependency and is
committed, the same as the Dioxus arm's.

**Verified:** `cargo check` succeeds with `default-features = false` and **no** features
at all — `leptos`'s own dependencies on `wasm-bindgen` and `web-sys` are unconditional,
and type-checking (not linking or running) never needs a `wasm32` target. `csr` was kept
anyway because it is what a real client-rendered Leptos app enables, and the guide's
examples are written and checked against exactly this feature set.

## Support stubs: none

Candidates import only `leptos::prelude::*` (and `leptos::ev::*` for event types, e.g.
`MouseEvent`). No shared `cn()`, no internal prelude, nothing vendored or stubbed —
matching the Dioxus arm's "one self-contained file" rule.

## A known asymmetry with the Dioxus arm, disclosed rather than hidden

Dioxus's guide has candidates accept a bag of extra HTML attributes via
`#[props(extends = GlobalAttributes)]` on a `Vec<Attribute>` field, spread onto the root
element with `..props.attributes`. Leptos 0.8.18's macro crate has the equivalent
mechanism — a `#[prop(attrs)]` field marker and a `dyn_attrs` codegen path — **commented
out in the shipped source** (`leptos_macro-0.8.18/src/component.rs`: `// TODO restore
dyn attrs`, the whole impl block commented out). Verified by downloading and reading
that exact crate version's source, not assumed. So this arm's guide does not teach an
attributes-passthrough prop: candidates take explicit props only (`class`, named event
callbacks, `children`). This is a real, current gap in the pinned Leptos version, not an
oversight in this arm — recorded here instead of silently working around it.

## Candidate → crate mapping

```
sandbox/
  Cargo.toml       leptos =0.8.21 (csr), own [workspace]
  Cargo.lock       generated fresh for this one dependency
  src/lib.rs       `pub mod component;` and nothing else
  src/component.rs the candidate, written by check.sh, removed on exit (gitignored)
```

The candidate becomes a module, the same way the Dioxus arm's `lib.rs` mounts its
candidate. A reference file with its `//!` header compiles here byte for byte.

## `check.sh <candidate.rs>`

Byte-for-byte the same script as the Dioxus arm's, with the crate name and paths
changed. See [`../dioxus/README.md`](../dioxus/README.md#checksh-candidaters) for the
full contract; the summary:

| Exit | Meaning                                                                                                                          |
| ---- | -------------------------------------------------------------------------------------------------------------------------------- |
| 0    | Compiles with no errors. Warnings are printed and allowed                                                                        |
| 1    | Compile errors, printed to stdout                                                                                                |
| 2    | Usage or setup error (bad arguments, missing tool, or Cargo failing with no diagnostic against the candidate); message on stderr |

Same diagnostic format (`cargo check --message-format=json`, each diagnostic's
`rendered` field, filtered to the sandbox crate, paths rewritten to the candidate's own
name and `<cargo-registry>/`), same `flock`-based isolation, same `--locked` and cleared
`RUSTFLAGS`/`CARGO_TARGET_DIR` handling, same empty `[workspace]` / root-`exclude`
arrangement so CI's `cargo … --workspace` never builds Leptos.

## Measured

Measured on the same container as the Dioxus arm's numbers, rustc 1.94.1. "Cold" means
an empty `target/`; crate sources were already in `~/.cargo/registry` (fetched by
`cargo generate-lockfile`), so fetch time is not included.

| Run                                        | Result | Time    |
| ------------------------------------------- | ------ | ------- |
| `dot.rs` (a real minimal component), cold   | exit 0 | 34.8 s  |
| `dot.rs`, warm                              | exit 0 | 0.26 s  |
| `tag.rs` (props, an enum-free event handler, `children`), warm | exit 0 | 0.32 s  |
| `dot.rs` with one injected error (`IntoVew` for `IntoView`) | exit 1 | —       |
| No arguments; two arguments; bad path       | exit 2 | —       |

The injected-error run printed rustc's real `error[E0405]` with its `help:` suggestion
(`a trait with a similar name exists`) — the same diagnostic quality the Dioxus arm
relies on, confirmed for this arm too rather than assumed. Both clean runs printed no
diagnostics.

`sandbox/target/` is ~520 MB after a check and is gitignored.

## Wiring into `mzbench`

`--arm leptos` is a fully supported third value alongside `mzizi` and `dioxus`
throughout `benchmarks/runner/src/`: prompt building (`prompt::Arm::Leptos`), the
compile-check dispatch (`exec::default_check`), episode directory naming, and
`meta.json`/final-line reporting. The one place it is deliberately **not** a third
value is the scorer invocation: `mzizi-benchmark-harness score --arm <mzizi|dioxus>`
only knows those two names, and this arm does not touch the harness (out of scope for
this change). The harness's `dioxus` extractor is already generic Rust — it reads plain
`enum … { … }` and `impl … { fn classes() { match … } }` shapes, nothing
Dioxus-macro-specific (see that crate's own doc comment: "used on the hand-written
reference, and on a Dioxus candidate... the same code both sides"). Leptos candidates
follow the identical enum/`classes()`/`slug()` convention (see the guide), so the runner
passes `--arm dioxus` to the harness for Leptos episodes too — reusing an already-generic
extractor for a second raw-Rust arm, not stretching a Dioxus-specific one.
