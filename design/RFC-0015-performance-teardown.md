# RFC-0015 — Performance teardown: what makes each stack slow, and what Mzizi takes from it

**Status:** draft for review. **Nothing in this RFC is implemented.** It proposes no change to any
language construct and no change to what `mz` lowers today. The only numbers in it are measured
in §11, on one machine, on one commit, each with its command. Nothing here says that Mzizi is
faster or slower than any stack it names, and no pull request may say so from this document
([AGENTS.md](../AGENTS.md), "The one rule that overrides the others").
**Author:** the machine author (Claude), for the owner, on issue
[#117](https://github.com/mzizi-dev/mzizi/issues/117) (Refs #117).
**Scope:** five subjects, one teardown each: rustc and LLVM (§3), tokio (§4), mio (§5), the
WebAssembly output (§6) and Dioxus (§7). Then the harness and contracts as inputs to optimisation
(§8), the fast development loop (§9), a measurement plan (§10) and the measurements taken for
this RFC (§11). Non-goals are in §12, and what this RFC does not claim is in §13.

> **Proposes to amend**, in full: none. §4, §5, §6 and §8 name places where RFC-0011 §8 (service
> lowering), RFC-0013 §13.1 and §14 (`mz run` and the lowering of a program) would change if the
> owner accepts the changes this RFC proposes. Each change is an amendment to be written when it
> is proposed, not here. Nothing takes effect until then.

---

## 0. Motivation

The owner asked, on 2026-10-20 (verbatim, also on #117):

> Why are we not reverse engineering plain rust, tokio, wasm, mio, dioxus to be the best optimal language and component architecture, and toolchain, compiler. Pull out all the things that make each slow and be build for speed, data structure, code optimization, code knowledge through our harness and contracts etc.

The request has a goal and a method. The goal is that Mzizi's compiler, runtime, components and
toolchain are designed for speed, and that each design choice names the cost it removes. The method is to take each of the five stacks apart and find where its cost is. The
charter's goal is the same in its own words: Mzizi is to stand against the best existing language
for each kind of task, and that is measured against the kill criterion, not assumed
([CHARTER.md](../CHARTER.md) §1, §4). This RFC does not replace the kill criterion.

"Best optimal" is the owner's goal. No result in this repository shows Mzizi is the best, and this
RFC does not write one.

The request points at four places where the leverage is (this list is the RFC's reading of it, not
the owner's words). Each already has a home in this repository:

1. **What `mz` lowers to**: `compiler/src/lower.rs` (a service, to an axum package) and
   `compiler/src/run.rs` and `compiler/src/run/` (a program, to a dependency-free Rust package).
2. **The runtime Mzizi ships**: the tokio server that `lower.rs` emits, and the per-program
   runtime that `run.rs` emits (the trap helpers, the 64 MiB worker thread).
3. **The agent's edit-check loop**: `mz check --agent`, `mz fix`, and the package cache that
   `mz run` keeps (`run.rs`).
4. **What contracts and the language harness let the compiler know**: `contract.rs`,
   `harness.rs`, `ir.rs` and `hash.rs`. These are the source of checks that a proof can remove,
   of layouts that known sizes can fix, and of validation that a contract already guarantees.

### 0.1 Failure modes this RFC addresses

This RFC uses a `PT-` prefix (performance teardown), because `FM-`, `CL-`, `T2-` and the other
prefixes belong to other RFCs. The evidence column says what was measured, and where.

| ID   | Failure mode                                                                                                                                                                                                                | Evidence                                                                                                                                                                                                                   |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| PT-1 | **The unmeasured speed claim.** A design decision justified by a cost nobody measured, or a result written as a claim.                                                                                                      | [AGENTS.md](../AGENTS.md) "The one rule"; the two pilots in [`benchmarks/results/`](../benchmarks/results/), neither of which showed an advantage.                                                                         |
| PT-2 | **A cost every binary pays for code it never runs.** The allocator and the formatting path are in both WebAssembly program builds measured (§11.3).                                                                         | Measured (§11.3). The allocator's `malloc` is the largest single function in the lowered `fib` build, at 5,099 bytes, 10.6% of the file.                                                                                   |
| PT-3 | **A runtime for a host the program is not on.** A multi-thread server runtime where the handler never waits, and which cannot build for WebAssembly at all.                                                                 | Read (§4.1): `dispatch` in `lower.rs` is synchronous; `#[tokio::main]` is the multi-thread runtime. Measured (§11.4): the generated service fails to build for `wasm32-unknown-unknown`, because mio has no backend there. |
| PT-4 | **A check the compiler already knows cannot fire, that runs at every use.** Every `int` operator calls a checked helper, and every construction of a record that has an `always` clause runs `mz_check`, even on constants. | Read (§8): `run.rs` lines 84–158 and `run/records.rs` lines 193–249 (the check is emitted only when the record `has_always`, line 249). Measured that the emitted check is present (§8.1). Not measured for speed.         |
| PT-5 | **An edit loop that rebuilds more than it changed.** A cold build of a lowered service took tens of seconds here, and the package cache keys on the source path, not on what the program means.                             | Measured (§11.2): 34.3 s cold for the registry service. Read: `run.rs` keys the package by a hash of the absolute source path (line 1015). `mz hash` does not cover programs (measured, §9.2).                             |

Each design choice below names the row it answers. A choice that answers none of them is not in
this RFC.

---

## 1. How to read this RFC

**What was studied, and how.** Two things, and the difference matters. Each subject's section says
what was **read in Mzizi's own code** (cited by file and line, and checked against the output of
`mz build`), what was **measured** (§11, with the command), and what is **the project's public
design** that the teardown still has to cite to a pinned source revision. The last kind is marked
as such. This RFC does not yet cite upstream source lines for it, and no change may depend on an
upstream claim until its teardown document does.

**Licences.** The owner's framing, agreed on 2026-10-20, is that all five subjects are open
source under MIT or Apache-2.0, so the teardown reads and profiles their source and design
documents. The licences as understood, to be confirmed from each pinned tree before any copy:
rustc is MIT or Apache-2.0; LLVM is Apache-2.0 with the LLVM exception; tokio and mio are MIT;
Dioxus is MIT or Apache-2.0. WebAssembly is a W3C specification, not code, and the tools that
shape its output (LLVM's WebAssembly backend, Binaryen's `wasm-opt`, `twiggy`) are open source.
This RFC copies no code. §14 Q3 says what a future copy must keep.

**Conventions.** A cost centre is a place where time, memory or bytes go. "Measured" means a
number came from a command on this machine, and §11 gives both. Sizes are bytes of the file as
built, unless a row says otherwise.

---

## 2. Method

For each subject, one teardown, in the same order:

1. **Cost centres.** Name where the time, the memory or the bytes go, from the project's own
   documentation and, where it can be done, from a profile of Mzizi's output.
2. **Measure on Mzizi's own code.** The lowered Rust that `mz build` writes, the programs in
   [`benchmarks/perf/programs/`](../benchmarks/perf/programs/), the B1 reference in
   [`benchmarks/tasks/b1-routing/reference-rust/`](../benchmarks/tasks/b1-routing/reference-rust/).
   Not the upstream project's own benchmarks, which measure a different program.
3. **Adopt or avoid.** One decision per cost centre, traced to a `PT-` row.
4. **The change.** A lowering or runtime change, named and numbered (`PC-1` and so on), not written.
   The `PC-` prefix is used because `C-` names RFC-0010's contract failure modes and `C1`–`C10`
   name the tracker's Tier 1 rows.
5. **The gate.** The benchmark that must show the change before it lands. Correctness is gated in
   CI, through `benchmarks/perf/run.sh --check-only`, which CI runs and which times nothing. Size
   is deterministic for one toolchain, so a size budget may be gated (§10). Timing is measured on
   the timing machine described in `benchmarks/perf/README.md`, never in CI.

Three rules apply to every change:

- **A removed check needs a proof.** A check comes out only when a type, a range or a contract
  clause proves it cannot fire, or when a test proves the removed path is unreachable. A passing
  test on its own is not a proof (§8.6).
- **A change to what every output contains is measured in bytes as well as time.** A panic
  handler, an allocator or a formatting path is paid for by every binary.
- **Design is adopted, code is not copied,** unless §14 Q3's policy is in force.

---

## 3. rustc and LLVM

### 3.1 What it is here

`mz build` writes Rust, and `rustc` compiles it with LLVM as the back end. The lowered program
depends on nothing outside `std` (`run.rs`, the package it writes). The lowered service depends on
axum and tokio, pinned exactly (`lower.rs` `PINS`, lines 39–41). Mzizi does not change rustc or
LLVM (§12).

### 3.2 Cost centres

| Cost centre         | What it costs                                                                                                                                                                   | Measured on Mzizi's code                                                                                                                                                                                                                                                                                             |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Monomorphisation    | One copy of machine code per generic instance. Mzizi's source has no generics yet (RFC-0013 §11.3 defers them), so the generic code is what `std` and the collections bring in. | Not separated. The `Vec` and `BTreeMap` code shows as ordinary functions in the §11.3 breakdown. Attributing it needs a profile this RFC does not run.                                                                                                                                                               |
| LLVM optimisation   | Time in `cargo build --release`; code size.                                                                                                                                     | A cold release build of the lowered registry service: 34.3 s wall. The B1 reference: 24.6 s wall (§11.2). One run each, one machine.                                                                                                                                                                                 |
| Linking             | The final link, and the binary's size.                                                                                                                                          | Not split out. The perf suite records `binary_bytes` and `build_ms` per variant; it was not run for this RFC (§11.6).                                                                                                                                                                                                |
| Incremental rebuild | The time after a one-line change to a `.mz` file.                                                                                                                               | Not measured. `mz run` keeps one package per source path and rewrites only the files whose text changed (`run.rs` `write`), so an unchanged program costs nothing to rebuild, but a changed one rebuilds the package.                                                                                                |
| Overflow checks     | Every `int` operator is a call to a checked helper (`run.rs` lines 84–158). The package also sets `overflow-checks = true` as a second net (`run.rs` line 317).                 | Measured: the profile setting does not change the size of the lowered `fib` WebAssembly (48,263 bytes with it and without it; the override was checked with `cargo build -v`, which shows `overflow-checks=on` by default and no flag under the override). The run-time cost of the helpers is not measured (§11.6). |
| Panic and unwinding | Unwind tables and panic-message code in the binary; the exit status a panic produces.                                                                                           | `run.rs` line 80 ends a trap with `process::exit(101)` rather than unwinding, so a trap does not depend on unwinding. A panic on the program's thread does (§3.3, PC-1).                                                                                                                                             |

### 3.3 What Mzizi adopts and avoids

**Adopts:**

- **Plain, non-generic Rust in the output.** The foundation slice has no generics, so the
  lowering pays no monomorphisation cost by construction. This is already how `run.rs` works.
- **One package cache per source, rebuilt only when the text changes**, which `run.rs` already
  does, and a cache keyed by what the program means (§9.2), which it does not yet.

**Avoids:**

- **Generic-heavy output.** It multiplies code for every instance, and the lowering can avoid it.
- **Unmeasured `rustc` flags.** A flag goes in only with a size or time measurement behind it.
- **A panic strategy change without the exit contract.** `CLAUDE.md` states that `mz run` exits 101
  for a panic on `main`'s thread. `panic = "abort"` would end such a process by `SIGABRT`
  instead, which is 128 plus 6. So `panic = "abort"` is not adopted until the exit contract is
  amended (§14 Q2). Its size effect may still be measured.

### 3.4 The changes that follow

- **PC-1, panic strategy for a lowered program.** `panic = "abort"` in the generated profile.
  Behaviour changes only for a panic on `main`'s thread, so it waits for §14 Q2. Gate:
  `benchmarks/perf/run.sh --check-only` for correctness, and the size of `mz-fib` in §10 M4 for
  the size effect. Status: not started; the size effect is to be measured.
- **PC-2, a package cache keyed by content.** See §9.2. Gate: a test that an edit which changes
  only a comment, including one that moves lines, reuses the package (§9.2 says why moving lines is
  the hard case), and a cold-to-warm timing on the timing machine.

---

## 4. tokio

### 4.1 What Mzizi ships today

A lowered service is a tokio program. `lower.rs` writes `#[tokio::main]` with the multi-thread
runtime (features `rt-multi-thread`, `macros` and `net`, line 123), and binds a `TcpListener`
on `127.0.0.1` (lines 888–905). The request handler is `async fn handle` (line 874), but the work
of a request is `dispatch` (line 795), which is **synchronous**: it routes the request, runs the
service's routes and builds the reply without awaiting anything. So the runtime's job for a request
is to accept a connection and poll the future that hyper and axum spawn for it. No handler waits
on I/O or on another task.

A lowered program has no tokio at all. `run.rs` line 237 runs `main` on one standard thread with a
64 MiB stack (the depth of the recursive programs needs it), and nothing else runs.

### 4.2 Cost centres (the project's design, to be cited to a pinned revision)

These come from tokio's public design and are not yet read from pinned source (§1).

- **The work-stealing scheduler.** Each worker has a run queue, and idle workers steal from busy
  ones. Its cost is synchronisation between workers and wakeups across threads.
- **Task allocation.** Each spawned task is a heap allocation. A server spawns one per
  connection.
- **`Send + 'static`.** A spawned future must be `Send` and own its data. A program that wants to
  share state has to use `Arc` and a lock. Mzizi's source has no ownership at the surface
  (RFC-0013 §14.1), so the lowering chooses the representation, and the bound is a cost only if the
  lowering spawns.
- **Runtime start-up.** The multi-thread runtime starts its worker threads when `main` starts.

**Measured here:** nothing about tokio in isolation. The B1 reference, plain axum and tokio, cold
builds in 24.6 s (§11.2). A request latency or throughput comparison is not measured, because the
perf suite covers programs and not services, and the repository has no load driver for a service
(§11.6, §10 M9).

### 4.3 What Mzizi adopts and avoids

**Adopts:**

- **A synchronous request path.** A lowered service's handler does its work in place, as
  `dispatch` already does. A runtime that polls only the accept loop, a current-thread runtime, does
  the same work with fewer threads. Whether it is faster is a measurement (§10 M9), not a claim.
- **One routing semantics.** The runtime choice does not change routing. RFC-0011 §6 defines the
  routing, and `lower.rs` says axum is the server and not the router, so there is one routing semantics (HD-5).

**Avoids:**

- **Spawning per request.** Dispatch in place.
- **`Send + 'static` as a constraint on Mzizi values.** The source has no ownership, so the bound
  would only add cost.
- **Forking tokio** (§12).

### 4.4 The change that follows

- **PC-3, a current-thread runtime for a service whose handlers never wait.** `lower.rs` builds the
  runtime with `tokio::runtime::Builder::new_current_thread` when the service has no await point
  in its dispatch path. Gate: `mzprobe verify` for B1 (correctness against the Mzizi and axum
  references, which CI runs), and the request latency and throughput of B1 on the timing machine
  (§10 M9). It changes the `PINS` features, which must stay in step with B1's manifest (`CLAUDE.md`).
  RFC-0016 Q13 would also change the tokio features (adding `signal`, for graceful shutdown), so the
  two changes touch the same line and should be decided together.
  Status: not started; §14 Q8 decides whether to measure first.

### 4.5 Single-threaded hosts

A WebAssembly host that runs one thread (the Workers model; RFC-0016 §4.4 designs that target, and it is not built) cannot run tokio's
multi-thread runtime. For such a target, a lowered service needs no tokio: the synchronous
`dispatch` is the whole handler, and the host's event loop supplies the I/O. This is a design. No
such target exists (`AGENTS.md`: "no Workers, WebAssembly or Containers target"). §5.3 gives the
I/O side, and §6 the output side.

---

## 5. mio

### 5.1 Where Mzizi meets it

mio is the non-blocking, cross-platform OS event layer that tokio is built on: epoll on Linux,
kqueue on the BSDs and macOS, IOCP on Windows. Mzizi meets it only through tokio, in the generated
service (`lower.rs` line 894). A program does no I/O beyond `print`.

**Measured (§11.4):** the generated service does not build for `wasm32-unknown-unknown`. The build
stops with `This wasm target is unsupported by mio. If using Tokio, disable the net feature.`, and
then with unresolved imports from mio's `sys` module. So any lowered service that uses tokio's
`net` feature is native-only. (The claim is about the mio version that tokio 1.53.1 resolves to in this build.)

### 5.2 Cost centres (the project's design)

- **Readiness polling.** One `epoll_wait` (or the equivalent) per wakeup, and the registration of
  each socket with it.
- **Buffer allocation per read**, in hyper rather than in mio, but on the same path.
- **Wakeups across threads**, when the runtime is multi-thread (§4.2).

Not measured here. The cost of a server's I/O needs a load driver (§10 M9).

### 5.3 What Mzizi adopts and avoids

**Adopts:** I/O as a **host-provided trait**. A lowered program or service never names a socket. It
calls an interface that the host implements: a native host implements it over the operating system,
a single-threaded host implements it over its own event loop, and a test host implements it over a
fixture. This is the direction the owner cites in registry #502, which this repository does not read
(Repo boundaries, [AGENTS.md](../AGENTS.md)), and it is the open question [RFC-0014](./RFC-0014-tier2.md) §2.6 (a draft, on `staging`) leaves
for capabilities. RFC-0016 (three deployment targets, a draft in #123, design approved by the owner
on 2026-10-21) takes a related but different route for Workers: it keeps the target-neutral core
(`dispatch` and the reply type) and replaces the tokio and axum glue per target (RFC-0016 §4.4), and it
says what the I/O adapters must provide (RFC-0016 §5.4). §14 Q13 asks how the two fit.

**Avoids:** a socket type in the language's surface, and mio in any generated package that targets
WebAssembly.

### 5.4 The change that follows

- **PC-4, the listener behind a trait.** `lower.rs` puts the listener behind a `Listener` trait with
  one native implementation (tokio and mio, the current behaviour), and the generated tests keep
  using `tower::oneshot`, which needs no socket at all. Gate: the generated `#[tokio::test]`s and
  `mzprobe verify` for B1 pass unchanged, and the generated package builds for `wasm32` with the
  native implementation removed (§10 M7). Status: blocked on RFC-0014 §2.6 (§14 Q11), and its shape on §14 Q13.

---

## 6. The WebAssembly output

### 6.1 What was built and measured

`mz build benchmarks/perf/programs/fib.mz` writes a package. `cargo build --release --target
wasm32-unknown-unknown` builds it, with no change to the package, on the installed target. The
hand-written reference in [`benchmarks/perf/rust/`](../benchmarks/perf/rust/) builds the same way.
The same is true for `interpolate`. The builds are unstripped, with the default release profile,
on one toolchain (§11.3).

| Build                      | File bytes |   Code section |  Data section |   Name section | Functions |
| -------------------------- | ---------: | -------------: | ------------: | -------------: | --------: |
| lowered `fib`              |     48,263 | 30,885 (64.0%) |  3,470 (7.2%) | 13,249 (27.5%) |       156 |
| hand-written `fib`         |     60,047 | 37,076 (61.7%) | 8,282 (13.8%) | 14,004 (23.3%) |       168 |
| lowered `interpolate`      |     50,978 | 32,899 (64.5%) |  3,782 (7.4%) | 13,618 (26.7%) |       160 |
| hand-written `interpolate` |     63,427 | 39,215 (61.8%) | 8,622 (13.6%) | 14,879 (23.5%) |       178 |

The lowered builds are smaller in this table. The RFC does not claim why, and does not claim that
the lowered code is smaller in general: two programs, one toolchain, one machine. The function
counts are the functions with bodies that the section reader found. The reader is a stdlib-only
Python script, kept outside the repository, that reads the file's own section table and its `name`
section. It was used because `wasm-opt` and `twiggy` are not installed here (§11.6).

The largest functions, lowered `fib` (§11.3): dlmalloc's `malloc` at 5,099 bytes (10.6% of the
file), `__rdl_realloc` at 949, `core::str::count::do_count_chars` at 875, dlmalloc's `free` at
870, `Formatter::pad_integral` at 793, `Formatter::pad` at 670, `core::fmt::write` at 628, and
`Once::call` at 613. In this build, the allocator and the formatting path together are the costliest named code.

### 6.2 Cost centres

| Cost centre             | What it costs                                                                        | Measured                                                                                                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The allocator           | The symbols show `std`'s dlmalloc; any `String` or `Vec` growth brings it in.        | `malloc`, `free` and `realloc` together: 6,918 bytes in lowered `fib`, about 14% of the file (§11.3).                                                                                                            |
| The formatting path     | `core::fmt`, for `{…}` interpolation of integers and text, and the padding it calls. | `pad_integral`, `pad`, `fmt::write`: 2,091 bytes in lowered `fib`. Whether `int` text goes through `core::fmt` is read from the top functions, not from the code.                                                |
| Panic and location data | Message and location strings in the data section.                                    | Data section 3,470 bytes in lowered `fib`, against 8,282 in the reference. The reference's top functions include `slice_error_fail_rt` and `Debug::fmt`, which are panic and debug paths. Not explained further. |
| The name section        | Present in an unstripped build, and absent from a stripped one.                      | 27.5% of lowered `fib`. It is not shipped unless it is left in.                                                                                                                                                  |
| `wasm-bindgen` glue     | Not used. The output is a raw module, so there is no JavaScript glue.                | Not applicable. The host ABI is designed (§6.3), not built.                                                                                                                                                      |
| `wasm-opt` and `twiggy` | Binaryen's size and optimisation passes, and a per-function size breakdown.          | Not installed on this machine and not run (§11.6, §14 Q1).                                                                                                                                                       |

### 6.3 What Mzizi adopts and avoids

**Adopts:**

- **Report size with and without the name section**, both figures, beside each other. Stripping is
  a fact about a build, not a result.
- **A raw module with a small host ABI** (a few imports) rather than generated JavaScript glue. The
  ABI is designed; it is not built.

**Avoids:**

- **`wasm-bindgen` as a dependency of a generated program package.** The glue would be a second layer
  in every output; its size is not measured here. Not adopted unless a measurement shows the ABI costs
  more. **This conflicts, for services, with the charter and RFC-0016:** CHARTER.md §2 and §4 name
  `workers-rs` for the Workers target, and RFC-0016 §4.4 (design approved by the owner) lowers a service
  for Workers to a `workers-rs` crate. `workers-rs` is built on `wasm-bindgen` (to be confirmed from
  its pinned manifest, RFC-0016 Q8). So this avoidance can hold for a program and for a raw module, not
  for a service on the Workers target as approved. §14 Q12 asks the owner which applies.
- **An allocator swap in the first iteration.** This RFC does not choose one. An alternative
  allocator is a separate decision, measured when it is proposed.
- **Debug formatting in generated code.** The reference's `Debug::fmt` path is in its binary, and
  the lowering should not add one.

**To verify before any change depends on it:** the default panic strategy of
`wasm32-unknown-unknown` is recorded by `rustc --print target-spec-json --target wasm32-unknown-unknown`,
and that output, not this paragraph, is the source for whether `panic = "abort"` is already the
default there.

### 6.4 The changes that follow

- **PC-5, a `wasm32` build mode for a program.** `mz build` writes a release profile that strips the
  symbols for the wasm target, and it reports both sizes (§6.3). Gate: the size budget in §10 M4 and
  M5, which is deterministic for one toolchain.
- **PC-6, integer text without `core::fmt`.** A small writer for `int` in the generated runtime, if
  the top functions above are confirmed to come from `int` text. Gate: `benchmarks/perf` output
  identical to `programs/interpolate.expected` (a correctness gate), and the size of
  `mz-interpolate` lower than the figure in the table (a size gate). §14 Q6 decides whether the
  runtime grows.

---

## 7. Dioxus

### 7.1 What is decided, and where it conflicts

Dioxus is a Rust UI framework. It builds a virtual DOM, tracks state through a signals runtime, and
hydrates server-rendered markup on the client. A Roots component is a contract and a rendered element
tree, and no component lowers today (`AGENTS.md`, "no component lowers").

The decided position is in [CHARTER.md](../CHARTER.md): §2 makes Mzizi Roots the component model and
names Dioxus "a compatible third-party rendering target", one of the two frontend paths ("pure Rust end
to end"), and §4's Phase 1 renders `mzizi-ui` "through a compatible third-party engine (Dioxus today)".
So under the charter Dioxus is a path a Roots component may be rendered through, not only a comparison.

The owner's brief for this RFC cites registry #506 for a position of server render plus attach, not a
client framework. This repository does not read the registry (Repo boundaries), so that is cited, not
verified. The site repository's rule is no UI framework in the browser ([`mzizi-site`](https://github.com/mzizi-dev/mzizi-site)
`AGENTS.md`), which is a rule for that site, not for every Mzizi page. The "avoids" in §7.4 are
therefore a proposal for the server-render path only, and they do not remove Dioxus as a rendering
target, which only a charter amendment could do (§14 Q15).

### 7.2 Cost centres (the project's design)

- **Virtual-DOM diffing.** Building a tree and comparing it with the last one on each state change:
  allocation and comparison per update.
- **The signals runtime.** Subscribers, dependency tracking and invalidation, per signal.
- **Hydration.** The client walks server-rendered markup to attach handlers, which costs on load
  and carries the runtime that does it.
- **The payload.** The WebAssembly or JavaScript a page loads before it is interactive.

Each is to be cited to Dioxus's pinned source before a change depends on it (§1).

### 7.3 Measured here

Nothing. [`benchmarks/arms/dioxus`](../benchmarks/arms/dioxus/) is a benchmark arm for how well a
model writes a task, and its own workspace root, excluded from the repository's workspace. It is not
a performance arm. This RFC does not measure Dioxus. A page's payload size would be the first
measurement, and it is deferred (§10).

### 7.4 What Mzizi adopts and avoids

**Adopts:** static rendering of a component to HTML at build time, with the client doing nothing it
does not have to. The Roots model is already close: a rendered tree is data.

**Avoids, on the server-render path only (§7.1):** a client-side diff engine, a signals runtime in the
page, and hydration as a required step. The charter's Dioxus rendering target is unchanged.

### 7.5 The change that follows

- **PC-7, none in this repository.** The render path for Roots components lives in the registry and
  its crates, which this repository does not build (Repo boundaries). The RFC names it and changes
  nothing here. Gate, when it is proposed: the crates' own tests and the site's rendered-content gate.

---

## 8. The harness and contracts as optimisation inputs

The owner's request names "code knowledge through our harness and contracts". Each example below
says whether it **exists** (the source or the emitted code is in this repository today) or is
**designed** (it is not built).

### 8.1 A check a constant proves: exists in the output, removal designed

[`examples/records.mz`](../examples/records.mz) declares a record with an `always` clause
(lines 24–26, `always low <= high`). The one construction in the example, `Span { low: 2, high: 9 }`,
lowers to a construction followed by a call to `mz_check`. This was checked with `mz build
examples/records.mz` (the emitted check is at line 617 of the generated `src/main.rs`; it is the only construction and the only check in the file):

    let s: Span = ({ let mz_value = Span { low: 2i64, high: 9i64 }; mz_value.mz_check(&MZ_AT_2); mz_value });

The check tests two constants. Today the lowering emits the call anyway (PT-4). **Designed:** a pass
in the lowering that evaluates an `always` clause over constant fields and drops the call when it
holds. The proof is of the same kind that `contract.rs` already gives component contracts
(RFC-0006). Extending it to a program's `always` is what RFC-0013 §11.4 describes, and it is not
built. A constant `always` that **fails** is a different case: the program is wrong on every run.
§14 Q7 says it should be a compile error, not a trap.

### 8.2 Integer checks a range proves: exists in the output, removal designed

Every `int` operator in a lowered program is a checked helper. The `fib` program
([`benchmarks/perf/programs/fib.mz`](../benchmarks/perf/programs/fib.mz)) lowers to these calls
(generated `src/main.rs` from `mz build`: the calls are at lines 560 and 568, and the helpers
`mz_add`, `mz_sub` and `mz_rem` are defined at lines 30, 37 and 64):

- `fib(mz_sub(n, 1i64, &MZ_AT_2))` and `fib(mz_sub(n, 2i64, &MZ_AT_3))`, in the recursive branch.
- `mz_add(…)` for the sum of the two calls.
- `mz_rem(f, 1000003i64, &MZ_AT_1)` for the checksum.

**Designed, partly provable.** Inside the recursive branch, the guard `when n < 2` has already
returned, so on the fall-through `n >= 2`. Then `n - 2 >= 0` and `n - 1 >= 1`, so neither
subtraction can overflow, and the two `mz_sub` checks can go. The `mz_add` cannot be removed: the
sum of two `int` results can overflow for a large enough `n`, and the compiler does not know the
range of `n`. The `mz_rem` by a non-zero constant other than -1 cannot trap: a remainder by a
constant divisor needs no zero or minus-one check. A rule keyed on a constant divisor removes it.
This is the goal that `benchmarks/perf/README.md` already states ("remove them where the compiler
can prove they are unnecessary"), and it is not built.

**Gate:** the `fib` program's output identical to `programs/fib.expected` (correctness), and its
timing on the timing machine (§10 M11). The size effect is measured now, and it is nil for the
profile setting (§11.3).

### 8.3 Fixed layouts from known sizes: designed

The lowering emits plain Rust structs and enums (`run/records.rs`, `run.rs`), so each layout is
rustc's. **Designed:** a lowering that knows a value's size and its uses can choose a representation
that the Rust struct cannot express. A candidate is an enum that is only matched and never stored
in a collection, whose payload could be inline. None of this is measured, and none of it is built.

### 8.4 Validation a contract already guarantees: exists for services, designed in general

The generated service holds no `unwrap`, `expect`, indexing or `panic!` on a request path
(`lower.rs` header, line 14, from RFC-0011 §5). Its `example` clauses are run twice: by
`mz contract` in process (`serve.rs`), and as the generated `#[tokio::test]`s (`lower.rs` line 538),
so the request path is already tested against the same clauses. **Designed:** where a service's
contract states a precondition on an input, a route that already checks that input could skip a
second check. The rule is that a check comes out **only** because a clause or a type proves it, never
because an example passed (§8.6).

### 8.5 The harness as one table of operators: exists, not yet read by lowering

[`compiler/src/harness.rs`](../compiler/src/harness.rs) (lines 10–18) states that each operator's
spelling, precedence and operand types come from one source, which the checker reads
(`binary_type`), and that the numeric and text methods come from `numbers::method` and
`text::method`. **Designed:** a lowering rule, such as the constant-divisor rule in §8.2, keyed by
operator and written once, read from the same source as the checker. No lowering reads `harness.rs`
today.

### 8.6 What the harness must never do

- **Remove a check because a test passed.** A test shows an input; a proof covers every input.
- **Change what a trapping program prints or exits with.** The `.expected` files and the `mz run`
  exit statuses pin that, and a removed check must keep them.

---

## 9. The fast development loop

### 9.1 The measured baseline

From §11.1, §11.2 and §11.5, on one machine:

- `mz check`, per file, process start included: 4.7 to 12.3 ms, median 7.9 ms, over 20 files.
- `mz build examples/registry.mz`, lowering only: 7 to 21 ms, median 16 ms.
- A cold `cargo build --release` of the lowered registry service: 34.3 s.
- `mz run benchmarks/perf/programs/fib.mz`, the default debug profile: 2.2 s cold, 1.2 to 1.4 s warm.
  The warm figure includes running `fib(38)` in a debug build, so it is not a build time.

On this machine the front end took milliseconds and the back end took seconds. The loop an agent runs most is `mz check` and
then `mz run`. In these figures the part that costs is cargo's, which points at rebuilding less rather than at the
front end.

### 9.2 A content-hash cache of the IR: partly exists, not used by build or run

**Exists:** `ir.rs` gives each node a SHA-256 identity over its canonical form (`ir.rs` line 81,
`sha256(&self.canonical())`), and `mz hash` prints the root hash (`main.rs` line 273). **It covers
components only.** Measured: `mz hash` on a program exits 2 with ``mz: `hash` does not cover programs yet;
`check`, `fix` and `contract` do``, and on a component prints the root hash (`mz hash primitives/button.mz`
gives `6a1550cae374  button  27 nodes`).

**Today:** `run.rs` line 1015 keys the package by a hash of the absolute source path, and `write`
rewrites only the files whose text changed. Checked on 2026-10-21 with `mz build` on
`benchmarks/perf/programs/fib.mz`: an edit to the text of an existing comment line writes a
byte-identical package, so `write` changes nothing and cargo rebuilds nothing. A comment that adds or
removes a line does change the package, because every trap site carries its source line and column
(the `MZ_AT_*` constants, for example `MzAt { file: "fib.mz", line: 17, col: 16, text: "n - 1" }`),
and those move. A second copy of the same program in another directory builds again, because the key
is the path.

**Designed:** key the lowered package by the root hash of the checked program, so that two sources
with the same meaning share one package, and an edit that moves no meaning reuses it. Two things stand
in the way. The trap sites' lines and columns are part of what a trapping program prints, so either
they stay in the key or they move out of the generated code into a table the package reads at run time
(§14 Q14). And it needs an IR that covers programs (RFC-0003 §1 names the properties; the program IR is not built). It is PC-2 (§3.4).

### 9.3 The development backend against LLVM: designed

Today `mz run` builds the lowered package with cargo's default debug profile, and `--release` uses
LLVM's optimising pipeline (`run.rs`, the `--release` flag at line 1046). Three options:

- **A. A code-generation backend for a dev build, such as Cranelift.** Fast to generate code, less
  optimised. It would be a separate tool, because the compiler crate has no dependencies
  (`AGENTS.md`, "Build, test, run").
- **B. An interpreter for the IR, in tree.** No native code, no dependencies. `serve.rs` already runs a
  service in process, so the semantics exist in one place for the interpreter to follow.
- **C. Keep cargo's debug build, and rebuild less** (§9.2, §6).

**Recommendation (§14 Q9):** C now, since the baseline above is cheap; then B for test runs, since it
keeps one set of semantics; A only if the measured cold loop is what limits an agent. The gate for
any of them is the time from a `mz check --agent` error to a passing `mz run`, over a fixed set of
edits, measured on the timing machine (§10 M12). That measure does not exist yet.

### 9.4 Release

`--release` stays on LLVM. Nothing here changes it.

---

## 10. Measurement plan

The plan uses [`benchmarks/perf/`](../benchmarks/perf/README.md) and extends it only where the
suite cannot measure a thing. Three rules come from its README. Correctness is gated in CI. Timing
is never gated in CI, because a shared runner is too noisy, and it is measured on the timing
machine. Every number names its machine, its `rustc` and its commit.

Sizes are deterministic for one toolchain, so they can be gated against a budget. A budget changes
only in the pull request that changes the toolchain (§14 Q4).

| #   | Measurement                                                    | Command or method                                                                                          | Status                                                   |
| --- | -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| M1  | `mz check` wall time over the corpus                           | `mz check` per file in `primitives/` and `examples/`, 7 runs each, median (§11.1)                          | Taken.                                                   |
| M2  | Cold `cargo build` of a lowered service                        | `mz build examples/registry.mz`, then `cargo build --release` in an empty target directory (§11.2)         | Taken.                                                   |
| M3  | Cold `cargo build` of the B1 reference                         | `cargo build --release` in an empty target directory (§11.2)                                               | Taken.                                                   |
| M4  | WebAssembly size breakdown of a lowered program                | `cargo build --release --target wasm32-unknown-unknown`, then the section reader (§11.3)                   | Taken for `fib` and `interpolate`.                       |
| M5  | WebAssembly size, stripped                                     | As M4, with symbols stripped for the target (PC-5)                                                         | Not taken.                                               |
| M6  | `wasm-opt` size and `twiggy` breakdown                         | Binaryen's `wasm-opt -Oz` and `twiggy top` on the M4 files                                                 | Not taken. Needs §14 Q1.                                 |
| M7  | Whether the generated service builds for WebAssembly           | `cargo build --release --target wasm32-unknown-unknown` in the generated package (§11.4)                   | Taken. Fails, as §5.1 says.                              |
| M8  | Native binary size and cold build, programs                    | The perf suite's `binary_bytes` and `build_ms`, from `benchmarks/perf/run.sh`                              | Not run for this RFC. The suite is the gate.             |
| M9  | Request latency and throughput of B1                           | A load driver against the generated service on the timing machine                                          | Not taken. No driver is in the repository. Needs §14 Q8. |
| M10 | `mz run` cold and warm                                         | `MZ_CACHE_DIR=<dir> mz run`, three runs (§11.5)                                                            | Taken.                                                   |
| M11 | Run time with and without the constant-divisor and range rules | The perf suite's `fib` and `collatz`, timed on the timing machine, before and after the rule (§8.2)        | Not taken. The rule is not built.                        |
| M12 | Edit-to-run time for an agent                                  | A fixed set of edits, each followed by `mz check --agent` and `mz run`, timed on the timing machine (§9.3) | Not taken. The set does not exist yet.                   |

**The gate for each change in §3 to §8:** PC-1 by M4 (size) and the perf suite's correctness; PC-2 by
a cache test and M10 on the timing machine; PC-3 by M9 and `mzprobe verify`; PC-4 by M7 and the
generated tests; PC-5 by M4 and M5; PC-6 by the `interpolate` output and M4; PC-7 is outside this
repository.

---

## 11. Measurements taken for this RFC

**Machine (all of §11):** Intel(R) Xeon(R) Processor at 2.80 GHz, 4 cores, 15 GiB RAM, Linux
6.18.44-fc-v139 x86_64, a shared virtual machine. The load average was between about 4 and 8 during
the runs, which is why §11 gives medians and ranges and no single figures. `rustc 1.97.0
(2d8144b78 2026-07-07)`, `cargo 1.97.0 (c980f4866 2026-06-30)`, with the `wasm32-unknown-unknown`
target installed. **Commit:** `eed51a1` (the head of `origin/staging` when the work started). **Date:**
2026-10-20.

**One machine, one commit, one toolchain.** These are not a comparison between stacks: no other stack
was measured in the same run, and they are not a claim about any other machine. The commands are
given so that the timing machine can repeat them.

### 11.1 `mz check` over the corpus (M1)

`mz` was built with `cargo build --release --bin mz` in `compiler/` (54 s). Then, for each file in
`primitives/*.mz` and `examples/*.mz` (9 and 11 files, 1,094 lines), seven runs of `mz check <file>`,
the median wall time per file, process start included.

| Statistic                       | Value                                       |
| ------------------------------- | ------------------------------------------- |
| Files                           | 20                                          |
| Median per file                 | 7.90 ms                                     |
| Fastest file                    | 4.69 ms (`examples/fib.mz`)                 |
| Slowest file                    | 12.27 ms (`examples/changelog_renderer.mz`) |
| Sum of the 20 medians           | 156.25 ms                                   |
| Any file exiting with 2 or more | None                                        |

### 11.2 Lowering, and cold builds (M2, M3)

- `mz build examples/registry.mz --out <dir>`, five runs: 7, 12, 16, 16 and 21 ms; median 16 ms.
  The output is `Cargo.toml`, `src/main.rs` (653 lines) and `fixtures/ui.json`.
- `cargo build --release` of that package, in an empty target directory (`--target-dir`): 34.3 s
  wall, 49.7 s user; the target directory is 98 MB. The crates came from the local cache, and the
  versions are the exact pins (axum 0.8.9 and tokio 1.53.1).
- `cargo build --release` of `benchmarks/tasks/b1-routing/reference-rust`, in another empty target
  directory: 24.6 s wall, 55.9 s user; 114 MB.

Two separate target directories, so the second build did not reuse the first's compiled
dependencies. One run each.

### 11.3 WebAssembly size (M4)

Commands: `mz build benchmarks/perf/programs/<name>.mz --out <dir>`, then `cargo build --release
--target wasm32-unknown-unknown --target-dir <dir>` in that package. The hand-written references:
`cargo build --release --target wasm32-unknown-unknown --offline --bin fib --bin interpolate` in
`benchmarks/perf/rust/`. The table is §6.1. The breakdown is from a stdlib-only Python reader that
reads the module's own section table, and the `name` section's function names, with `python3 -I`.
Its per-function sizes are the size of each function body as the module records it; inlined copies are not separated.

Overflow checks (§3.2): the same lowered `fib` built with `CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=false`
is 48,263 bytes, the same as with the profile setting. The verbose build confirms the override
reached `rustc`: the default passes `overflow-checks=on`, and the override passes no flag.

### 11.4 The generated service for WebAssembly (M7)

`cargo build --release --target wasm32-unknown-unknown` in the generated `registry` package. It fails
at mio with `This wasm target is unsupported by mio. If using Tokio, disable the net feature.`, then
with unresolved imports from mio's `sys` module.

### 11.5 `mz run` on `fib` (M10)

`MZ_CACHE_DIR=<dir> mz run benchmarks/perf/programs/fib.mz`, three runs, the default debug profile:
2,213 ms (cold: the package is written and cargo builds it), then 1,187 ms and 1,372 ms (warm). The
warm figures include running `fib(38)` in a debug build, so they are not build times. The program
printed `fib(38) is 39088169` and `checksum 88052`, as `programs/fib.expected` requires.

### 11.6 What was not measured

- Native binary sizes and run times of the perf suite. The suite was not run, because it times on
  the timing machine and not on this shared one (its README says so).
- Request latency and throughput of any service. The repository has no load driver for one.
- Anything about tokio, mio or Dioxus in isolation, or any upstream source.
- `wasm-opt`, `twiggy` and a stripped build. Not installed or not run.
- The run-time cost of the checked helpers, with or without the constant-divisor or range rules.

---

## 12. Non-goals

- **Rewriting rustc or LLVM.** Mzizi lowers to Rust, and the backend it has is cargo's. A development
  backend is an option in §9.3, not a rewrite of a compiler.
- **Forking tokio, mio, Dioxus or `wasm-bindgen`.** Mzizi depends on them only as pinned crates, as it
  does today.
- **A claim of speed before a measurement.** The owner's "best optimal" is the direction this RFC
  works in, not a result it has.
- **A claim about Workers, WebAssembly components or containers.** No such target exists. §4.5, §5.3
  and §6 are designs.
- **Changing the exit-status contract** of `mz run` without amending it (§3.3, §14 Q2).
- **A comparison of Mzizi with another language on a model-written task.** That is the kill-criterion
  benchmark of [RFC-0009](./RFC-0009-comparison-benchmark.md), not this.

---

## 13. What this RFC does not claim

- No speed, size or latency claim about Mzizi against any stack. The §11 numbers are one machine's, with
  their commands.
- No statement that a cost centre in §3 to §7 dominates a real program's time. The teardowns say where
  the cost is. The measurements in §10 say how much it is, and only for the programs they name.
- The upstream designs in §3 to §7 come from the projects' public design and are **not yet cited to pinned
  source** (§1). Each subject's teardown owes that citation before a change depends on it.
- The registry (#502, #506) and the site repository's rules are cited, not verified: this RFC does not read either repository.
- The lowered WebAssembly being smaller than the hand-written one in §6.1 is two programs on one
  machine. It is not a finding about the approach.

---

## 14. Open questions for the owner

Each has a recommendation. The owner decides.

**Q1. Install `wasm-opt` (Binaryen) and `twiggy` for the size work (§6, M5 and M6)?** Recommendation:
yes, on the timing machine, pinned there. Neither goes into the compiler crate, which has no dependencies
(`AGENTS.md`).

**Q2. Panic strategy for a lowered program (PC-1).** `panic = "abort"` would change a panic on `main`'s
thread from exit 101 to a signal, 128 plus 6. Recommendation: keep unwinding until the owner amends
`mz run`'s exit contract (`CLAUDE.md`), and record the size effect now, so the decision has a figure.

**Q3. Attribution for any code taken from the five projects.** No code is copied in this RFC.
Recommendation: a future copy keeps its licence header and its commit names the upstream file and
revision, and a `NOTICE` entry is added in the same pull request. The owner confirms the policy.

**Q4. A size budget in CI for the WebAssembly build (M4, M5).** Recommendation: gate the stripped size of
the fixed `fib` program against a budget set from the measured figure with a small margin, and change the
budget only in the pull request that changes the toolchain. This adds a CI job, so it is the owner's call.

**Q5. Who runs `benchmarks/perf/` for timing, and where?** Recommendation: the private
`mzizi-benchmarks` repository, which the perf README already names, checking this one out. CI stays
correctness-only. The owner names who runs that machine.

**Q6. A hand-written integer writer in the generated runtime (PC-6).** Recommendation: measure the top
functions' source first. The gain is bytes, and the output must stay byte-identical to the `.expected`
files. The owner decides whether the generated runtime grows.

**Q7. A constant `always` that fails (§8.1).** Recommendation: a compile error, since the program is wrong on
every run. A trap stays for a value that depends on input. This is a language decision that RFC-0013 §11.4
would need to amend.

**Q8. A current-thread runtime for a service (PC-3, M9).** The change alters the `PINS` features, which
must stay in step with B1's manifest (`CLAUDE.md`). Recommendation: approve a load driver first (M9), and
measure before the runtime changes. A driver is new code and needs the owner's approval.

**Q9. The development backend (§9.3).** Options A, B and C. Recommendation: C now, B next, and A only if the
measured loop requires it. The owner decides the order.

**Q10. Where the teardowns live.** Recommendation: keep each subject's teardown in this RFC until it has
measurements beyond one machine, then split it into `design/teardown/<subject>.md`. The acceptance
criterion of #117 ("a teardown doc per subject") is met by the sections now, and by files later.

**Q11. The I/O capability.** The host trait in §5.3 depends on RFC-0014 §2.6, which is an open question
in that draft. Recommendation: decide the capability design there first, then PC-4.

**Q12. `wasm-bindgen` and the Workers target (§6.3).** RFC-0016 §4.4, approved as design, lowers a
service for Workers to `workers-rs`, which is built on `wasm-bindgen`; §6.3 avoids `wasm-bindgen` in a
generated package. Recommendation: scope §6.3's avoidance to programs and raw modules, and keep
RFC-0016's `workers-rs` path for services, measuring its glue's size when that wave is built. The owner
decides.

**Q13. One I/O seam or two (§5.3, PC-4).** PC-4 puts the listener behind a `Listener` trait in one
generated package; RFC-0016 §4.4 keeps `dispatch` target-neutral and emits different glue per
`--target`, and RFC-0016 §5.4 sets what adapters must provide. Recommendation: follow RFC-0016's
per-target glue, and drop PC-4 or restate it as a check that the core stays free of tokio, axum and
mio symbols. The owner decides.

**Q14. Trap locations and a content-keyed cache (§9.2, PC-2).** Every trap site embeds its line and
column, so a comment that moves lines changes the generated code. Recommendation: decide whether
locations stay in the generated code (and in the cache key) or move to a table, before PC-2 is
designed further. The owner decides.

**Q15. The Dioxus rendering target (§7).** The charter names Dioxus a compatible rendering target and
Phase 1's engine; the brief's registry #506 points at server render plus attach. Recommendation: keep
the charter's text, and treat §7.4 as a proposal for the server-render path only, unless the owner
amends the charter. The owner decides.

---

## 15. Reconciliation with the other RFCs

- **RFC-0011 §8 and §6.** The service lowering. PC-3 and PC-4 change `lower.rs`, and the routing semantics
  stay as RFC-0011 §6 defines them.
- **RFC-0013 §11.4, §13.1 and §14.** `always` and `mz run`. PC-1, PC-2 and §8.1 change how a program lowers and
  caches, and Q2 and Q7 are decided against those sections.
- **RFC-0006.** The contract proofs §8.1 would reuse are the same kind `contract.rs` gives components.
- **RFC-0012.** The harness's single operator table (§8.5) is the source a lowering rule would read.
- **RFC-0003 §1.** The content-addressed identity that §9.2 needs for programs, which is not built.
- **RFC-0014 §2.6.** The capability question that §5.3 and PC-4 depend on (Q11).
- **RFC-0016 §4.4, §5.4 and Q13** (a draft in #123, design approved by the owner, nothing built). §4.5
  agrees with its Workers design (no tokio, `dispatch` as the handler). §6.3's `wasm-bindgen` avoidance
  conflicts with its `workers-rs` lowering for services (Q12), PC-4 overlaps its per-target glue (Q13),
  and PC-3 and its Q13 both change the tokio features.
- **CHARTER.md §2 and §4.** `workers-rs` for Workers (Q12) and Dioxus as a rendering target (§7.1, Q15).
  This RFC proposes no charter change.

Nothing in this list is amended by this RFC.
