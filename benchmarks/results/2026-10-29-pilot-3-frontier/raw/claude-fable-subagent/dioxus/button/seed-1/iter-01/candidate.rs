//! BUTTON — Layer 2 primitive, Dioxus.
//!
//! A pill-shaped button: a single `button` element styled from semantic colour
//! tokens. Its touch target is 56px by default and never less than 48px.

use dioxus::prelude::*;

/// The colour treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Solid primary fill.
    #[default]
    Default,
    /// Bordered, translucent input background.
    Outline,
    /// Secondary fill.
    Secondary,
    /// No fill until hovered.
    Ghost,
    /// Destructive tint.
    Destructive,
    /// Rendered as a text link.
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

/// Height and padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 56px.
    #[default]
    Default,
    /// 48px; the minimum size allowed.
    Sm,
    /// 56px, with wider padding.
    Lg,
    /// 56px, square.
    Icon,
    /// 48px, square.
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

/// The classes every button carries.
const BASE: &str = "focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 rounded-full border border-transparent bg-clip-padding text-sm font-medium focus-visible:ring-[3px] aria-invalid:ring-[3px] [&_svg:not([class*='size-'])]:size-4 inline-flex items-center justify-center whitespace-nowrap transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none shrink-0 [&_svg]:shrink-0 outline-none group/button select-none";

/// Join the base classes, the variant's classes, the size's classes and the caller's
/// extra classes, with no trailing space when `extra` is empty.
pub fn compose(variant: ButtonVariant, size: ButtonSize, extra: &str) -> String {
    if extra.is_empty() {
        format!("{BASE} {} {}", variant.classes(), size.classes())
    } else {
        format!("{BASE} {} {} {extra}", variant.classes(), size.classes())
    }
}

/// Props for [`Button`].
#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    /// The colour treatment.
    #[props(default)]
    pub variant: ButtonVariant,
    /// Height and padding.
    #[props(default)]
    pub size: ButtonSize,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Called when the button is clicked.
    #[props(default)]
    pub onclick: Option<EventHandler<MouseEvent>>,
    /// Any other HTML attribute, such as `type`, `disabled` or `aria-label`.
    #[props(extends = GlobalAttributes, extends = button)]
    pub attributes: Vec<Attribute>,
    /// The button's content.
    pub children: Element,
}

/// A pill-shaped button.
#[component]
pub fn Button(props: ButtonProps) -> Element {
    let class = compose(props.variant, props.size, &props.class);
    rsx! {
        button {
            "data-slot": "button",
            "data-portal": "https://mzizi.dev/components/button",
            "data-variant": props.variant.slug(),
            "data-size": props.size.slug(),
            class,
            onclick: move |event| {
                if let Some(handler) = props.onclick {
                    handler.call(event);
                }
            },
            ..props.attributes,
            {props.children}
        }
    }
}
