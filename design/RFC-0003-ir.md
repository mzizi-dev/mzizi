# RFC-0003 — The content-addressed IR and the agent's read/write surface

**Status:** draft; core implemented in this PR (hashing, store, outline)
**Author:** the machine author (Claude)
**Scope:** the IR node model, canonical serialization, the content-addressed store,
structural sharing, `mz outline`, and the query/patch surface an agent drives.
Implements RFC-0002 §2.1. Contract evaluation and lowering are later RFCs.

> **Amended by RFC-0006.** §8.4's open question — contract evaluation semantics — is
> answered there, and answering it moved two things here. Contracts now lower into the
> store and **participate in the hash** (RFC-0006 §6), and §7's structural-sharing figure
> is corrected accordingly: see §7.2, which records what the old number was actually
> counting.

---

## 1. Designed against the barriers, not against a wish list

RFC-0001 designed the syntax against nine named failure modes in _writing_ code. This RFC
does the same for **reading a codebase and retaining what was read** — the other half of
the loop, and the half that decides whether a small model can work in a large project at
all. Eight barriers, from the machine author's own experience:

| ID       | Barrier (what actually blocks compression and recall)                                                                                                                                                 |
| -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **RB-1** | **Textual addressing only.** Editing requires an exact string match, so a change needs _more_ context than the change itself — enough surrounding text to make the anchor unique.                     |
| **RB-2** | **File-granular reads.** Learning one prop's type costs the whole file: imports, comments, unrelated functions. No "interface only" primitive exists in the loop.                                     |
| **RB-3** | **Invisible dependency graph.** "What breaks if I change this?" means grepping, which yields false positives (same name, different symbol) and false negatives (re-exports, aliases, dynamic access). |
| **RB-4** | **Diffs conflate moved with changed.** Rename, reformat, and real semantic change all arrive as the same noise, so meaning already derived must be re-derived.                                        |
| **RB-5** | **Generated code is a blind spot.** With macros and build-time transforms, the thing that runs is not the thing that was read.                                                                        |
| **RB-6** | **No stable identity across time.** A summary silently rots: lines move, names change, and nothing reports which recorded facts are still true.                                                       |
| **RB-7** | **Repeated structure is unavoidable cost.** 371 components with identical prop ceremony: paid for on every read, and useless as an edit anchor because it is not unique.                              |
| **RB-8** | **Understanding cannot be cached.** Every session starts cold, and the hand-written substitute (a `CLAUDE.md`) drifts — this project has repeatedly found stale doctrine in its own.                  |

Content addressing answers seven of the eight from a single decision. That is the whole
argument for it.

| Barrier | What the IR does about it                                                                                                        |
| ------- | -------------------------------------------------------------------------------------------------------------------------------- |
| RB-1    | Nodes are addressed by hash, or by a unique structural path (`connectivity_bar/fn:retry`) — never by line number or string match |
| RB-2    | `mz outline` serves the interface at a fraction of source cost; any single node can be fetched alone                             |
| RB-3    | Reference edges are explicit in the node, so "who points at this hash" is a lookup, not a search                                 |
| RB-4    | A moved node keeps its hash. Only real change alters a hash, so a diff _is_ the semantic diff                                    |
| RB-5    | No macros: the IR is the ground truth and is directly inspectable                                                                |
| RB-6    | The hash **is** the identity, permanently. A recorded fact can be re-verified by asking whether that hash still exists           |
| RB-7    | Structural sharing: identical subtrees are one stored node. The 371 identical ceremonies are stored once                         |
| RB-8    | The store is a machine-verified, always-current artifact — the thing a hand-written `CLAUDE.md` was failing to be                |

RB-5 is the exception only in the sense that the syntax already solved it (RFC-0001 §1.5,
native view grammar): the IR inherits rather than fixes it.

## 2. The node model

