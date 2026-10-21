# RFC-0016 — Three deployment targets: Workers, containers and bare metal, and the lowering each one needs

**Status:** draft; the design is approved by the owner, design only (2026-10-21, Q1). **Nothing in this RFC is implemented.** No command, flag, generated file,
CI job or charter passage described here exists yet. The charter text in §3 is a proposal: it takes
effect only when the owner approves it, and CHARTER.md is not edited by this document. Each wave in §9
names the tests that would make its part true, and until those tests are on `main`, the tracker rows in
§8 stay as they are.
**Author:** the machine author, for the owner, on issue [#122](https://github.com/mzizi-dev/mzizi/issues/122).
**Scope:** how a Mzizi `service` and a `program` reach a running process. It covers the three deployment
targets (§2), the proposed charter amendment (§3), the lowering each target needs (§4), how a backend on
Cloudflare and a backend on Fly reach each other (§5), what the language must not decide about regions
(§6), what CI must check (§7), the tracker rows (§8), a build order (§9), non-goals (§10) and open
questions (§11). It does not design the standard library (RFC-0014 §2), state or I/O (P8), or concurrency
(P9); §5.4 says what those designs must provide so that a service stays target-neutral.

> **Proposes to amend**, none of it applied by this document:
>
> - **CHARTER.md** §1 (the sentence that names "Cloudflare Workers and Containers" as the server and edge
>   targets, line 77), §2 (the Full-stack row, line 92, and the Edge row, line 93) and §4 (the Phase 1
>   paragraph, line 124). §3 gives the replacement text and the version bump.
> - **RFC-0011 §8.1**: `mz build` gains `--target`, and a generated service reads `HOST` beside `PORT`. The
>   default stays `127.0.0.1`, so a native build behaves as it does today except where §4.2 says so.
> - **LANGUAGE-TRACKER.md P11**: its evidence is corrected in this PR, because it says a service is the only
>   thing that lowers, and a `program` lowers too (`compiler/src/run.rs`). Rows P13–P16 are added as designs
>   (§8). This is the one edit to a fact file that this RFC makes now, since it corrects a stated fact and
>   builds nothing.

---

## 0. Method: the failure modes this RFC answers

RFC-0001 §0's rule applies: a decision traces to a named failure mode. The `D-` prefix marks deployment
failure modes. The evidence column says what was checked, and where. Nothing here was measured on any
platform.

| ID      | Failure mode                                                                                                                                                                               | Evidence                                                                                                                                                                                                           |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **D-1** | **The host lock-in.** The only deployment the charter names is Cloudflare, so a backend that must run on Fly, or on a machine the operator owns, needs a rewrite.                          | Owner, 2026-10-20 (verbatim in §1). Not measured.                                                                                                                                                                  |
| **D-2** | **The loopback bind.** A generated service binds `127.0.0.1`, so nothing outside its own network namespace can reach it. A container that binds loopback answers no request from the host. | `compiler/src/lower.rs:894` binds `("127.0.0.1", port)`; the CI serve step reaches it over loopback (`.github/workflows/ci.yml`, the `lowering` job).                                                              |
| **D-3** | **The untested artifact.** No image is built or run in CI, so a fault in packaging appears at deploy time.                                                                                 | `ci.yml` has no image job. The compiler emits no Dockerfile, and no workflow builds a `wasm32`, `aarch64` or musl target. The one musl reference is a downloaded cargo-deny release asset (`supply-chain.yml:62`). |
| **D-4** | **The silent fallback.** A `PORT` that does not parse is replaced by 8080 with no message, so an operator's typo runs on the wrong port.                                                   | `compiler/src/lower.rs:890–893`: `.ok().and_then(parse).unwrap_or(8080)`.                                                                                                                                          |
| **D-5** | **The region in source.** A region or host written into a `.mz` file would make one program hash differently in each deployment, and would tie a service to one platform.                  | A design argument from the content-addressed IR (RFC-0003). Not measured.                                                                                                                                          |
| **D-6** | **The abrupt stop.** The generated server has no shutdown handling, so a platform's stop signal ends in-flight requests without a response.                                                | `lower.rs:888–906`: `axum::serve(listener, app())` with no `with_graceful_shutdown`, and the `tokio` pin enables no `signal` feature (`lower.rs:109–129`).                                                         |

Each design choice below names the row it removes. A choice that removes none of them is not in this RFC.

---

## 1. Motivation

The owner decided on 2026-10-20:

> "we are making a large mistake. We focusing our mzizi build out to only workers. Our build out should also
> contain containers and bare metal."
>
> "This way wherever a backend may be on cloudflare or fly we can connect to both"
>
> "We don't always have to use South Africa as the base please use Europe and Singapore for fly as well."

**What the code does today** (`origin/staging` at `a0bb6b0`):

- `mz build <file.mz> --out <dir>` lowers a `service` to a Cargo package that depends on `axum =0.8.9` and
  `tokio =1.53.1` (`compiler/src/lower.rs:39–45`). That package serves `127.0.0.1:$PORT` and nothing else.
- `mz build` and `mz run` lower a `program` to a dependency-free package, and `mz run` builds it with
  `cargo build --offline` (`compiler/src/run.rs:1038–1046`). The output is a native executable.
- No target other than the host's native one exists. No Dockerfile is emitted, and no `wasm32`, `musl` or
  `aarch64` build appears in the compiler or in CI. Every CI job runs on `ubuntu-latest` (`ci.yml`, one line
  per job).
- The compiler lowers two file kinds. A `service` is a server, and a `program` is an executable that runs to
  completion and exits with a status.

**What the charter says today.** `CHARTER.md` §2 (line 93) describes the edge layer as Workers, and
Containers as "a native Rust HTTP server packaged as a container image, reached from a Worker through the
Container binding". Phase 1 (line 124) says the deliverable is "actually deployed to Cloudflare, as a Worker
or a Container depending on the workload". Bare metal is not named anywhere in the charter. The same wording
is mirrored in the docs and site repositories' charter pages, which the freshness rule keeps in step with
this file.

Two points follow from the code and the charter, not from the owner's direction:

- A container that is "reached from a Worker through the Container binding" is one way to run a container.
  The charter's wording makes the Worker the only caller. A container target that is reachable from anywhere
  needs the bind and shutdown changes in §4.2.
- `workers-rs` targets only the Workers runtime (CHARTER §4, Phase 1 paragraph). A service that runs only as a
  Worker runs in one place. The owner's second point is that a backend can be on either Cloudflare or Fly, and
  the language should not make that choice for it.

**The gate.** Two passages of the charter bear on sequencing, and this RFC does not lift either one. CHARTER §1
(line 82) says nothing is ported to Mzizi until the backend task family passes the kill criterion. CHARTER §4
(line 117) says downstream work waits for Phase 0's number. §10 and Q1 deal with that.

**Routing rationale, not a measurement.** The owner's Paris choice rests on this: many subsea cables from North
and West Africa land in Marseille and reach Paris, so North and West Africa may connect to Europe better than to
South Africa. This is a rationale. Per-country latency is measured in the pilot and reported as it falls, and
this RFC claims no latency figure.

---

## 2. Definitions

A target is the form a build produces, and the place it runs. The same `.mz` source reaches each target through
`mz build --target` (§4.1). The source does not name a target.

| Target         | What `mz build` produces                                                                               | Where it runs                                                                                                                                                                                                                 | Triples                                                                                               | Status today                                                                                  |
| -------------- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| **Workers**    | A Cargo package built with `workers-rs` for wasm32. The Workers runtime calls its `fetch` handler.     | Cloudflare Workers only.                                                                                                                                                                                                      | `wasm32-unknown-unknown` (to be confirmed at implementation, §4.4)                                    | Not built. Designed as a later wave (§4.4).                                                   |
| **Containers** | A native Linux executable in an OCI image, with a `Dockerfile`.                                        | Any OCI runtime that starts the image's entrypoint and gives it a port: Fly Machines, Kubernetes, a Cloudflare Container (reached from a Worker through the Container binding, which is one host of this target), and others. | `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl` (a static binary in a `FROM scratch` image) | Not built. The native binary the image would hold is built today (for the host's glibc only). |
| **Bare metal** | The same native Linux executable, run directly on a host under a service manager or a shell. No image. | A Linux host the operator runs.                                                                                                                                                                                               | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, and the two musl triples above               | Only `x86_64-unknown-linux-gnu` is built, and only in CI on `ubuntu-latest`.                  |

