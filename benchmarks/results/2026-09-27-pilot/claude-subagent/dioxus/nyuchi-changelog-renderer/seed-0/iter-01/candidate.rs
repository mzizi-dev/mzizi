//! NYUCHI CHANGELOG RENDERER — N10: Documentation Outlier, Dioxus.
//!
//! Renders a feed of changelog entries, each annotated with the architecture
//! nodes it touches and the components it adds, modifies or deprecates.
//! Shares its contract with `nyuchi-changelog-renderer.tsx`.

use dioxus::prelude::*;

/// Visual accent for a node badge, keyed by the node's architecture axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeAccent {
    /// Cobalt accent — horizontal-axis nodes. Also the fallback for an
    /// unrecognised node number.
    #[default]
    Horizontal,
    /// Tanzanite accent — vertical-axis nodes.
    Vertical,
    /// Malachite accent — depth-axis nodes.
    Depth,
    /// Gold accent — outlier-axis nodes.
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

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Depth => "depth",
            Self::Outlier => "outlier",
        }
    }
}

/// Looks up the human-readable label for a node number, e.g. `1` → `"Tokens"`.
/// Falls back to `"Unknown"` for a node the table doesn't cover.
pub const fn node_label(node: u32) -> &'static str {
    match node {
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

/// Looks up the architecture axis (and thus badge accent) for a node number.
/// Falls back to [`NodeAccent::Horizontal`] for a node the table doesn't cover.
pub const fn node_axis(node: u32) -> NodeAccent {
    match node {
        1 => NodeAccent::Vertical,
        2 => NodeAccent::Horizontal,
        3 => NodeAccent::Horizontal,
        4 => NodeAccent::Vertical,
        5 => NodeAccent::Vertical,
        6 => NodeAccent::Horizontal,
        7 => NodeAccent::Horizontal,
        8 => NodeAccent::Depth,
        9 => NodeAccent::Outlier,
        10 => NodeAccent::Outlier,
        _ => NodeAccent::Horizontal,
    }
}

/// One changelog entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry {
    /// Version string, e.g. `"1.4.0"`.
    pub version: String,
    /// Short entry title.
    pub title: String,
    /// Entry description.
    pub description: String,
    /// Human-readable release date.
    pub date: String,
    /// Node numbers this entry affects.
    pub nodes_affected: Option<Vec<u32>>,
    /// Component names this entry adds.
    pub components_added: Option<Vec<String>>,
    /// Component names this entry modifies.
    pub components_modified: Option<Vec<String>>,
    /// Component names this entry deprecates.
    pub components_deprecated: Option<Vec<String>>,
}

/// Props for [`ChangelogRenderer`].
#[derive(Props, Clone, PartialEq)]
pub struct ChangelogRendererProps {
    /// Changelog entries to render, in order.
    pub entries: Vec<ChangelogEntry>,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Any other HTML attribute.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
}

const BASE: &str = "flex flex-col gap-8";

/// Compose the root class string.
fn root_classes(extra: &str) -> String {
    if extra.is_empty() {
        BASE.to_string()
    } else {
        format!("{BASE} {extra}")
    }
}

/// Renders a feed of changelog entries, each with node and component
/// annotations.
#[component]
pub fn ChangelogRenderer(props: ChangelogRendererProps) -> Element {
    rsx! {
        div {
            "data-slot": "nyuchi-changelog-renderer",
            "data-portal": "https://mzizi.dev/components/nyuchi-changelog-renderer",
            class: root_classes(&props.class),
            role: "feed",
            "aria-label": "Changelog",
            ..props.attributes,
            for entry in &props.entries {
                article {
                    key: "{entry.version}",
                    class: "relative border-l-2 border-border pl-6",
                    div {
                        class: "absolute top-1 -left-[5px] size-2 rounded-full bg-primary",
                    }
                    div {
                        class: "flex flex-wrap items-baseline gap-3",
                        span {
                            class: "font-mono text-sm font-bold text-primary",
                            "{entry.version}"
                        }
                        time {
                            class: "text-xs text-muted-foreground",
                            "{entry.date}"
                        }
                    }
                    h3 {
                        class: "mt-1 text-lg font-semibold",
                        "{entry.title}"
                    }
                    p {
                        class: "mt-1 text-sm text-muted-foreground",
                        "{entry.description}"
                    }
                    if let Some(nodes) = &entry.nodes_affected {
                        if !nodes.is_empty() {
                            div {
                                class: "mt-2 flex flex-wrap gap-1",
                                "aria-label": "Nodes affected",
                                for n in nodes {
                                    span {
                                        key: "{n}",
                                        class: "rounded-full px-2 py-0.5 text-xs font-medium {node_axis(*n).classes()}",
                                        title: "N{n} — {node_label(*n)} ({node_axis(*n).slug()} axis)",
                                        "N{n}"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(added) = &entry.components_added {
                        if !added.is_empty() {
                            div {
                                class: "mt-2 flex flex-wrap gap-1",
                                "aria-label": "Components added",
                                for c in added {
                                    span {
                                        key: "{c}",
                                        class: "rounded bg-[var(--status-success,#64FFDA)]/10 px-1.5 py-0.5 text-xs text-[var(--status-success,#22C55E)]",
                                        "+{c}"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(modified) = &entry.components_modified {
                        if !modified.is_empty() {
                            div {
                                class: "mt-1 flex flex-wrap gap-1",
                                "aria-label": "Components modified",
                                for c in modified {
                                    span {
                                        key: "{c}",
                                        class: "rounded bg-[var(--color-cobalt)]/10 px-1.5 py-0.5 text-xs text-[var(--color-cobalt)]",
                                        "~{c}"
                                    }
                                }
                            }
                        }
                    }
                    if let Some(deprecated) = &entry.components_deprecated {
                        if !deprecated.is_empty() {
                            div {
                                class: "mt-1 flex flex-wrap gap-1",
                                "aria-label": "Components deprecated",
                                for c in deprecated {
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
}
