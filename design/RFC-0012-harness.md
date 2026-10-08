# RFC-0012 — The language harness: the spine of the language

**Status:** draft for review. **Nothing here is implemented beyond what the code already does**,
and §2 says exactly which parts that is. Everything else is design.
**Author:** the machine author (Claude), from the owner's direction of 2026-09-30, and amended
from the owner's direction of 2026-10-07.
**Amended** on 2026-10-07 (Refs #69): renamed **the language harness**; §1 restated as the spine,
with the harness entry (§1.1) and the registration rule (§1.2); §2, §5, §7 and §9 updated; §9
question 3 decided; a question added on measuring whether a harness-taught agent does better.
**Scope:** what the language harness is; what one feature registers in it; which existing parts
of the language it already covers (RFC-0001 §4, RFC-0003); what an agent reads through it; the
plugin host, its manifest and its lifecycle; how `mz` exposes it; how external clients attach;
and how the agent skills and the benchmark guides come from it. It does not design new syntax,
a lowering to Rust, or the clients themselves.

> **Owner direction, 2026-09-30:** "The harness is the core part of the language. It's what the
> agent reads, not just the plugins but the language itself." And: "The toolchain connects to
> the harness natively, easily. The CLI does the same thing." And: "The harness lives in the
> language repo." And, on the agent skills: "Keep the skills, then fold them in."

The owner renamed it **the language harness** a week later, and widened it:

> **Owner direction, 2026-10-07** (dictated; lightly edited here for punctuation and repeated
> words, with the meaning kept): "The harness is not just for plug-ins to plug into. It is also
> its own kind of core function of the language. The language's own functions tie into it: it
> is like a central chain in the language. … It makes other things plug into it easily. Our
> toolchain is plugged into it, the various pieces of it are plugged into it, the component
> harness is plugged into it. … It's not a wrapper around the language. It is part of the
> language, so the language itself is plugged into it. This is what will improve the
> benchmarks: not compiling against individual things, but comparing using the harness."
>
> "An agent using the language picks up the harness, and the moment it picks up the harness it
> knows what to do and how to use the language. That's why the skills can be folded into the
> harness easily. We release a new function, and it plugs into the harness right away: our
> boundaries, our strings, our arithmetic, everything like that. The harness is the central
> spine, internal and external, for the language."
>
> And, the same day: "The harness work should be part of the build. What's the point of building
> a language if it doesn't know what to attach to?"

The owner expects the language harness to improve the benchmarks. **That is a hypothesis, and
this RFC does not claim it.** Nothing here has been measured, and §9 question 9 says how the
benchmark is designed to test it.

**Naming.** In this RFC, and everywhere in this repository, **the language harness** means the
thing this RFC specifies; "the harness" alone, in a sentence about RFC-0012, means the same.
`benchmarks/harness/` (`mzizi-benchmark-harness`, the Phase 0 scorer) is a different thing, and
is always called **the benchmark harness**. It is a client of the language harness (§6), not a
part of it.

## 1. The spine

The language harness is the spine of the language. **Every feature of the language is defined
once, as a harness entry, inside the compiler**: each type, with its operators and methods; each
statement form; each declaration kind (`program`, `component`, `service`); each diagnostic code
and its fixes; and each `mz` command. Everything else is generated from those entries, or
checked against them by a test:

- the checker's own tables, where that is practical (§1.2);
- `mz harness definition`, the language as an agent reads it (§5);
- the agent skills and the benchmark guides an agent is given (§7).

Three things attach to the spine, and none of them holds a second copy of the language:

1. **The toolchain.** `mz check`, `fix`, `contract`, `outline`, `ir`, `hash`, `build` and `run`
   are entries themselves. The design is that the toolchain, and the component and service
   machinery (the evaluator of RFC-0006 and RFC-0010, the in-process server and the lowering of
   RFC-0011), read the language from the same registry. In the first slice only `mz`'s command
   dispatch and the program parser's type names read it; the checker keeps its own rules, and
   §1.2's tests hold the two together (§2).
2. **The agent protocol**, what an agent sends the language and gets back: RFC-0001 §4's
   `mz check --agent` NDJSON, its diagnostics with `exact` and `guess` fixes, `mz fix`, the
   contract results of `mz contract`, and the read surface of RFC-0003 (`mz outline`, `mz ir`,
   `mz hash`). Every code in it is an entry (§1.1).
3. **External clients and plugins**: the CLI, the MCP server and third-party plugins (§4, §6).
   A first-party feature and a third-party plugin register through the same interface, an
   entry. The difference is §4.1's boundary: a third-party plugin may add, and may not change
   the language's meaning.

It is **part of the language, not a wrapper around it.** The RFCs define the language for the
people who design it; the entries present the same language to the agents who write it, and a
test fails when the two disagree with the checker. The syntax and semantics are still the
language: the harness is where they are registered, not a replacement for them.

**The toolchain is not the language.** `mz` is an implementation, in Rust, of what the language
and its harness specify. It could be replaced without changing a Mzizi program, the entries, or
the protocol an agent sees, which is why the entry and the protocol are specified here and not
in the binary.

### 1.1 The harness entry

A feature registers one entry. An entry holds:

| Field                 | What it holds                                                                                                                                                                      |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **name** and **kind** | A name unique across the registry (`int`, `let`, `is not`, `mz run`, `MZ0905`) and its kind: declaration, type, operator, statement, function, lexical, command, diagnostic        |
| **grammar**           | The forms an author writes, in canonical form (RFC-0001 §3, RFC-0013 §17)                                                                                                          |
| **teach**             | One paragraph for an agent: what it is, how to write it, and the mistakes from other languages it rejects                                                                          |
| **types**             | Its type rules in brief; for an operator, its precedence level as well                                                                                                             |
| **codes**             | The diagnostic codes it reports, each one an entry of its own with its `say` text, its severity, the fix kinds it carries (`exact`, `guess` or none) and a source that triggers it |
| **examples**          | Runnable examples: source that `mz check` accepts, and, for a program, the output `mz run` prints. **Examples are tested**, so an entry cannot teach code that does not run        |
| **rfc**               | The sections that design it                                                                                                                                                        |

A code's fix kinds are recorded **as raised**. When two `exact` fixes overlap, `mz check` keeps
one and demotes the other to a `guess` (RFC-0008 §6), so an agent can receive a `guess` on a code
whose entry lists only `exact`.

An entry describes what is **built**. A form an RFC designs and the compiler does not build has no
entry of its own; the diagnostic that rejects it (`MZ0919` for RFC-0013's later waves) is the
entry an agent reads, and says the form is designed and not built.

An entry may be **kind-level** only: its name, kind and a summary, with the details in its RFCs.
That is a stage, not an exemption: §2 says which entries are kind-level today, and their codes
stay on an explicit, documented pending list until each one has its entry.

### 1.2 The registration rule

**A pull request that adds or changes a language feature adds or updates its harness entry in
the same pull request**, as it adds a CHANGELOG entry and updates its LANGUAGE-TRACKER row. A
tracker row is not done until its harness entries are.

The rule is enforced, not only written down. A test fails:

- when the compiler can emit a diagnostic code that has no entry and is not on the pending list,
  or when an entry names a code nothing emits;
- when the checker accepts a construct with no entry: the statement and expression forms the
  parser builds are matched to their entries exhaustively, so a new form does not compile until
  it names its entry, and the test checks that every name it gives is registered;
- when an entry's trigger does not report its code, at its severity, with a declared fix kind;
- when an example does not check, or a program example does not print its stated output.

Where the checker can read its own facts from the registry, or the registry can read them from
the checker, it does, so there is nothing to drift. Everything else is a parallel copy, written
in the entry and in the checker, and the tests above are what hold the two together. §2 says
which is which today.

## 2. What exists today, and what does not

Checked against `compiler/src/main.rs`, `compiler/src/harness.rs` and the test suite in the pull
request that adds the first slice (Refs #69).

| Part                                 | State today                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent protocol: diagnostics          | **Exists.** `mz check --agent <file>` prints one NDJSON diagnostic per line (`code`, `severity`, `file`, `span`, `say`, optional `fix` with `confidence` `exact` or `guess`), then one summary line (`errors`, `warnings`, `exact_fixable`, `ms`). Tested in `cargo test`, gated in CI.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Agent protocol: fixes                | **Exists.** `mz fix <file>` applies every `exact` fix in one pass, writes the file and re-checks it (RFC-0001 §4.3).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Agent protocol: contracts            | **Exists.** `mz contract [--agent] <file>` evaluates the contract block against the file's own declarations (RFC-0006).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Read surface (RFC-0003)              | **Partly exists.** `mz outline`, `mz ir` and `mz hash` exist, for components. RFC-0003 §5's query and patch surface does not.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Harness entries (§1.1)               | **First slice exists.** `compiler/src/harness.rs` registers `program` and everything RFC-0013 builds so far: `int`, `float`, `bool` and `text`; the 13 binary and 2 prefix operators; the 11 numeric methods of §4.4; `fn`, `let`, `var`, assignment, `when`, `return`, expression statements, `print`, text literals, comments and names; 52 diagnostic codes, each `MZ09xx` a program raises and the shared codes it reuses, each with a trigger; and the nine `mz` commands. `component` and `service` are **kind-level** only, and their 55 codes are on a frozen pending list. Tested in `compiler/tests/harness.rs`.                                                                                                                                                                                                                                                                                                                                                                                                              |
| The registration rule (§1.2)         | **Partly a single source, partly enforced by tests.** One source: `mz` dispatches on the command entries; the parser accepts exactly the surface types' names; operator spelling, precedence and operand types, the operator and type lists (generated with each enum) and the methods' signatures are read from the checker. Parallel copies, checked by tests: each code's severity and fix kinds (a debug build checks every report against them, fix kinds included), and each feature's examples. Written by hand and not compared with the checker: each code's `say` text and each feature's grammar and teaching text; only a length cap on `say` is tested. The tests fail on an emitted code with no entry and not pending, a trigger that stops reporting its code, an example that stops checking or running as stated, and a lexed operator with no entry; the pending list is frozen and may only shrink. Exhaustive matches make a new statement, expression, operator or type fail to compile until it names its entry. |
| The language as an agent reads it    | **First slice exists.** `mz harness definition` prints the registry as deterministic, hand-serialised JSON. The skills (§7) and the benchmark guides are still written by hand, and are not generated from it yet.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| The plugin host, manifest, lifecycle | **Design only.** Nothing in `mz` loads, lists or calls a plugin.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `mz harness` (§5)                    | **Exists:** `version`, `definition [--agent]` and `entry <name>`. `plugins` does not.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |

As for Rust: `mz build` lowers a `service` to a local Rust + axum package, and `mz build` and
`mz run` lower a `program` in RFC-0013's foundation slice and its Wave 1 numbers (`float` and the
numeric methods) to a dependency-free Rust package; no
component lowers, and there is no Workers, WebAssembly or Containers target. The language harness
does not depend on lowering: every part above works on the language as it is checked now.

## 3. RFC-0001 §4 and RFC-0003 are parts of the language harness

This RFC does not replace either. It names them as two of the language harness's existing parts and puts
one versioned boundary around them.

- **RFC-0001 §4, the agent protocol**, is the language harness's write loop: generate, check, read the
  diagnostic, apply the fix, check again. Its rules stand as written: whole-program and
  deterministic, one diagnostic per true author error, `say` readable with zero file context,
  fixes as data, and compile speed as a protocol property.
- **RFC-0003, the content-addressed IR**, is the language harness's read surface: what an agent loads
  for the code it is not editing (`mz outline` for dependencies, `mz ir` and `mz hash` for
  identity). Its measurements (RFC-0003 §7) are the IR's own properties, not the charter's claim.

What the language harness adds is a **protocol version** that covers both. A client that attaches (§6)
asks for a protocol version and gets exactly that shape back, so a change to the diagnostic
format or the outline format is a version change a client can see, rather than a silent break.

## 4. The plugin host

### 4.1 What a plugin is

A plugin is a named unit that attaches to the language harness and adds one of these: a command (a new
`mz` subcommand or tool), a check (extra diagnostics under its own code range), a view (a new
read representation), or a knowledge pack (agent-facing text generated from the language
definition, such as a skill). A plugin cannot change the language: it cannot add syntax, alter a
built-in diagnostic, or relax a contract.

A plugin attaches the way a first-party feature does: by registering entries (§1.1). The
language's own features are registered through the same interface, so a plugin reads and
extends the same registry the checker uses. What a third party may not do is this section's
boundary: its entries add commands, checks under its own code range, views and knowledge
packs, and never replace or alter a first-party entry. Only a pull request to this repository,
under §1.2, changes what the language means.

### 4.2 The manifest

Each plugin carries a manifest. A proposed shape, written as TOML to match `arm.toml`
(`benchmarks/runner/README.md`), and open for review:

```toml
name = "mzizi-language"        # unique; lowercase, hyphenated
version = "0.1.0"
harness = ">=1, <2"            # the protocol versions it works with (§3)
kind = ["knowledge", "command"]
provides = ["mz_language_definition", "mz_check"]
requires = []                  # other plugins, by name
```

The host refuses a plugin whose `harness` range excludes the running protocol version, whose
`name` is taken, or whose `requires` cannot be met. An unknown key is an error, as it is in
`arm.toml`.

### 4.3 Lifecycle

1. **Discover:** the host reads the manifests it is given. Composition is **static** first
   (plugins named in a checked-in list), following the prior art (§8); dynamic discovery is an
   open question (§9).
2. **Resolve:** it checks versions and `requires`, and orders plugins so a dependency loads
   before its dependant. A cycle is an error.
3. **Attach:** each plugin receives the language definition and the protocol version.
4. **Serve:** the host routes commands and checks to plugins.
5. **Detach:** on shutdown, or when a plugin is disabled.

Each step's failure is reported in the agent protocol's own NDJSON form, so an agent reads a
plugin failure the way it reads a diagnostic.

## 5. How `mz` exposes the language harness

`mz harness`. `version`, `definition` and `entry` are **implemented** (§2); `plugins` is not:

- `mz harness version`: the protocol version (§3), the language version and the SHA-256 of the
  one-line definition, as one JSON line. The crates stay at `0.0.0` and releases are git tags, so
  the first slice names the language by its phase and the RFC-0013 waves built
  (`phase-0, RFC-0013 wave 0 and wave 1 numbers`),
  and the definition's hash pins its exact content: it changes whenever an entry does, with no
  number to bump by hand. Question 6 (§9) stays open for the owner.
- `mz harness definition [--agent]`: the language as an agent reads it (§1): every entry, in a
  stable order, as deterministic JSON. `--agent` prints it on one line.
- `mz harness entry <name>`: one entry, by its name (`mz harness entry int`,
  `mz harness entry is not`, `mz harness entry MZ0905`).
- `mz harness plugins`: the attached plugins and their manifests (§4), once a plugin can attach.

Each exits as the other commands do: 0 on success, 2 for a usage problem, which includes a name
with no entry. The existing commands (`check`, `fix`, `contract`, `outline`, `ir`, `hash`,
`build`, `run`) stay where they are. They are the language harness's built-in commands, each one
an entry, without changing their output.

## 6. How clients attach

The language harness has clients. They attach to it and do not host it.

| Client                              | Where                   | How it attaches (proposed)                                                                         |
| ----------------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------- |
| `mzizi-cli`                         | `mzizi-dev/agent-tools` | runs `mz` and reads the protocol, pinned to a protocol version                                     |
| `mzizi-mcp` (`mcp.mzizi.dev`)       | `mzizi-dev/agent-tools` | exposes the language harness's commands and definition as MCP tools, generated from the definition |
| fundi (the console's agent tooling) | `mzizi-dev/agent-tools` | the same, through `mzizi-cli` or `mzizi-mcp`                                                       |
| the benchmark harness and runner    | `benchmarks/`           | the runner already runs `mz check --agent`, and the language harness scores the result; no change  |

This repository's rule holds in the language harness's direction as well: **everything else consumes
Mzizi; Mzizi consumes nothing else** (RFC-0004 §3). The language harness never calls a client, and its CI
never needs one.

## 7. The skills and the benchmark guides come from the definition

Today an agent learns Mzizi from the skills: the npm package `@nyuchi/mzizi-skills`, the MCP's
`mzizi_get_skills` tool, `api.mzizi.dev`'s `/v1/skills`, and the registry's plugin. They are four
copies of hand-written text, kept current by the freshness rule (AGENTS.md). The benchmark guides
an agent is given (`benchmarks/prompts/*-guide.md`) are a fifth, also written by hand.

Once the definition exists, **the skills and the benchmark guides are generated from it**: from
the entries' grammar, teaching text, type rules, codes and tested examples (§1.1), and the agent
protocol (§3). A feature that lands registers its entry (§1.2), and from then on every copy an
agent reads says the same thing, because none is written by hand. **Keep the skills as they are
now, then fold them in** (owner, 2026-09-30).

- **What has to exist first:** `mz harness definition` (§5), with the registration rule's tests
  (§1.2), so the definition cannot drift from the checker.
- **Until then** the skills and the guides stay the source of what an agent is taught, and must
  track the language under the freshness rule.
- **After that**, the skills are generated from the definition. All four existing copies keep
  working during the transition, while their content stops being written by hand; whether they
  are later retired is §9 question 5.
- **The benchmark guides** are generated the same way. A benchmark run records the definition's
  hash (§5) it was given, so a result names the language it measured. A change to a generated
  guide is a change to what the benchmark measures, and is re-measured in `BUDGET.md` like any
  hand edit.

## 8. Prior art

_Extended on 2026-10-07 (#48). Each line is from the system's own documentation, read that
day, and says what this RFC takes from it and where it differs. The survey is a reading of
documents. It measures nothing, and no system below was run against Mzizi._

**`mzizi-dev/agent-tools`' `docs/rfc-harness-plugins.md`** (draft, 2026-09-30, "a plugin-first
harness that updates with the language") designs the clients' side. Its own header now defers to
this RFC where the two differ. What this RFC takes from it: six first-party plugins; **static
composition** before any dynamic loading; a `mzizi-language` plugin that makes "updates with the
language" mechanical; and one pin, `language.pin.json`, shared by the language harness and the skill. This
RFC cites that document by its section headings and its summary as relayed on 2026-09-30, not by
a full reading. A full reading was attempted on 2026-10-07 and could not be made: the session
doing this survey was refused access to the private repository. Its details must still be checked
against it before either RFC is accepted.

| System                                                                       | What RFC-0012 takes                                                                                                                                                                                                                                                                                                                                                                          | How it differs                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language Server Protocol, and rust-analyzer                                  | One protocol that many clients speak, and a handshake (`initialize`) in which client and server exchange capabilities before anything else. That is the model for §3's protocol version and for §9, question 6.                                                                                                                                                                              | LSP belongs to no language: it is shaped for editors (documents, cursor positions, hover), and its specification sits outside every language that uses it. rust-analyzer is a second front end, with its own parser and type inference, and it gets rustc's own diagnostics by running `cargo check`. So one Rust program can be read two ways. The language harness has one implementation behind every client, and an LSP server for Mzizi would be a client of it (§6), not the language harness itself. |
| Roslyn (the .NET Compiler Platform)                                          | "Compilers become platforms": the compiler's own pipeline (syntax trees, symbols, the semantic model) is the public API, and the IDE features are built on it. User-defined analyzers plug into a compilation, and their diagnostics appear next to the compiler's in both the build (MSBuild) and the editor. That is §4.1's "check" plugin, seen by every client.                          | An in-process .NET API, not a wire protocol: a client has to run inside the .NET runtime. Analyzers come in with a project's references (NuGet packages), not from one checked-in list (§4.3). Roslyn's API is documented as the compiler's, not as part of C#.                                                                                                                                                                                                                                             |
| The TypeScript language service (`tsserver`)                                 | The editor service and `tsc` share one checker, so they give the same type errors.                                                                                                                                                                                                                                                                                                           | Its plugins are "for changing the editing experience only" and "aren't loaded during normal commandline typechecking or emitting, (so are not loaded by tsc)". So an error a plugin adds is shown in the editor and never fails a build. **That is the split this RFC must not repeat.** A plugin's check (§4.1) has to appear in `mz check --agent` for every client, the CLI included, or the language harness gives agents two answers.                                                                  |
| `rustc --error-format=json`, `cargo --message-format=json`                   | Diagnostics as data, one JSON object per line, tagged with their kind (`$message_type` in rustc, `reason` in cargo), each fix tagged with how safe it is to apply (`MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`), and `cargo fix` applying the safe ones. RFC-0001 §4's NDJSON with `exact` and `guess` fixes, and `mz fix`, are the same design with two levels. | Documented as compiler output, in the rustc and Cargo books, not in the language reference. Neither format carries a version field. Cargo does version `cargo metadata` (`--format-version`), but not its build messages. **This is the strongest counter-example to the claim**: Rust has this protocol and does not call it part of the language. What this RFC adds is the version (§3) and a definition of the language to go with it (§1, part 1).                                                     |
| MCP servers for languages and toolchains                                     | MCP's `initialize` negotiates a `protocolVersion` (a date, such as `2025-11-25`) and capabilities, the same pattern as LSP. Agents already attach to languages this way: `mcp-language-server` proxies any stdio LSP server and exposes definition, references, rename and diagnostics as tools, and Unison ships an MCP server for typechecking, docs and dependencies.                     | These servers wrap a tool from outside it. A generic bridge such as `mcp-language-server` inherits LSP's editor shape, and its tools are only as good as the language server behind it. In this RFC `mzizi-mcp` stays a client (§6), and its tools are generated from the language harness's definition rather than written next to it.                                                                                                                                                                     |
| Unison and its codebase manager (`ucm`)                                      | The closest precedent for the claim. Each definition "is identified by a hash of its syntax tree", the codebase is a store of those definitions, and `ucm` is how code is added to it, so the tool and the language are hard to separate. `ucm` serves LSP and MCP from the same codebase. RFC-0003's content-addressed IR (`mz hash`) shares the idea.                                      | In Unison the tie follows from where code lives: in the codebase, not in text files. Mzizi keeps text files and derives hashes on demand, so it cannot inherit that tie. It has to state it, which is what §1 and §3 try to do.                                                                                                                                                                                                                                                                             |
| Plugin hosts with static composition (Go's `golang.org/x/tools/go/analysis`) | Analyzers are values in a list written in source, combined into one driver (`multichecker`). Each analyzer declares the analyzers it `Requires`, and the driver runs those first. That is §4.3's discover and resolve: a checked-in list, ordered by `requires`.                                                                                                                             | Go composes at compile time, so a new analyzer means a new binary. §4.3 proposes reading manifests from a checked-in list when `mz` starts, which is still static but needs no rebuild. Go's analyzers can add diagnostics but cannot change Go. §4.1 sets the same limit.                                                                                                                                                                                                                                  |

**What this says about the claim.** The RFC claims the language harness is part of the language, not
tooling attached to it. None of the six systems above specifies its machine interface as part of
its language. LSP and MCP belong to no language. rustc's JSON and Roslyn's APIs are documented as
compiler interfaces. TypeScript has two interfaces that disagree on plugins. Unison comes
closest, and there the tie follows from storing code by hash, not from a definition. So the
survey does not support the claim as a description of established practice. It narrows what the
claim has to mean here. Three things would make it more than a label: (1) the protocol and a
machine-readable language definition are specified in this repository and versioned with the
language (§1, §3), not documented as one binary's output; (2) one implementation serves every
client, so no client gets a different answer (the TypeScript row); (3) a test fails when the
definition and the checker disagree (§7). None of the three exists yet (§2). Until they do, the
claim is a design goal, and from the outside the language harness looks like the tooling in the table.

## 9. Open questions

1. **Decided: where the language harness lives.** In this repository, `mzizi-dev/mzizi`, which
   specifies and implements it (owner, 2026-09-30). The `agent-tools` packages are clients.
2. **Open: the implementation language.** Rust is recommended, consistent with the compiler.
3. **Decided (2026-10-07): the shape of the language definition** (§1). It is generated from
   the compiler's own registered entries (§1.1), not a hand-written file checked against the
   compiler. A hand-written file is a second copy, and the owner's direction is that the
   language's own features plug into the harness; the registration rule's tests (§1.2) hold the
   entries to the checker.
4. **Open: dynamic plugin discovery**, or static composition only.
5. **Open: whether the skills are retired or generated** (§7). Generation is decided for the
   transition; whether the four copies are later retired in favour of the definition alone is
   open.
6. **Open: how the protocol version relates to the language version**: one number, or two.
7. **Open: the plugin sandbox**: what a third-party plugin may read and run.
8. **Open: an LSP server for Mzizi** (§8, the LSP row): whether one is built, as a client of the
   language harness, so that editors attach the way agents do.
9. **Open: whether an agent taught by the language harness does better.** The owner expects the
   harness to improve the benchmarks (2026-10-07). That is a hypothesis for the benchmark to
   test, not a result, and nothing in this repository has measured it. The design for testing
   it: a benchmark arm whose guide is generated from `mz harness definition` (§7), compared with
   an arm given today's hand-written guide, on the same tasks and model, scored by the benchmark
   harness on RFC-0009's metrics. Until that run exists and is reported as it fell, no document
   here may say the harness improves anything.
