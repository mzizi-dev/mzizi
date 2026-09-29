# Harness test fixtures

## `button_reference.rs`

A byte-identical copy of a file from another repository. Nothing has been added to it,
including this provenance, which is why the provenance lives here instead.

|                   |                                                                                                         |
| ----------------- | ------------------------------------------------------------------------------------------------------- |
| Source repository | `mzizi-dev/mzizi-registry`                                                                              |
| Path              | `components/registry/n2-primitives/button.rs`                                                           |
| Commit            | `3afeb752253a86b5488867078c2238d2cf62b4f0`                                                              |
| Licence           | Apache-2.0 (the registry's `LICENSE`; its `NOTICE` reads "Mzizi / Copyright 2026 The Bundu Foundation") |

Verified identical with:

```
cmp /workspace/mzizi-dev/mzizi-registry/components/registry/n2-primitives/button.rs \
    benchmarks/harness/tests/fixtures/button_reference.rs
```

which printed nothing and exited 0. If the registry file changes, re-copy it, re-run `cmp`,
and update the commit above. Do not edit this copy by hand.

## `button_broken.mz`

Written for this harness, not copied from anywhere. It is `primitives/button.mz` with `sm`
made 44px (class `h-11`, declared height 44) and its own contract weakened to match, so it
passes `mz check` and `mz contract` while disagreeing with the reference's `h-12` (48px). Its
header comment describes the change; `button.mz`'s own explanatory `##` comments are also not
carried over. Before 2026-09-29 it kept `h-12` beside `height 44`, a shape `mz check` now
rejects (MZ0313, FM-11).
