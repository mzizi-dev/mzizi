//! Leptos arm sandbox. `check.sh` writes the candidate to `src/component.rs`; this file
//! only mounts it, the same way the Dioxus arm's `lib.rs` mounts its candidate. Do not add
//! anything else here: every candidate must see the same crate.

pub mod component;
