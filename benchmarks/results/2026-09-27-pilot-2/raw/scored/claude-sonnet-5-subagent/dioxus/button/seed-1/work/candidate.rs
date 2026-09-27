//! BUTTON — N2 primitive, Dioxus.
//!
//! Nyuchi Frontend Architecture: Layer 2 (Primitives).
//!
//! Token compliance:
//! - Radius: rounded-full (9999px) — buttons are always pill per brand.
//! - Touch target: 56px default, 48px minimum — never below 48px.
//! - Focus ring: uses the `--ring` token from the semantic layer.
//! - Colors: uses semantic tokens (primary, secondary, muted, destructive).
//! - `data-slot`: present for CSS targeting and testing.
//!
//! No harness — primitives are too low-level for observability wiring. Brand
//! components that use `Button` wire into the harness at their own level.

use dioxus::prelude::*;

/// Visual treatment of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Solid primary fill — the default.
    #[default]
    Default,
    /// Outlined, subtle input-tinted fill.
    Outline,
    /// Solid secondary fill.
    Secondary,
    /// No fill until hovered or expanded.
    Ghost,
    /// Destructive / dangerous action.
    Destructive,
    /// Renders as an inline text link.
    Link,
}

impl ButtonVariant {
    /// The Tailwind classes for this variant.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => "bg-primary text-primary-foreground hover:bg-primary/80",
            Self::Outline => "border-border bg-input/30 hover:bg-input/50 hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground",
            Self::Secondary => "bg-secondary text-secondary-foreground hover:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
            Self::Ghost => "hover:bg-muted hover:text-foreground dark:hover:bg-muted/50 aria-expanded:bg-muted aria-expanded:text-foreground",
            Self::Destructive => "bg-destructive/10 hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/20 text-destructive focus-visible:border-destructive/40 dark:hover:bg-destructive/30",
            Self::Link => "text-primary underline-offset-4 hover:underline",
        }
    }

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
            Self::Secondary => "secondary",
            Self::Ghost => "ghost",
            Self::Destructive => "destructive",
            Self::Link => "link",
        }
    }
}

/// Size (touch target) of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 56px touch target — the brand standard.
    #[default]
    Default,
    /// 48px touch target — the minimum allowed.
    Sm,
    /// 56px touch target, wider padding.
    Lg,
    /// Square 56px touch target, for icon-only buttons.
    Icon,
    /// Square 48px touch target, for icon-only buttons.
    IconSm,
}

impl ButtonSize {
    /// The Tailwind classes for this size.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => "h-14 gap-2 px-5 has-data-[icon=inline-end]:pr-4 has-data-[icon=inline-start]:pl-4",
            Self::Sm => "h-12 gap-1.5 px-4 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
            Self::Lg => "h-14 gap-2 px-6 has-data-[icon=inline-end]:pr-5 has-data-[icon=inline-start]:pl-5",
            Self::Icon => "size-14",
            Self::IconSm => "size-12",
        }
    }

    /// The `data-size` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
            Self::Icon => "icon",
            Self::IconSm => "icon-sm",
        }
    }
}

const BASE: &str = "focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 rounded-full border border-transparent bg-clip-padding text-sm font-medium focus-visible:ring-[3px] aria-invalid:ring-[3px] [&_svg:not([class*='size-'])]:size-4 inline-flex items-center justify-center whitespace-nowrap transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none shrink-0 [&_svg]:shrink-0 outline-none group/button select-none";

/// Compose the full class string for a button: base classes, the variant's
/// classes, the size's classes and the caller's extra classes, in that order.
pub fn button_classes(variant: ButtonVariant, size: ButtonSize, extra: &str) -> String {
    let variant_classes = variant.classes();
    let size_classes = size.classes();
    let mut out = String::with_capacity(
        BASE.len() + variant_classes.len() + size_classes.len() + extra.len() + 3,
    );
    out.push_str(BASE);
    out.push(' ');
    out.push_str(variant_classes);
    out.push(' ');
    out.push_str(size_classes);
    if !extra.is_empty() {
        out.push(' ');
        out.push_str(extra);
    }
    out
}

/// Props for [`Button`].
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    /// Visual treatment.
    #[props(default)]
    pub variant: ButtonVariant,
    /// Touch-target size.
    #[props(default)]
    pub size: ButtonSize,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Any other HTML attribute, including `type`, `disabled` and event
    /// handlers such as `onclick`.
    #[props(extends = GlobalAttributes, extends = button)]
    pub attributes: Vec<Attribute>,
    /// Button label and/or icon content.
    pub children: Element,
}

/// A pill-shaped button. Shares its contract with `button.tsx`.
#[component]
pub fn Button(props: ButtonProps) -> Element {
    rsx! {
        button {
            "data-slot": "button",
            "data-portal": "https://mzizi.dev/components/button",
            "data-variant": props.variant.slug(),
            "data-size": props.size.slug(),
            class: button_classes(props.variant, props.size, &props.class),
            ..props.attributes,
            {props.children}
        }
    }
}