**Containers and bare metal share one binary.** The containers target is the bare-metal binary with an image
around it. The difference is which triple is built and what sets `HOST` and `PORT`.

**Glibc and musl.** A dynamically linked glibc binary needs the host's glibc at least as new as the one it was
built against, so its minimum version is set by the build host. A static musl binary has no such dependency,
which is why the container image uses it. Bare metal can use either. The RFC proposes a static musl build for
bare metal too, and keeps glibc as the default because it is what is built today (§7).

**Programs.** A `program` lowers to a native executable, which is the bare-metal form (§4.1). A `program` in a
container is a one-shot job, which this RFC does not propose (Q2). A `program` does not target Workers: a Worker
has no process exit status and no standard output, so the RFC proposes no Workers form for a `program`.

**Not targets.** Windows, macOS, native mobile (CHARTER §2, Phase 3) and hardware (CHARTER §2, Phase 5) are not
targets in this RFC. Each is named in the charter and gets its own design when it is scoped.

---

## 3. The proposed charter amendment

### 3.1 The version rule

The charter records each owner-directed change in a changelog blockquote at the top of CHARTER.md, headed
`vX.Y changelog — owner-directed DATE`. Each changed passage is marked _(owner-directed DATE)_ where it lands.
The version moves by a minor step for an owner-directed change: v0.3 (owner-directed 2026-09-29) and v0.4
(owner-directed 2026-09-30) each took the next minor number. The next version is **v0.5**.

