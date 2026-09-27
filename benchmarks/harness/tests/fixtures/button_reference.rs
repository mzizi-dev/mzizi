// Mirrored from mzizi-dev/mzizi-registry's
// `components/registry/n2-primitives/button.rs` — the hand-written Rust port
// `button.mz`'s own header comment names as its corpus reference. This session
// has no access to the `mzizi-registry` repository (out of scope for this
// task; see RFC-0006 §10.1's harness design), so the commit this was copied
// from could not be named — only the file path above, per the same section's
// instruction to cite the path when the commit is unavailable. What is
// reproduced below is the `ButtonSize::classes()` match arms: the size→class
// mapping is the one fact this fixture needs to be real, and it is copied
// verbatim from the referenced file's Tailwind class strings.
//
// Reduced to only what the harness reads: the size enum and its `classes()`
// match. `ButtonVariant` (colour, not size) carries no `h-N`/`size-N` token
// and is out of scope for this metric (RFC-0006 §10.1), so it is omitted.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Default,
    Sm,
    Lg,
    Icon,
    IconSm,
}

impl ButtonSize {
    pub fn classes(&self) -> &'static str {
        match self {
            ButtonSize::Default => "h-14 gap-2 px-5",
            ButtonSize::Sm => "h-12 gap-1.5 px-4",
            ButtonSize::Lg => "h-14 gap-2 px-6",
            ButtonSize::Icon => "size-14",
            ButtonSize::IconSm => "size-12",
        }
    }
}
