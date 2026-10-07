# RFC-0012 — The harness: the core of the language, what the agent reads

**Status:** draft for review. **Nothing here is implemented beyond what the code already does**,
and §2 says exactly which parts that is. Everything else is design.
**Author:** the machine author (Claude), from the owner's direction of 2026-09-30.
**Scope:** what the harness is; which existing parts of the language it already covers (RFC-0001
§4, RFC-0003); what an agent reads through it; the plugin host, its manifest and its lifecycle;
how `mz` exposes it; how external clients attach; and how the agent skills fold into it. It
does not design new syntax, a lowering to Rust, or the clients themselves.

> **Owner direction, 2026-09-30:** "The harness is the core part of the language. It's what the
> agent reads, not just the plugins but the language itself." And: "The toolchain connects to
> the harness natively, easily. The CLI does the same thing." And: "The harness lives in the
> language repo." And, on the agent skills: "Keep the skills, then fold them in."

**Naming.** In this RFC, and everywhere in this repository, **the harness** means the thing this
RFC specifies. `benchmarks/harness/` (`mzizi-benchmark-harness`, the Phase 0 scorer) is a
different thing, and is always called **the benchmark harness**. It is a client of the harness
(§6), not a part of it.

## 1. What the harness is

The harness is the layer of the language an agent reads and works through. It has three parts:

1. **The language as an agent reads it.** The grammar, the types, the contract forms and the
   canonical form, stated as one machine-readable definition, versioned with the language. This
   is what an agent needs to write correct Mzizi without reading the RFCs.
2. **The agent protocol.** What an agent sends the language and gets back: RFC-0001 §4's
   `mz check --agent` NDJSON, its diagnostics with `exact` and `guess` fixes, `mz fix`, the
   contract results of `mz contract`, and the read surface of RFC-0003 (`mz outline`, `mz ir`,
   `mz hash`).
3. **The plugin host.** The one place the toolchain (`mz` and its subcommands), the CLI, the MCP
   server and plugins attach, so that each gets the same language definition and the same
   protocol rather than a copy of it.

It is **the core of the language, not the whole of it.** The syntax and semantics are the
language as well. The difference is audience: the RFCs define the language for the people who
design it, and the harness presents it to the agents who write it.

**The toolchain is not the language.** `mz` is an implementation, in Rust, of what the language
and the harness specify. It could be replaced without changing a Mzizi program or the protocol
an agent sees, which is why the protocol is specified here and not in the binary.

## 2. What exists today, and what does not

Checked against `compiler/src/main.rs` and the test suite on `main` at `63a9066`.

| Part                                 | State today                                                                                                                                                                                                                                                                             |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Agent protocol: diagnostics          | **Exists.** `mz check --agent <file>` prints one NDJSON diagnostic per line (`code`, `severity`, `file`, `span`, `say`, optional `fix` with `confidence` `exact` or `guess`), then one summary line (`errors`, `warnings`, `exact_fixable`, `ms`). Tested in `cargo test`, gated in CI. |
| Agent protocol: fixes                | **Exists.** `mz fix <file>` applies every `exact` fix in one pass, writes the file and re-checks it (RFC-0001 §4.3).                                                                                                                                                                    |
| Agent protocol: contracts            | **Exists.** `mz contract [--agent] <file>` evaluates the contract block against the file's own declarations (RFC-0006).                                                                                                                                                                 |
| Read surface (RFC-0003)              | **Partly exists.** `mz outline`, `mz ir` and `mz hash` exist. RFC-0003 §5's query and patch surface does not.                                                                                                                                                                           |
| The language as an agent reads it    | **Design only.** No single machine-readable definition exists. Today an agent learns Mzizi from the skills (§7) and from the benchmark guides (`benchmarks/prompts/*-guide.md`), both written by hand.                                                                                  |
| The plugin host, manifest, lifecycle | **Design only.** Nothing in `mz` loads, lists or calls a plugin.                                                                                                                                                                                                                        |
| `mz harness` (§5)                    | **Design only.** No such subcommand exists.                                                                                                                                                                                                                                             |

As for Rust: `mz build` lowers a `service` to a local Rust + axum package; no component lowers, and there is no Workers, WebAssembly or Containers target. The harness does not depend on lowering: every part
above works on the language as it is checked now.

## 3. RFC-0001 §4 and RFC-0003 are parts of the harness

This RFC does not replace either. It names them as two of the harness's existing parts and puts
one versioned boundary around them.

- **RFC-0001 §4, the agent protocol**, is the harness's write loop: generate, check, read the
  diagnostic, apply the fix, check again. Its rules stand as written: whole-program and
  deterministic, one diagnostic per true author error, `say` readable with zero file context,
  fixes as data, and compile speed as a protocol property.
- **RFC-0003, the content-addressed IR**, is the harness's read surface: what an agent loads
  for the code it is not editing (`mz outline` for dependencies, `mz ir` and `mz hash` for
  identity). Its measurements (RFC-0003 §7) are the IR's own properties, not the charter's claim.