The version is written in three places, and all of them move in the same change: the heading
`## Mzizi Research Charter, v0.4` (CHARTER.md line 7), the sentence in LANGUAGE-TRACKER.md's introduction
("[CHARTER.md](./CHARTER.md) v0.4"), and the charter's own changelog. The docs and site repositories' freshness
checks read the charter's version, so the same PR's site and docs update is part of the amendment (§10).

### 3.2 The changelog blockquote (proposed)

> **v0.5 changelog — owner-directed 2026-10-20.** Three changes, each marked _(owner-directed 2026-10-20)_ where
> it lands. (1) **Three deployment targets, not one:** Cloudflare Workers, containers on any OCI host, and bare
> metal (§2, §4; [RFC-0016](./design/RFC-0016-three-targets.md)). (2) **Every backend sits behind an interface
> the host provides, with one adapter per platform,** so a service reaches stores on Cloudflare and on Fly alike
> (§2; RFC-0016 §5). (3) **Regions are deployment configuration, not language:** the `.mz` source names no region
> and no host (§2; RFC-0016 §6). Nothing here is built. The Phase 0 gate, the kill criterion and the backend port
> gate are unchanged.

### 3.3 The replacements

Each replacement is the exact text, and the old text it replaces is quoted in the first clause.

**(a) §1, line 77.** Replace the phrase "server and edge through Cloudflare Workers and Containers, ML through
Candle" with:

> server and edge through Cloudflare Workers, containers on any OCI host, and bare metal _(owner-directed
> 2026-10-20)_, ML through Candle

**(b) §2, the Full-stack row (line 92).** Replace "the server/API half lowered per target — `workers-rs` for
Workers, a native Rust HTTP server (axum-class) for Containers." with:

> the server/API half lowered per target — `workers-rs` for Workers, a native Rust HTTP server (axum-class) for
> Containers and for bare metal _(owner-directed 2026-10-20)_.

**(c) §2, the Edge row (line 93).** Replace the whole row, from `| **Edge (Cloudflare Workers and Containers)**`
to its end, with this row:

> | **Deployment targets** _(owner-directed 2026-10-20)_ | **Three targets, one source.** (1) **Workers:** `workers-rs` (wasm32), run by the Cloudflare Workers runtime. (2) **Containers:** a native Rust HTTP server (axum-class) packaged as an OCI image and run on any OCI host: Fly Machines, Kubernetes, a Cloudflare Container reached from a Worker through the Container binding, or another host. (3) **Bare metal:** the same native binary run directly on a Linux host, glibc or static musl, on x86_64 and aarch64. The compiler's `--target` flag chooses the target, not the source. Handlers reach target-specific facilities only through declared capabilities, so one source runs on each target. All three are charter targets; the order they are built in is a plan (RFC-0016 §9), not a charter decision. |

**(d) §4, the Phase 1 paragraph (line 124).** Replace "actually deployed to Cloudflare, as a Worker or a
Container depending on the workload. The **Workers** target lowers to `workers-rs` (wasm32); the **Containers**
target lowers to a native Rust HTTP server (axum-class, e.g. axum) packaged as a container image and reached from
a Worker through the Container binding." with:

> actually deployed, as a Worker or a Container on Cloudflare, or as a container or a bare-metal binary on any
> other host, depending on the workload _(owner-directed 2026-10-20)_. The **Workers** target lowers to
> `workers-rs` (wasm32). The **Containers** target lowers to a native Rust HTTP server (axum-class, e.g. axum)
> packaged as an OCI image, and runs on any OCI host, for example Fly Machines or a Cloudflare Container reached
> from a Worker through the Container binding. The **bare-metal** target is the same native binary run directly on
> a Linux host. Each target's acceptance is one request answered by a deployment of that target.

The rest of the Phase 1 paragraph, including the statement that `workers-rs` targets only the Workers runtime and
the warning about capabilities, stays as it is.

### 3.4 What the amendment does not change

The Phase 0 gate and the kill criterion (§4, §6); the backend port gate (§1, line 82); the non-goals in §5; the
Phase 2 Candle paragraph; Phase 3 (native mobile); Phase 5 (hardware). None of these is touched by §3.3.

---

## 4. Lowering changes

Each subsection is design. The exact text of each change is given so that a reviewer can check it against the
code it would replace.

### 4.1 `mz build --target`

Today the usage line (`compiler/src/harness.rs:841`) is `mz build <file.mz> --out <dir>`. Proposed:

```
mz build <file.mz> --out <dir> [--target native|container|workers]
```

- **`native`** is the default. Its output is the package the compiler writes today, byte for byte. A test (§9,
  W2) compares the output for `examples/registry.mz` with a committed snapshot, so an accidental change to the
  native package fails CI.
- **`container`** writes the native package plus a `Dockerfile`, a `.dockerignore` and a `Cargo.lock` (Q4). It
  applies to a `service` only in this RFC (Q2).
- **`workers`** is refused today with exit status 2, and a message that names RFC-0016 §4.4. It is accepted only
  when §4.4's wave is built.
- An unknown target is a usage error, exit status 2, following `main.rs`'s existing convention.
- Exit statuses are unchanged: 0 for success, 1 when the file has errors, 2 for a usage or I/O problem.
- The usage line is one source for dispatch and for the help text (RFC-0012 §1.2). Changing it means changing
  the entry in `compiler/src/harness.rs` in the same PR.