```text
Node {
  kind:     &str            // "component" | "prop" | "enum" | "variant" | "element" | ...
  label:    Option<String>  // the author-visible name, when the kind has one
  fields:   [(String, String)]   // leaf data, sorted by key
  children: [Hash]          // ordered; order is semantic
}
```

A node's hash is over its **canonical serialization**, which includes its children's
hashes. That makes the store a Merkle DAG: change a leaf and every ancestor's hash changes,
while every untouched sibling keeps its own.

### 2.1 Canonical serialization

Hashing is only stable if serialization is exact, so the form is fully specified and
length-prefixed rather than delimiter-separated — a delimiter can appear inside a value and
silently merge two different nodes into one hash:

```text
kind_len ":" kind
label_len ":" label            (or "-" when absent)
field_count ":"  for each, in key order:  key_len ":" key value_len ":" value
child_count ":"  for each, in order:      hash_hex
```

Fields are sorted by key so that authoring order cannot change identity; children are
**not** sorted, because their order is meaning (a view's children are a sequence).

### 2.2 Hash function

SHA-256, hand-rolled, verified against the published NIST vectors.

The house convention is to hand-roll small, fully-specified things rather than take a
dependency (JSON in N8/N11, and see `primitives/README.md` on why a dependency tree is a
real cost for the target population). SHA-256 qualifies: it is completely specified and has
official test vectors, so correctness is _verifiable_ rather than assumed.

**Stated plainly:** this is a content-identity function, not a security boundary. Collision
resistance matters; side channels do not. If a hash ever becomes a trust boundary — signed
releases, a shared public store — swap in a reviewed crate at that point. That is a
deliberate, recorded limit, not an oversight.

Hashes are displayed as the first 12 hex characters, which is unambiguous at any realistic
project size and cheap to put in a diagnostic or a path.

## 3. Names are metadata, which is what makes renames free

The store maps `Hash -> Node`. A separate namespace maps `name -> Hash`. Nothing inside a
node records the name of anything it references — only hashes.

Consequences:

- **A rename touches one namespace entry.** No call site changes, because no call site ever
  held the name. This removes a whole class of exhaustive multi-site refactor, which is
  precisely the class small models perform worst at.
- **Source carries no import lines.** Already true at the syntax level
  (`primitives/README.md`), and this is the mechanism underneath it: source says _what_, the
  namespace says _which_.
- **Two components can be compared for identity, not just similarity.** Same hash means
  genuinely the same logic, wherever it came from.

## 4. `mz outline` — the representation for dependencies

When editing component A that uses B, an agent needs B's _interface_, not B's body. The
outline is that, and it is itself valid Mzizi — a component with everything except bodies:

```mz
component button
  prop variant: button_variant = default
  prop size: button_size = default
  prop label: text
  prop on_tap: event(none)
  enum button_variant  default outline secondary ghost destructive link
  enum button_size  default sm lg icon icon_sm
  contract 5
end component button
```

Being valid Mzizi is deliberate: an agent already knows how to read it, so the outline
needs no second format and no second parser. The measured cost for the primitive set is
reported in §7.

## 5. The query and patch surface

Implemented in this PR:

- `mz hash <file.mz>` — the root hash, and the node count after sharing.
- `mz outline <file.mz>` — the interface form above.
- `mz ir <file.mz>` — the flat node listing with hashes, for inspection and for tests.

Designed here, next in implementation order:

- `mz refs <hash>` — every node that references this one (RB-3).
- `mz path <hash>` — the structural path, e.g. `connectivity_bar/fn:retry`.
- `mz patch <path> <source>` — replace the node at a structural path; returns the new root
  hash. This is the write half of RB-1, and it is why paths must be unique and readable.
- `mz diff <hash-a> <hash-b>` — node-level added / removed / changed, with moves reported
  as moves because a moved node's hash is unchanged (RB-4).

## 6. What this unlocks that was previously blocked

- **Contract evaluation.** Contract bodies currently parse but do nothing. With an IR they
  can be evaluated against the tree — which is what finally makes the Phase 0 defect metric
  (compiles clean, behaviourally wrong) measurable by the toolchain rather than by a Rust
  test standing in for it. **Done**, in [RFC-0006](./RFC-0006-contracts.md) and
  `mz contract`; the half that still needs a harness is comparison against a *reference*
  implementation (RFC-0006 §10.1).
- **Incremental compilation keyed on hashes.** Only changed hashes and their ancestors
  recompile. This is the cheapest available route to the sub-second check loop the charter
  names as a Phase 0 success metric.
- **A cache that cannot lie.** The store is derived, not authored, so it cannot drift from
  the code the way prose documentation does.

## 7. Measured, in this PR

Numbers are from `tests/ir_measured.rs`, on the nine primitives plus the corpus example,
so they are reproducible rather than asserted:

| Claim                    | Measured                                                                                                   |
| ------------------------ | ---------------------------------------------------------------------------------------------------------- |
| Structural sharing       | **169 shared nodes vs 174 isolated** — 5 saved across 10 files (was 127 vs 141; see §7.2)                  |
| Outline cost             | **worst case 38% of source** (`spinner.mz`); the test fails above 75%                                      |
| Parse + lower, whole set | **~7 ms** for 10 files; budget 400 ms                                                                      |
| Identity stability       | same source → same root hash; blank-line changes do not alter it; a variant column change reaches the root |
| Rename cost              | renaming every component in the store changes **zero** nodes                                               |

The sharing number is honest but small, and worth reading correctly: ten files is a tiny
corpus, and sharing pays off in proportion to repetition. The saving here comes from
genuinely identical subtrees — props of the same name and type. At the scale of the real
571-component registry, where the same prop ceremony recurs hundreds of times, the ratio
should improve substantially — but that is a prediction, and it stays labelled as one until
the registry is lowered.

### 7.1 A correction the implementation forced

The first draft of this RFC called structural paths "stable across edits". That was wrong,
and the measured suite caught it: two `row` siblings inside `alert`'s view collided on the
same path, which would have made a patch address ambiguous.

Two things came out of it. Paths now disambiguate same-named siblings — by `slot` where one
exists (`alert/view/element:notice/element:row@alert-title`, readable, and `slot` is
already the design system's identity attribute), by ordinal otherwise. And the claim is
narrowed to what is true: **paths are unique and readable and survive reformatting; the
permanently stable identity is the hash.** Inserting a sibling can shift an
ordinal-disambiguated path, so anything holding a reference across edits should hold the
hash, not the path.

### 7.2 A correction the *next* implementation forced

The 14-saved figure this table used to report was mostly measuring a placeholder. Contract
bodies were not lowered, so every component contributed the identical empty `contract`
marker node — ten copies collapsing into one. **Nine of the fourteen "saved" nodes were
that.** Genuine sharing of authored content was five, and after RFC-0006 lowered real
contract clauses into the store it still is.

The lesson is narrower than "the number was wrong" and worth keeping: a compression figure
measured over a corpus that contains a stub is measuring the stub. Any future sharing claim
should say which node kinds it is counting.

## 8. Open questions for RFC-0004

1. **Local state.** Still open from RFC-0001, and now with an added constraint: signal
   semantics must survive content addressing.
2. **The patch API's conflict model.** Two agents patching sibling nodes should compose;
   patching the same node must not silently pick a winner.
3. **Store persistence.** In-memory today. On-disk format, and whether the store is shared
   between projects, is a real decision with a caching payoff and a trust question attached.
4. ~~**Contract evaluation semantics** — the subject language for assertions like
   `every button_size height at_least 48`.~~ — _Answered by
   [RFC-0006](./RFC-0006-contracts.md), which also settles §6's "contract bodies currently
   parse but do nothing" and decides that contracts participate in the hash._
