//! Dioxus arm sandbox. `check.sh` writes the candidate to `src/component.rs`; this file
//! only mounts it, the same way `mzizi-ui`'s `lib.rs` mounts each registry component as
//! its own module. Do not add anything else here: every candidate must see the same crate.

pub mod component;