What the harness adds is a **protocol version** that covers both. A client that attaches (§6)
asks for a protocol version and gets exactly that shape back, so a change to the diagnostic
format or the outline format is a version change a client can see, rather than a silent break.

## 4. The plugin host

### 4.1 What a plugin is

A plugin is a named unit that attaches to the harness and adds one of these: a command (a new
`mz` subcommand or tool), a check (extra diagnostics under its own code range), a view (a new
read representation), or a knowledge pack (agent-facing text generated from the language
definition, such as a skill). A plugin cannot change the language: it cannot add syntax, alter a
built-in diagnostic, or relax a contract.

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

## 5. How `mz` exposes the harness

A proposed `mz harness` subcommand, **not implemented**:

- `mz harness version`: the protocol version and the language version.
- `mz harness definition`: the language as an agent reads it (§1, part 1), as JSON.
- `mz harness plugins`: the attached plugins and their manifests.

The existing commands (`check`, `fix`, `contract`, `outline`, `ir`, `hash`) stay where they are.
They become the harness's built-in commands without changing their output.

## 6. How clients attach

The harness has clients. They attach to it and do not host it.

| Client                              | Where                   | How it attaches (proposed)                                                                |
| ----------------------------------- | ----------------------- | ----------------------------------------------------------------------------------------- |
| `mzizi-cli`                         | `mzizi-dev/agent-tools` | runs `mz` and reads the protocol, pinned to a protocol version                            |
| `mzizi-mcp` (`mcp.mzizi.dev`)       | `mzizi-dev/agent-tools` | exposes the harness's commands and definition as MCP tools, generated from the definition |
| fundi (the console's agent tooling) | `mzizi-dev/agent-tools` | the same, through `mzizi-cli` or `mzizi-mcp`                                              |
| the benchmark harness and runner    | `benchmarks/`           | the runner already runs `mz check --agent`, and the harness scores the result; no change  |

This repository's rule holds in the harness's direction as well: **everything else consumes
Mzizi; Mzizi consumes nothing else** (RFC-0004 §3). The harness never calls a client, and its CI
never needs one.

## 7. The skills fold into the harness

Today an agent learns Mzizi from the skills: the npm package `@nyuchi/mzizi-skills`, the MCP's
`mzizi_get_skills` tool, `api.mzizi.dev`'s `/v1/skills`, and the registry's plugin. They are four
copies of hand-written text, kept current by the freshness rule (AGENTS.md).

Once the harness exists, what the skills teach comes from the harness itself: the language as
an agent reads it (§1, part 1) and the agent protocol (§3). **Keep the skills as they are now,
then fold them in** (owner, 2026-09-30).

- **What has to exist first:** `mz harness definition` (§5), with a test that it matches what
  the compiler accepts, so the definition cannot drift from the checker.
- **Until then** the skills stay the source of what an agent is taught, and must track the
  language under the freshness rule.
- **After that**, the skills are either retired or generated from the harness. Which one is
  open (§9). **Recommended:** generate them from the harness during a transition, so all four
  existing copies keep working while their content stops being written by hand.

## 8. Prior art

_Extended on 2026-10-07 (#48). Each line is from the system's own documentation, read that
day, and says what this RFC takes from it and where it differs. The survey is a reading of
documents. It measures nothing, and no system below was run against Mzizi._

**`mzizi-dev/agent-tools`' `docs/rfc-harness-plugins.md`** (draft, 2026-09-30, "a plugin-first
harness that updates with the language") designs the clients' side. Its own header now defers to
this RFC where the two differ. What this RFC takes from it: six first-party plugins; **static
composition** before any dynamic loading; a `mzizi-language` plugin that makes "updates with the
language" mechanical; and one pin, `language.pin.json`, shared by the harness and the skill. This
RFC cites that document by its section headings and its summary as relayed on 2026-09-30, not by
a full reading. A full reading was attempted on 2026-10-07 and could not be made: the session
doing this survey was refused access to the private repository. Its details must still be checked
against it before either RFC is accepted.

| System                                                                       | What RFC-0012 takes                                                                                                                                                                                                                                                                                                                                                                          | How it differs                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language Server Protocol, and rust-analyzer                                  | One protocol that many clients speak, and a handshake (`initialize`) in which client and server exchange capabilities before anything else. That is the model for §3's protocol version and for §9, question 6.                                                                                                                                                                              | LSP belongs to no language: it is shaped for editors (documents, cursor positions, hover), and its specification sits outside every language that uses it. rust-analyzer is a second front end, with its own parser and type inference, and it gets rustc's own diagnostics by running `cargo check`. So one Rust program can be read two ways. The harness has one implementation behind every client, and an LSP server for Mzizi would be a client of it (§6), not the harness itself. |
| Roslyn (the .NET Compiler Platform)                                          | "Compilers become platforms": the compiler's own pipeline (syntax trees, symbols, the semantic model) is the public API, and the IDE features are built on it. User-defined analyzers plug into a compilation, and their diagnostics appear next to the compiler's in both the build (MSBuild) and the editor. That is §4.1's "check" plugin, seen by every client.                          | An in-process .NET API, not a wire protocol: a client has to run inside the .NET runtime. Analyzers come in with a project's references (NuGet packages), not from one checked-in list (§4.3). Roslyn's API is documented as the compiler's, not as part of C#.                                                                                                                                                                                                                           |
| The TypeScript language service (`tsserver`)                                 | The editor service and `tsc` share one checker, so they give the same type errors.                                                                                                                                                                                                                                                                                                           | Its plugins are "for changing the editing experience only" and "aren't loaded during normal commandline typechecking or emitting, (so are not loaded by tsc)". So an error a plugin adds is shown in the editor and never fails a build. **That is the split this RFC must not repeat.** A plugin's check (§4.1) has to appear in `mz check --agent` for every client, the CLI included, or the harness gives agents two answers.                                                         |
| `rustc --error-format=json`, `cargo --message-format=json`                   | Diagnostics as data, one JSON object per line, tagged with their kind (`$message_type` in rustc, `reason` in cargo), each fix tagged with how safe it is to apply (`MachineApplicable`, `MaybeIncorrect`, `HasPlaceholders`, `Unspecified`), and `cargo fix` applying the safe ones. RFC-0001 §4's NDJSON with `exact` and `guess` fixes, and `mz fix`, are the same design with two levels. | Documented as compiler output, in the rustc and Cargo books, not in the language reference. Neither format carries a version field. Cargo does version `cargo metadata` (`--format-version`), but not its build messages. **This is the strongest counter-example to the claim**: Rust has this protocol and does not call it part of the language. What this RFC adds is the version (§3) and a definition of the language to go with it (§1, part 1).                                   |
| MCP servers for languages and toolchains                                     | MCP's `initialize` negotiates a `protocolVersion` (a date, such as `2025-11-25`) and capabilities, the same pattern as LSP. Agents already attach to languages this way: `mcp-language-server` proxies any stdio LSP server and exposes definition, references, rename and diagnostics as tools, and Unison ships an MCP server for typechecking, docs and dependencies.                     | These servers wrap a tool from outside it. A generic bridge such as `mcp-language-server` inherits LSP's editor shape, and its tools are only as good as the language server behind it. In this RFC `mzizi-mcp` stays a client (§6), and its tools are generated from the harness's definition rather than written next to it.                                                                                                                                                            |
| Unison and its codebase manager (`ucm`)                                      | The closest precedent for the claim. Each definition "is identified by a hash of its syntax tree", the codebase is a store of those definitions, and `ucm` is how code is added to it, so the tool and the language are hard to separate. `ucm` serves LSP and MCP from the same codebase. RFC-0003's content-addressed IR (`mz hash`) shares the idea.                                      | In Unison the tie follows from where code lives: in the codebase, not in text files. Mzizi keeps text files and derives hashes on demand, so it cannot inherit that tie. It has to state it, which is what §1 and §3 try to do.                                                                                                                                                                                                                                                           |
| Plugin hosts with static composition (Go's `golang.org/x/tools/go/analysis`) | Analyzers are values in a list written in source, combined into one driver (`multichecker`). Each analyzer declares the analyzers it `Requires`, and the driver runs those first. That is §4.3's discover and resolve: a checked-in list, ordered by `requires`.                                                                                                                             | Go composes at compile time, so a new analyzer means a new binary. §4.3 proposes reading manifests from a checked-in list when `mz` starts, which is still static but needs no rebuild. Go's analyzers can add diagnostics but cannot change Go. §4.1 sets the same limit.                                                                                                                                                                                                                |

**What this says about the claim.** The RFC claims the harness is part of the language, not
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
claim is a design goal, and from the outside the harness looks like the tooling in the table.

## 9. Open questions

1. **Decided: where the harness lives.** In this repository, `mzizi-dev/mzizi`, which specifies
   and implements it (owner, 2026-09-30). The `agent-tools` packages are clients.
2. **Open: the implementation language.** Rust is recommended, consistent with the compiler.
3. **Open: the shape of the language definition** (§1, part 1): JSON generated from the parser's
   own tables, or a hand-written file checked against the compiler by a test.
4. **Open: dynamic plugin discovery**, or static composition only.
5. **Open: whether the skills are retired or generated** (§7). Generation is recommended for the
   transition.
6. **Open: how the protocol version relates to the language version**: one number, or two.
7. **Open: the plugin sandbox**: what a third-party plugin may read and run.
8. **Open: an LSP server for Mzizi** (§8, the LSP row): whether one is built, as a client of the
   harness, so that editors attach the way agents do.