`mz run` is unchanged. It builds the native form of a `program`, which is the bare-metal form.

### 4.2 Bind address, port and shutdown

Today the generated `main` (`compiler/src/lower.rs:888–906`) reads `PORT`, defaults to 8080, binds
`("127.0.0.1", port)`, prints `listening on http://127.0.0.1:{port}`, and has no signal handling. Proposed, as
generated Rust:

```rust
#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = match std::env::var("PORT") {
        Err(_) => 8080,
        Ok(p) => match p.parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("PORT must be a port number, got {p:?}");
                std::process::exit(1);
            }
        },
    };
    let listener = match tokio::net::TcpListener::bind((host.as_str(), port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on {host}:{port}: {e}");
            std::process::exit(1);
        }
    };
    println!("listening on http://{host}:{port}");
    if let Err(e) = axum::serve(listener, app()).with_graceful_shutdown(shutdown()).await {
        eprintln!("server stopped: {e}");
        std::process::exit(1);
    }
}

/// Resolves on Ctrl-C or on SIGTERM, which is the stop signal a container runtime sends.
async fn shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}
```

Three behaviour changes, each of which the reviewer should see:

- **An unparsable `PORT` exits with status 1** instead of silently running on 8080 (D-4). A `PORT` that is unset
  still defaults to 8080.
- **`HOST` is read.** Its default, `127.0.0.1`, is what the binary does today, so a developer's `cargo run` is
  unchanged. The container image sets `HOST=0.0.0.0` (§4.3, Q6).
- **Graceful shutdown** (D-6). This needs tokio's `signal` feature, which the `cargo_toml` template in
  `lower.rs:109–129` does not enable today. The version pins in `PINS` do not change. The feature list in
  `benchmarks/tasks/b1-routing/reference-rust/Cargo.toml` should match, since CLAUDE.md requires that file to
  stay in step with `PINS`. An IPv6 `HOST` must be printed in brackets (`http://[::]:8080`), which the
  `println!` above does not yet do; the implementation adds that.

The CI serve step (`ci.yml`, the `lowering` job) runs `PORT=18080`, greps `listening`, and sends one request over
`127.0.0.1`. Its assumptions still hold after these changes.

### 4.3 The container output

The `container` target writes this `Dockerfile`, with the package name and the triple filled in. The text here is
the design; the exact base-image tags and digests are chosen at implementation, with the owner (Q3), and are not
pinned by this RFC.

```dockerfile
# Generated by `mz build --target container` from <source>. Do not edit (RFC-0016 §4.3).
FROM rust:<tag> AS build
RUN rustup target add x86_64-unknown-linux-musl \
    && apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=build /src/target/x86_64-unknown-linux-musl/release/mz-<name> /mz-<name>
ENV HOST=0.0.0.0 PORT=8080
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["/mz-<name>"]
```

- **Two stages.** The build stage compiles a static musl binary. The runtime stage is `FROM scratch`, so the image
  holds the binary and nothing else: no shell, no package manager and no C library to patch.
- **A numeric user.** `scratch` has no user database, so `USER 65532:65532` is numeric.
- **Health is a route, not a `HEALTHCHECK`.** The generated service answers `GET /_mz/health` with 200 and the
  body `ok`, ahead of its own routes, on every service target (§4.5). Orchestrators probe it (Fly's HTTP checks,
  Kubernetes probes). A Docker `HEALTHCHECK` needs a program inside the image to run, and `scratch` has none, so
  the Dockerfile emits none.
- **Configuration is environment.** `HOST`, `PORT`, and the capability settings in §5.4. No secret is written into
  the image.
- **`.dockerignore`** excludes `target/`, so a local build does not enter the context.
- **Cargo.lock** is written into the output and copied into the build stage, so `--locked` holds (Q4). The
  repository gitignores `Cargo.lock`, and generated output is not committed, so this does not change the rule
  that the lock is not committed.
- **Network.** The build stage fetches crates from crates.io, which a user has. CI's rule is narrower (§7, Q3).

### 4.4 The Workers target, for services (later)

This is a plan. Nothing in it is built, and no wave is scheduled.

1. **The target-neutral core already exists.** In the generated runtime (`RUNTIME`, `lower.rs:580–907`), the reply
   type and the routing (`Reply`, `dispatch`, `route` and their helpers, `lower.rs:591–856`) use only the standard
   library. A search of that range finds no `axum`, `tokio`, `hyper` or `http` symbol. The axum and tokio glue is
   `into_response`, `handle`, `app` and `main` (`lower.rs:857–906`).
2. **What a Workers lowering replaces.** It keeps the core and replaces only the glue: a `#[event(fetch)]` handler
   calls `dispatch` and converts `Reply` into a `worker::Response`. No tokio server socket is used, because the
   Workers runtime provides the socket.
3. **The build.** A `worker` (workers-rs) crate built for wasm32, as a `cdylib`, with the build step the
   workers-rs tooling uses. The version pin is an open question (Q8).
