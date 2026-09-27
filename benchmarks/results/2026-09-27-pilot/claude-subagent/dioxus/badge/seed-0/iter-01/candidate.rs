//! BADGE — N2 primitive, Dioxus.
//!
//! A compact status label. Shares its contract with `badge.tsx`.

use dioxus::prelude::*;

/// Visual treatment of a badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    /// Solid primary fill — the default.
    #[default]
    Default,
    /// Solid secondary fill.
    Secondary,
    /// Destructive / error state.
    Destructive,
    /// Bordered, transparent fill.
    Outline,
    /// No fill until hovered.
    Ghost,
    /// Looks like a link.
    Link,
}

impl BadgeVariant {
    /// The Tailwind classes for this variant.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground [a]:hover:bg-primary/80",
            Self::Secondary => {
                "bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80"
            }
            Self::Destructive => {
                "bg-destructive/10 [a]:hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 text-destructive dark:bg-destructive/20"
            }
            Self::Outline => {
                "border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground bg-input/30"
            }
            Self::Ghost => "hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50",
            Self::Link => "text-primary underline-offset-4 hover:underline",
        }
    }

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Secondary => "secondary",
            Self::Destructive => "destructive",
            Self::Outline => "outline",
            Self::Ghost => "ghost",
            Self::Link => "link",
        }
    }
}

const BASE: &str = "h-5 gap-1 rounded-md border border-transparent px-2 py-0.5 text-xs font-medium transition-all has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&>svg]:size-3! inline-flex items-center justify-center w-fit whitespace-nowrap shrink-0 [&>svg]:pointer-events-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive overflow-hidden group/badge";

/// Compose the full class string for a badge.
pub fn badge_classes(variant: BadgeVariant, extra: &str) -> String {
    let variant_classes = variant.classes();
    let mut out =
        String::with_capacity(BASE.len() + variant_classes.len() + extra.len() + 2);
    out.push_str(BASE);
    out.push(' ');
    out.push_str(variant_classes);
    if !extra.is_empty() {
        out.push(' ');
        out.push_str(extra);
    }
    out
}

/// Props for [`Badge`].
#[derive(Props, Clone, PartialEq)]
pub struct BadgeProps {
    /// Visual treatment.
    #[props(default)]
    pub variant: BadgeVariant,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Any other HTML attribute, including `onclick` and `href`.
    #[props(extends = GlobalAttributes, extends = a)]
    pub attributes: Vec<Attribute>,
    /// Badge content.
    pub children: Element,
}

/// A small status label.
#[component]
pub fn Badge(props: BadgeProps) -> Element {
    rsx! {
        span {
            "data-slot": "badge",
            "data-portal": "https://mzizi.dev/components/badge",
            "data-variant": props.variant.slug(),
            class: badge_classes(props.variant, &props.class),
            ..props.attributes,
            {props.children}
        }
    }
}
