//! NYUCHI CHANGELOG RENDERER — N10: Documentation Outlier, Dioxus.
//!
//! Renders the database changelog as a visual timeline. Reads from
//! `public.changelog` (`nodes_affected`, `components_added`, etc.). The
//! `nodes_affected` field mirrors that column name exactly.

use dioxus::prelude::*;

/// Visual accent for a node badge, keyed by the node's axis in the Mzizi
/// ecosystem grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeAccent {
    /// Horizontal-axis node — also the fallback for an unrecognised node.
    #[default]
    Horizontal,
    /// Vertical-axis node.
    Vertical,
    /// Depth-axis node.
    Depth,
    /// Outlier node, off the horizontal/vertical/depth grid.
    Outlier,
}

impl NodeAccent {
    /// The Tailwind classes for this accent.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Horizontal => "bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]",
            Self::Vertical => "bg-[var(--color-tanzanite)]/10 text-[var(--color-tanzanite)]",
            Self::Depth => "bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]",
            Self::Outlier => "bg-[var(--color-gold)]/10 text-[var(--color-gold)]",
        }
    }

    /// The slug used in a node badge's title text for this accent's axis.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Depth => "depth",
            Self::Outlier => "outlier",
        }
    }
}

/// The human-readable label for an ecosystem node number, or `"Unknown"`
/// when the node number is not part of the ten-node grid.
pub const fn node_label(n: u32) -> &'static str {
    match n {
        1 => "Tokens",
        2 => "Primitives",
        3 => "Brand",
        4 => "Safety",
        5 => "Resilience",
        6 => "Pages",
        7 => "Shell",
        8 => "Assurance",
        9 => "Fundi",
        10 => "Documentation",
        _ => "Unknown",
    }
}

/// The badge accent for an ecosystem node number, defaulting to
/// [`NodeAccent::Horizontal`] when the node number is not part of the
/// ten-node grid.
pub const fn node_accent(n: u32) -> NodeAccent {
    match n {
        1 | 4 | 5 => NodeAccent::Vertical,
        2 | 3 | 6 | 7 => NodeAccent::Horizontal,
        8 => NodeAccent::Depth,
        9 | 10 => NodeAccent::Outlier,
        _ => NodeAccent::Horizontal,
    }
}

/// The axis name shown in a node badge's title text, or `"unknown"` when
/// the node number is not part of the ten-node grid. Kept separate from
/// [`node_accent`]'s fallback, which is `Horizontal` rather than absent.
pub const fn node_axis_text(n: u32) -> &'static str {
    match n {
        1 | 4 | 5 => "vertical",
        2 | 3 | 6 | 7 => "horizontal",
        8 => "depth",
        9 | 10 => "outlier",
        _ => "unknown",
    }
}

const BASE: &str = "flex flex-col gap-8";

/// Compose the root class string: the base layout classes followed by the
/// caller's extra classes.
pub fn root_classes(extra: &str) -> String {
    if extra.is_empty() {
        BASE.to_string()
    } else {
        format!("{BASE} {extra}")
    }
}

/// One entry in the Mzizi changelog, mirroring the `public.changelog`
/// table.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry {
    /// The released version, e.g. `"0.7.10"`.
    pub version: String,
    /// The entry's short title.
    pub title: String,
    /// The entry's longer description.
    pub description: String,
    /// The human-readable release date.
    pub date: String,
    /// Ecosystem node numbers this entry affected. Mirrors the
    /// `nodes_affected` `integer[]` column exactly; empty when the entry
    /// affected no nodes.
    pub nodes_affected: Vec<u32>,
    /// Component names added by this entry; empty when none were added.
    pub components_added: Vec<String>,
    /// Component names modified by this entry; empty when none were
    /// modified.
    pub components_modified: Vec<String>,
    /// Component names deprecated by this entry; empty when none were
    /// deprecated.
    pub components_deprecated: Vec<String>,
}

/// Props for [`ChangelogRenderer`].
#[derive(Props, Clone, PartialEq)]
pub struct ChangelogRendererProps {
    /// The changelog entries to render, in the order given.
    pub entries: Vec<ChangelogEntry>,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
}

/// Renders the database changelog as a visual timeline.
#[component]
pub fn ChangelogRenderer(props: ChangelogRendererProps) -> Element {
    rsx! {
        div {
            "data-slot": "nyuchi-changelog-renderer",
            "data-portal": "https://mzizi.dev/components/nyuchi-changelog-renderer",
            class: root_classes(&props.class),
            role: "feed",
            "aria-label": "Changelog",

            for entry in props.entries.iter() {
                article {
                    key: "{entry.version}",
                    class: "relative border-l-2 border-border pl-6",

                    div { class: "absolute top-1 -left-[5px] size-2 rounded-full bg-primary" }

                    div { class: "flex flex-wrap items-baseline gap-3",
                        span { class: "font-mono text-sm font-bold text-primary", "{entry.version}" }
                        time { class: "text-xs text-muted-foreground", "{entry.date}" }
                    }

                    h3 { class: "mt-1 text-lg font-semibold", "{entry.title}" }
                    p { class: "mt-1 text-sm text-muted-foreground", "{entry.description}" }

                    if !entry.nodes_affected.is_empty() {
                        div { class: "mt-2 flex flex-wrap gap-1", "aria-label": "Nodes affected",
                            for n in entry.nodes_affected.iter() {
                                span {
                                    key: "{n}",
                                    class: "rounded-full px-2 py-0.5 text-xs font-medium {node_accent(*n).classes()}",
                                    title: "N{n} — {node_label(*n)} ({node_axis_text(*n)} axis)",
                                    "N{n}"
                                }
                            }
                        }
                    }

                    if !entry.components_added.is_empty() {
                        div { class: "mt-2 flex flex-wrap gap-1", "aria-label": "Components added",
                            for c in entry.components_added.iter() {
                                span {
                                    key: "{c}",
                                    class: "rounded bg-[var(--status-success,#64FFDA)]/10 px-1.5 py-0.5 text-xs text-[var(--status-success,#22C55E)]",
                                    "+{c}"
                                }
                            }
                        }
                    }

                    if !entry.components_modified.is_empty() {
                        div { class: "mt-1 flex flex-wrap gap-1", "aria-label": "Components modified",
                            for c in entry.components_modified.iter() {
                                span {
                                    key: "{c}",
                                    class: "rounded bg-[var(--color-cobalt)]/10 px-1.5 py-0.5 text-xs text-[var(--color-cobalt)]",
                                    "~{c}"
                                }
                            }
                        }
                    }

                    if !entry.components_deprecated.is_empty() {
                        div { class: "mt-1 flex flex-wrap gap-1", "aria-label": "Components deprecated",
                            for c in entry.components_deprecated.iter() {
                                span {
                                    key: "{c}",
                                    class: "rounded bg-[var(--status-warning,#F59E0B)]/10 px-1.5 py-0.5 text-xs text-[var(--status-warning,#F59E0B)] line-through",
                                    "{c}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