4. **Constraints the lowering must honour.** No `std::thread`, no listener, no blocking I/O. Outbound I/O goes
   through the Workers runtime's `fetch` and through bindings. A `use` declaration (RFC-0001 §1.7: `use` means
   "needs a capability") maps to a binding on this target and to an adapter on the others (§5.4). The syntax for
   capabilities is not designed here (P7, P8).
5. **Tests.** The per-example tests (`lower.rs:520–550`, one `#[tokio::test]` per clause) keep running natively
   against `dispatch`, which needs no runtime. The wasm build is checked first by compilation (§7), and by one
   request under the Workers runtime later, which needs network access that CI does not have today.

### 4.5 Diagnostics and the harness

Every new code and every new feature goes into `compiler/src/harness.rs` in the same PR, with one `trigger` per
code and tested examples (RFC-0012 §1.2; `cargo test --test harness` is the check).

- **`MZ0813`** (proposed): a `service` declares a route whose path begins with `/_mz/`, the prefix reserved for
  routes the runtime owns. Severity: error. Fix kinds: none, because the author chooses the path. Trigger: a route
  declared at `/_mz/health`. The example set has one service that fails and one that passes. `MZ0813` is in the
  service family, which RFC-0011 numbers `MZ08xx`. It is the first free code there. (`MZ0993` is designed as
  "runtime failure" in RFC-0013 and RFC-0014, so it is not used here.)
- **The `GET /_mz/health` route** (§4.3) is emitted on every service target, native included, so the generated
  shape does not depend on the target. A service that declares a route under the reserved prefix is refused by
  `MZ0813` before it is lowered.
- **`--target`** adds no code. A bad target is a usage error, exit status 2 (§4.1).

---

## 5. Connectivity: one interface, one adapter per platform

### 5.1 The principle

Every backend a service uses sits behind an interface the host provides. Each platform supplies an adapter for
that interface. The service's code depends on the interface and never on the platform. A backend can then be on
Cloudflare or on Fly, and a service reaches it through whichever adapter its target provides.

### 5.2 What exists (context only)

The component registry (`mzizi-dev/mzizi-registry`) is a separate repository. The language does not depend on it
(AGENTS.md, "Repo boundaries"), and this section cites it only as context. Its state was read on 2026-10-20 from
the `claude/mzizi-data-writes` branch (commit `17826658`), not from its `main` (`f70703da`), where the crates are
not present.

- `mzizi-data` defines an `Io` trait with `write_all` and `read_exact` (`mzizi-rs/crates/mzizi-data/src/io.rs:13–17`).
- Its one transport is `SocketIo`, which wraps `worker::Socket` (`mzizi-rs/crates/mzizi-data/src/workers/socket.rs`).
  `mzizi-mongodb` has a Workers transport of the same kind (`mzizi-mongodb/src/workers/mongo.rs`).
- A native adapter, a `tokio::net::TcpStream` with rustls for TLS, would implement the same trait. It is planned,
  not built. Nothing in this RFC depends on the registry's schedule.

### 5.3 The directions

| From → to                             | Route                                                                                                                                                       | Status                                      |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Worker → a store on Fly               | A private Cloudflare Tunnel to the Fly service, from the Worker's side. A store with an HTTP API can also be called directly.                               | Designed in the fact sheet; not built. Q11. |
| Worker → a store on Fly, via bindings | A Workers VPC binding, if `workers-rs` supports one.                                                                                                        | Open. Q8.                                   |
| Container or bare metal → D1, KV, R2  | The platform's HTTP APIs. A container has no Workers bindings.                                                                                              | Designed; not built.                        |
| Container → container on Fly          | Fly's private network (6PN). Whether a service bound to `0.0.0.0` is reachable there, or must also bind IPv6, is to be checked against Fly's documentation. | Open. Q10.                                  |
| Bare metal → either                   | HTTP APIs, and TCP where the operator configures it.                                                                                                        | Designed; not built.                        |

### 5.4 What the language must expose so services stay target-neutral

The language's own designs carry the requirement. This RFC does not design them.

- **Capabilities name interfaces, never hosts.** A service declares what it uses (`use`, RFC-0001 §1.7). Its
  settings are named for the capability (for example `store`), and each target resolves the name: an environment
  variable on the container and bare-metal targets, a binding on Workers. A URL or a host in `.mz` is a design
  error, because it breaks §6 and D-5.
- **One interface per store kind, with an adapter per target.** That is P8's requirement (state and I/O, not
  designed). The standard library's HTTP client (P2, RFC-0014 §2) is the same kind of thing.
- **Every adapter failure is a value the service handles.** An outbound call returns a result, and the failure is
  not dropped (T2-5, RFC-0014 §0; RFC-0013 CL-3). No adapter panics across the boundary.
- **The adapters are Rust the toolchain ships for each target**, in the way `compiler/src/run.rs` ships its
  runtime. The first slice therefore does not depend on P6 (Rust interop), which is not built.

---

## 6. Regions: what the language does not do

### 6.1 The language does not decide regions

