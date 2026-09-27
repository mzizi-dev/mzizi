use dioxus::prelude::*;

/// NYUCHI CHANGELOG RENDERER — N10: Documentation Outlier
///
/// Renders the database changelog as a visual timeline.
/// Reads from: public.changelog (nodes_affected, components_added, etc.)
/// Note: The changelog table uses nodes_affected (integer[]) —
/// this interface mirrors that column name exactly.

/// Changelog entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangelogEntry {
    /// Version number.
    pub version: String,
    /// Title of the change.
    pub title: String,
    /// Description of the change.
    pub description: String,
    /// Date of the change.
    pub date: String,
    /// Nodes affected by the change.
    pub nodes_affected: Option<Vec<i32>>,
    /// Components added by the change.
    pub components_added: Option<Vec<String>>,
    /// Components modified by the change.
    pub components_modified: Option<Vec<String>>,
    /// Components deprecated by the change.
    pub components_deprecated: Option<Vec<String>>,
}

/// Node accent variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeAccent {
    /// Vertical axis.
    #[default]
    Vertical,
    /// Horizontal axis.
    Horizontal,
    /// Depth axis.
    Depth,
    /// Outlier axis.
    Outlier,
}

impl NodeAccent {
    /// The Tailwind classes for this accent.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Vertical => "bg-[var(--color-tanzanite)]/10 text-[var(--color-tanzanite)]",
            Self::Horizontal => "bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]",
            Self::Depth => "bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]",
            Self::Outlier => "bg-[var(--color-gold)]/10 text-[var(--color-gold)]",
        }
    }

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
            Self::Depth => "depth",
            Self::Outlier => "outlier",
        }
    }
}

const BASE: &str = "flex flex-col gap-8";

/// Compose the full class string for a node.
pub fn node_classes(accents: &[NodeAccent], extra: &str) -> String {
    let mut out = String::with_capacity(BASE.len() + accents.len() * 10 + extra.len() + 2);
    out.push_str(BASE);
    if !accents.is_empty() {
        out.push(' ');
        for accent in accents {
            out.push_str(accent.classes());
            out.push(' ');
        }
    }
    if !extra.is_empty() {
        out.push(' ');
        out.push_str(extra);
    }
    out
}

/// Props for [`ChangelogRenderer`].
#[derive(Props, Clone, PartialEq)]
pub struct ChangelogRendererProps {
    /// Changelog entries.
    pub entries: Vec<ChangelogEntry>,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Any other HTML attribute.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
}

/// Ecosystem node labels — maps node number to its title
const NODE_LABELS: [(&str, NodeAccent); 10] = [
    ("Tokens", NodeAccent::Vertical),
    ("Primitives", NodeAccent::Horizontal),
    ("Brand", NodeAccent::Horizontal),
    ("Safety", NodeAccent::Vertical),
    ("Resilience", NodeAccent::Vertical),
    ("Pages", NodeAccent::Horizontal),
    ("Shell", NodeAccent::Horizontal),
    ("Assurance", NodeAccent::Depth),
    ("Fundi", NodeAccent::Outlier),
    ("Documentation", NodeAccent::Outlier),
];

/// A component that renders a changelog as a visual timeline.
#[component]
pub fn ChangelogRenderer(props: ChangelogRendererProps) -> Element {
    rsx! {
        div {
            "data-slot": "nyuchi-changelog-renderer",
            "data-portal": "https://mzizi.dev/components/nyuchi-changelog-renderer",
            class: node_classes(&[NodeAccent::Default], &props.class),
            ..props.attributes,
            role: "feed",
            "aria-label": "Changelog",
            props.entries.iter().map(|entry| {
                rsx! {
                    article {
                        key: "{entry.version}",
                        class: "relative border-l-2 border-border pl-6",
                        // Timeline dot
                        div {
                            "class": "absolute top-1 -left-[5px] size-2 rounded-full bg-primary"
                        }
                        // Version + date
                        div {
                            class: "flex flex-wrap items-baseline gap-3",
                            span {
                                "class": "font-mono text-sm font-bold text-primary",
                                "{entry.version}"
                            }
                            time {
                                "class": "text-xs text-muted-foreground",
                                "{entry.date}"
                            }
                        }
                        h3 {
                            "class": "mt-1 text-lg font-semibold",
                            "{entry.title}"
                        }
                        p {
                            "class": "mt-1 text-sm text-muted-foreground",
                            "{entry.description}"
                        }
                        // Nodes affected badges
                        if let Some(nodes_affected) = &entry.nodes_affected {
                            if !nodes_affected.is_empty() {
                                div {
                                    "class": "mt-2 flex flex-wrap gap-1",
                                    "aria-label": "Nodes affected",
                                    nodes_affected.iter().map(|n| {
                                        rsx! {
                                            span {
                                                "class": "rounded-full px-2 py-0.5 text-xs font-medium",
                                                "{NodeAccent::slug(NODE_LABELS[*n as usize].1)}",
                                                title: "N{n} — {NODE_LABELS[*n as usize].0} ({NODE_LABELS[*n as usize].1} axis)",
                                                "N{n}"
                                            }
                                        }
                                    })
                                }
                            }
                        }
                        // Component changes
                        if let Some(components_added) = &entry.components_added {
                            if !components_added.is_empty() {
                                div {
                                    "class": "mt-2 flex flex-wrap gap-1",
                                    "aria-label": "Components added",
                                    components_added.iter().map(|c| {
                                        rsx! {
                                            span {
                                                "class": "rounded bg-[var(--status-success,#64FFDA)]/10 px-1.5 py-0.5 text-xs text-[var(--status-success,#22C55E)]",
                                                "+{c}"
                                            }
                                        }
                                    })
                                }
                            }
                        }
                        if let Some(components_modified) = &entry.components_modified {
                            if !components_modified.is_empty() {
                                div {
                                    "class": "mt-1 flex flex-wrap gap-1",
                                    "aria-label": "Components modified",
                                    components_modified.iter().map(|c| {
                                        rsx! {
                                            span {
                                                "class": "rounded bg-[var(--color-cobalt)]/10 px-1.5 py-0.5 text-xs text-[var(--color-cobalt)]",
                                                "~{c}"
                                            }
                                        }
                                    })
                                }
                            }
                        }
                        if let Some(components_deprecated) = &entry.components_deprecated {
                            if !components_deprecated.is_empty() {
                                div {
                                    "class": "mt-1 flex flex-wrap gap-1",
                                    "aria-label": "Components deprecated",
                                    components_deprecated.iter().map(|c| {
                                        rsx! {
                                            span {
                                                "class": "rounded bg-[var(--status-warning,#F59E0B)]/10 px-1.5 py-0.5 text-xs text-[var(--status-warning,#F59E0B)] line-through",
                                                "{c}"
                                            }
                                        }
                                    })
                                }
                            }
                        }
                    }
                }
            })
        }
    }
}