The language has no `region`, placement or replication syntax, and `.mz` names no region and no host. A region is
an operator's choice for one deployment. This keeps one source and one content hash valid across every
deployment (D-5, RFC-0003). The IR and the harness carry no region either.

### 6.2 Regions are deployment configuration

The operator sets regions in the platform's configuration: `primary_region` in `fly.toml`, and the set of regions
with the Fly CLI. The RFC states the decision and does not encode it.

### 6.3 The regions the owner has named (2026-10-20)

- `jnb` (Johannesburg) is the primary region for stateful data today.
- `cdg` (Paris) is the recommended Europe region for Fly, with `mad` (Madrid) as the second choice.
- `sin` (Singapore) is the Asia-Pacific region.
- Stateless services may run in all three. Stateful services (the Cassandra, Postgres ledger, Redpanda, CouchDB and
  Doris services in `data-infra`) stay primary in `jnb` until a replication design exists. The data-infra repository
  owns that design.
- The routing rationale in §1 is a reason, not a measurement.

### 6.4 Documentation example

This `fly.toml` is documentation only. It is not checked in and CI does not run it. Field names are to be checked
against Fly's current `fly.toml` reference before anyone uses it.

```toml
# Documentation example for a service built with `mz build --target container`.
app = "example-service"
primary_region = "jnb"

[env]
  HOST = "0.0.0.0"
  PORT = "8080"

[http_service]
  internal_port = 8080
  force_https = true
  min_machines_running = 1

  [[http_service.checks]]
    method = "GET"
    path = "/_mz/health"
    interval = "15s"
    timeout = "2s"
    grace_period = "5s"
```

The Europe and Singapore regions are added with the Fly CLI, for example `fly regions set jnb cdg sin`, and each
region's machine count is then set with `fly scale count`. Confirm both commands with `fly help` before use.

### 6.5 What follows

A stateless instance in `cdg` or `sin` calls a store in `jnb` across the network for each request that reads or
writes state. This RFC does not claim how long that takes. The cost is a consequence of the placement, and it is
measured where it matters. Read replicas near the other regions are a data-infra design, and until one exists,
the stateful services stay in `jnb`.

---

## 7. CI plan

Nothing in this section is added by this PR. It is the plan for the waves in §9.

| Job (new unless marked)                                                                                     | Runner               | What it proves                                                                                                                                                                                               | Network                                   |
| ----------------------------------------------------------------------------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------- |
| `compiler`, `lowering` (its `mz run` step included), `benchmarks`, `secret scan`, `supply chain` (existing) | `ubuntu-latest`      | Unchanged. The native package keeps its snapshot test (§4.1).                                                                                                                                                | As today.                                 |
| `musl` (W4)                                                                                                 | `ubuntu-latest`      | The registry example builds as a static `x86_64-unknown-linux-musl` binary (`file` reports static linking), serves on loopback, and answers the same `OPTIONS` request the `lowering` job sends.             | The rustup target (Q3).                   |
| `aarch64` (W4)                                                                                              | an arm64 runner (Q7) | The same build and request on `aarch64`, natively. No emulation.                                                                                                                                             | The rustup target (Q3).                   |
| `oci-image` (W3)                                                                                            | `ubuntu-latest`      | `mz build examples/registry.mz --out X --target container`, then `docker build`, then `docker run` with `HOST=0.0.0.0` and a published port, one request, `GET /_mz/health` returns 200, then `docker stop`. | None if the base image is `scratch` (Q3). |
| `wasm32-check` (W5)                                                                                         | `ubuntu-latest`      | `cargo check --target wasm32-unknown-unknown` over the lowered Workers package. Compilation only.                                                                                                            | The rustup target (Q3).                   |

**The rule this must keep.** AGENTS.md ("Build, test, run") says the only network access a build needs is to
crates.io, plus two tools from GitHub releases. Three things here go beyond that:

- A rustup target is downloaded from the toolchain distribution server. The toolchain itself already comes from
  that server, so a target adds to a channel CI already uses.
- A `docker build` that starts `FROM` a public base image pulls that image from a registry. This is outside the
  rule, and the RFC avoids it: the image's base is `scratch`, and the binary is built on the runner (Q3, option A).
- An arm64 runner, or an emulated one through QEMU, which needs a third-party action (Q7).

**Docker.** The hosted `ubuntu-latest` image includes a Docker daemon. The `oci-image` job uses it with no service
container and no registry login. The job builds an image into the local daemon, runs it, and discards it. It never
runs `docker push`. Publishing an image is a later owner decision and not part of any wave here.

**Timeouts.** Each new job sets `timeout-minutes`. The existing `lowering` job uses 15.

---

## 8. The tracker

### 8.1 P11, corrected in this PR

P11 is the deployment row. Its status stays 🟡. The evidence is corrected to the code on `main`, and the
"done when" is split by target:

> | P11 | Targets: Cloudflare Workers, containers, bare metal | 🟡 | Native today: `mz build` lowers a `service` to a local axum package that binds `127.0.0.1` (#32), and a `program` to a dependency-free Rust package that `mz build` and `mz run` build with Cargo (`compiler/src/run.rs`). Only the x86_64 glibc build is made, in CI on `ubuntu-latest`. No container image, no musl or aarch64 build and no WebAssembly or Workers target exist. Designed in [RFC-0016], a draft. | Each target builds and answers one request on its own platform: bare metal (P14), containers (P13) and Workers (P15). A `program` runs natively on each triple it is built for. ([CHARTER.md] §2) |

### 8.2 New rows

Each row is 📝 (designed in an RFC, not implemented). None changes what the language can do today.

| ID  | Capability                                                                  | Status | Evidence                                                            | Done when                                                                                                     |
| --- | --------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| P13 | Container target: a `service` as an OCI image                               | 📝     | [RFC-0016] §4.1–§4.3 and §4.5 (`MZ0813`, `/_mz/health`)             | The `oci-image` job passes on `main` (§7)                                                                     |
| P14 | Bare-metal target: static musl and glibc, x86_64 and aarch64                | 📝     | [RFC-0016] §2 (triples), §7 (`musl`, `aarch64` jobs)                | Both triples build and serve one request in CI on their own runner                                            |
| P15 | Workers target: a `service` as a wasm32 Worker                              | 📝     | [RFC-0016] §4.4 (a later wave)                                      | `wasm32-check` passes, and one request is answered under the Workers runtime                                  |
| P16 | Connectivity: each backend behind an interface, with one adapter per target | 📝     | [RFC-0016] §5 (the principle; the registry's `Io` trait as context) | A service reaches one store on Cloudflare and one on Fly through the same interface. Depends on P2, P7 and P8 |

**The overlap with #121.** PR #121 (the docs pass) also edits the P11 row, the README, the CHANGELOG and
`design/ROADMAP.md`. On a rebase, the P11 row should keep this PR's evidence and the name #121 gives it. The name
in §8.1 is #121's, so the two rows should differ only in the evidence and "done when" text.

---

## 9. Build order: a plan, not a charter decision

Each wave is a separate PR, with its own tests and its own CHANGELOG entry. None starts before the owner decides Q1. Q1 is decided (§11): design only, so no wave after W1 has started.

| Wave | What it builds                                                                                                                                                                                  | Its test                                                                                                                                     |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| W1   | This RFC, the charter text (§3) for the owner, the tracker rows (§8), the README and ROADMAP pointers.                                                                                          | Prettier and markdownlint pass. `cargo test --test harness` is unchanged.                                                                    |
| W2   | The target-neutral changes: `HOST`, strict `PORT`, graceful shutdown (§4.2), `GET /_mz/health` and the reserved prefix with `MZ0813` (§4.5), and the `--target` flag with `native` only (§4.1). | A snapshot of the native package. A test that `PORT=abc` exits 1. A test that `SIGTERM` ends the server cleanly. The `MZ0813` harness entry. |
| W3   | The container target: the `Dockerfile`, `.dockerignore` and `Cargo.lock` output (§4.3), and the `oci-image` job (x86_64 only).                                                                  | The `oci-image` job, with its request and health check.                                                                                      |
| W4   | Bare-metal triples: the static musl build and the aarch64 build (§2), and the `musl` and `aarch64` jobs (§7).                                                                                   | The two jobs, each serving one request, and `file` reporting static linking for musl.                                                        |
| W5   | The Workers target for services (§4.4): the lowering, and the `wasm32-check` job.                                                                                                               | `cargo check` for wasm32. One request under the Workers runtime is a later test that needs network access CI does not have.                  |
| W6   | Connectivity adapters (§5): the native transport and the Cloudflare HTTP adapters, in the repositories that own them.                                                                           | The adapter's own tests, in its repository. Nothing in this repository depends on those repositories (AGENTS.md).                            |

---

## 10. Non-goals

- **No region, placement or replication syntax** in `.mz`, the IR or the harness (§6.1).
- **No generated deployment configuration in this RFC.** The compiler emits the binary and the image. A `fly.toml`,
  a Kubernetes manifest or a `wrangler` configuration is the operator's, except that a Workers wave may emit its
  Wrangler configuration (Q8).
- **No registry push and no release image.** The compiler stays dependency-free and writes files only. CI builds
  images into the local daemon and discards them.
- **No Windows or macOS target, no native mobile, no hardware** (CHARTER §2; §4 Phases 3 and 5).
- **No speed, cost or latency claim per target.** The performance suite (`benchmarks/perf/`) times native binaries on
  one machine, and CI gates only that their output agrees. It is not a deployment benchmark, and no result from it
  is attributed to a target.
- **No change to the Phase 0 gate, the kill criterion or the backend port gate** (CHARTER §1, §4, §6). This RFC is
  design work, and §10's gate question is Q1.
- **No concurrency or state design** (P8, P9).
- **No other repository is changed by this RFC.** §5.2 cites the component registry only as context.

---

## 11. Open questions

Each question has a recommendation. The owner decides.

**Q1. Sequencing against the Phase 0 gate.** CHARTER §4 (line 117) says Phase 1 is not built before Phase 0 has a
number, and CHARTER §1 (line 82) gates the backend port on the kill criterion. The container and bare-metal
targets are Phase 1 work. _Recommendation:_ the owner approves §3 and this design now. W2, which changes only the
native package and adds no target, proceeds on the owner's word. W3 to W6 wait for the gate, or for an explicit
owner exception recorded in the charter's changelog.
_Decided (owner, 2026-10-21):_ "approve RFC-0016 design only". The design in this RFC is approved as design. No
exception to the Phase 0 gate is granted, so no build wave (W2 to W6) starts on this decision; each waits for the
gate or for a later owner decision that names it. CHARTER.md is not edited by this decision, and §3 stays a
proposal.
_Confirmed the same day (owner, 2026-10-21):_ "wait for phase 0 gate and with the build waves". The charter text (§3)
and every build wave wait for the Phase 0 gate together. The owner's scoping direction for the container target is
part of the design now: "any container type docker is the default, but standard linux containers etc as well". So
the image is a standard OCI image, Docker is the default tool that builds it, and W3's test, when it runs, also
builds and runs it with a second OCI tool (Podman).

**Q2. Programs on the container target.** A `program` in a container would be a one-shot job, and its exit status
would follow RFC-0013 §13. _Recommendation:_ not in this RFC. A `program` is native, and its bare-metal form is its
binary. A container job can be added later with its own tests.

**Q3. Network in CI and base images.** AGENTS.md limits a build's network access to crates.io and two GitHub-release
tools, and §7 lists three items beyond that. _Recommendation, option A:_ the `oci-image` job builds the binary on the
runner and uses `FROM scratch`, so no base image is pulled. The rustup target download goes through the same channel
that installs the toolchain. The owner amends AGENTS.md to name that channel. Option B, pinned public base images, is
not recommended for CI, and it is the only option that needs a registry pull.

**Q4. Lock file in the container output.** The repository gitignores `Cargo.lock`, and generated output is not
committed. Without a lock, an image built on two days can contain two sets of crates. _Recommendation:_ the container
output writes `Cargo.lock`, and the Dockerfile builds with `--locked`. The lock is generated output, so the rule
against committing it holds.

**Q5. The health route and its reserved prefix.** _Recommendation:_ `GET /_mz/health` on every service target, the
`/_mz/` prefix reserved, and `MZ0813` for a route that uses it (§4.5). A service that already uses `/_mz/` has to
change its path.

**Q6. Where the container's bind address is set.** _Recommendation:_ the Dockerfile sets `HOST=0.0.0.0`, so the binary's
default stays loopback for every other target, and a bare-metal operator who forgets `HOST` gets a loopback bind, not an
exposed one.

**Q7. The aarch64 runner.** The `lowering` job runs on `ubuntu-latest`, which is x86_64. _Recommendation:_ a hosted arm64
Linux runner, if the organisation has one. The alternative is QEMU through a third-party action, which AGENTS.md would
have to allow, and which runs a native build under emulation, so a slow job is the likely result. The owner confirms the
runner label.

**Q8. Workers details.** Four open items. (a) The `worker` crate's version pin, and the wasm32 triple name, to be confirmed
at W5. (b) Whether the examples run natively against `dispatch` only (§4.4, point 5), or also under a Workers emulator.
(c) The bundle-size budget, which the registry's `mzizi-data` leaves open as its decision 6.2 (fact sheet §6). (d)
Whether a Workers VPC binding can reach a store on Fly, and whether `workers-rs` supports it, which the fact sheet also
leaves open. _Recommendation:_ decide (c) and (d) before W5 starts.

**Q9. The Cloudflare Container binding.** The charter's words "reached from a Worker through the Container binding"
(CHARTER §2) are carried over, and this RFC does not verify them against Cloudflare's current documentation.
_Recommendation:_ verify the binding's current shape before the wave that relies on it.

**Q10. Fly's private network.** Whether a service bound to `0.0.0.0` is reachable from other Fly apps over 6PN, or must
also bind IPv6 `::`. _Recommendation:_ check Fly's documentation before W3 deploys anything, and bind both if it requires
it, through `HOST`.

**Q11. Fly to Cloudflare tunnels.** Whether a Worker reaches a Fly store through one Cloudflare Tunnel per service or per
region. _Recommendation:_ a per-deployment decision in the deployment's own configuration. This RFC does not fix it. The
design lives with the deployment, not in the language.

**Q12. Which store interfaces come first.** P2 is a closed list (RFC-0014 Q4), and P8 is not designed. _Recommendation:_ the
owner settles RFC-0014 Q4 first. The first store interface is then the one a deployed service needs, and it is designed in
the RFC for P8.

**Q13. Graceful shutdown and the `signal` feature.** Enabling tokio's `signal` feature changes the generated `Cargo.toml`
but no pin. _Recommendation:_ enable it, and make the same change in `benchmarks/tasks/b1-routing/reference-rust/Cargo.toml`
so the reference and the lowering keep the same runtime features.

---

_This RFC changes no language capability. Its claims about deployment are design. Where it names a platform's behaviour
(Fly's private network, Cloudflare's container binding, a Workers binding), it says the behaviour is open, and Q8 to Q11
list what to check._
