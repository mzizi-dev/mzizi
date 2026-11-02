# Writing one registry component in Dioxus 0.7.10

You are writing one registry component as a single Rust file against **Dioxus 0.7.10**.
Your file is checked with `cargo check`, as a module of a small crate whose only
dependency is `dioxus` (features `macro`, `signals`, `hooks`, `html`; no renderer).
Errors fail it; warnings do not.

## File shape

A file starts with a `//!` header naming the component and its tier, then
`use dioxus::prelude::*;`, then the variant enums, the class constants, the props struct
and the component function, in that order. A component is a function marked
`#[component]` that takes a props struct and returns an `Element`. Its markup comes from
the `rsx!` macro.

## What is available

Only `use dioxus::prelude::*;`. You cannot add crates, and there is no shared helper
module: everything the component needs lives in the file.

**Variant enums.** A visual variant is a plain enum deriving
`Debug, Clone, Copy, PartialEq, Eq, Default`, with `#[default]` on the default variant.
Each enum has a `classes()` method, a `match` returning each variant's Tailwind classes,
and a `slug()` method returning the string the component emits as `data-variant` (or
`data-size`). Both are `pub const fn` returning `&'static str`. The `match` is the whole
variant system, and the compiler rejects a variant with no classes. Do not add a `_ =>`
arm; it turns that check off.

**Props.** Each field of the props struct is one prop, and the struct derives
`Props, Clone, PartialEq`.

- `#[props(default)]` uses the type's `Default`: the `#[default]` variant for an enum,
  `""` for a `String`, `false` for a `bool`, `None` for an `Option`.
- `#[props(default = value)]` gives an explicit default. A field with neither is required.
- `pub children: Element` receives whatever the caller nests inside the component.
- `#[props(extends = GlobalAttributes)]` on a `pub attributes: Vec<Attribute>` field
  collects every other HTML attribute the caller passes (`id`, `aria-*`, `type`). Spread
  it with `..props.attributes` after the component's own attributes.
- An event callback is `Option<EventHandler<MouseEvent>>` with `#[props(default)]`, called
  as `handler.call(event)`.
- Every registry component takes `class: String`, default empty, appended last.

```rust
use dioxus::prelude::*;

/// Props for [`Tag`].
#[derive(Props, Clone, PartialEq)]
pub struct TagProps {
    /// Show the tag as selected.
    #[props(default = false)]
    pub selected: bool,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Called when the tag is clicked.
    #[props(default)]
    pub onselect: Option<EventHandler<MouseEvent>>,
    /// Any other HTML attribute, including a link's `href`.
    #[props(extends = GlobalAttributes, extends = a)]
    pub attributes: Vec<Attribute>,
    /// Tag label.
    pub children: Element,
}

/// A selectable tag, rendered as a link.
#[component]
pub fn Tag(props: TagProps) -> Element {
    rsx! {
        a {
            "data-slot": "tag",
            "data-selected": props.selected.then_some("true"),
            class: format!("inline-flex rounded-md px-2 {}", props.class).trim_end().to_string(),
            onclick: move |event| {
                if let Some(handler) = props.onselect {
                    handler.call(event);
                }
            },
            ..props.attributes,
            {props.children}
        }
    }
}
```

**The view, `rsx!`.**

- An element is its tag followed by braces: `div { … }`.
- An attribute is `name: value`. HTML names that are not Rust identifiers (`data-*`,
  `aria-*`) are string literals: `"data-slot": "tag"`. `type` is `r#type`.
- A value can be any expression: a literal, a method call, a `String`. An `Option<&str>`
  value renders the attribute only when it is `Some`.
- A local variable with the attribute's name is written once: `class,`.
- Text is a string literal: `"Remove"`. Format arguments work inline: `"{count} items"`.
- `{props.children}` renders the nested content.
- `if`, `if let Some(x) = …` and `for` work directly inside `rsx!`, with an `else` branch
  where one thing or another renders.
- An event handler is a closure: `onclick: move |event| { … }`.

**Class strings.** Keep the classes every instance carries in one `const BASE: &str`, and
join the base, each enum's `classes()` and the caller's `class` with single spaces, with
no trailing space when `class` is empty:

```rust
const BASE: &str = "inline-flex items-center gap-1 rounded-full text-xs font-medium";

/// Join the base classes, a tone's classes and the caller's extra classes.
pub fn compose(tone_classes: &str, extra: &str) -> String {
    if extra.is_empty() {
        format!("{BASE} {tone_classes}")
    } else {
        format!("{BASE} {tone_classes} {extra}")
    }
}
```

| Spec says                       | Dioxus                                 |
| ------------------------------- | -------------------------------------- |
| `data-slot="x"`                 | `"data-slot": "x"`                     |
| `data-size` is the size in use  | `"data-size": props.size.slug()`       |
| its children                    | `{props.children}`                     |
| a click handler                 | `onclick:` with a closure, as in `Tag` |
| shown only when a flag is set   | `if props.open { … }`                  |
| one thing or another            | `if … { … } else { … }`                |
| once per item of a list         | `for item in props.items.iter() { … }` |
| an optional value, when present | `if let Some(x) = props.x { … }`       |
| other attributes pass through   | `..props.attributes`                   |

## Naming rules

- Types, enums and variants are `PascalCase` (`IconSm`). Fields and functions are
  `snake_case`. The props struct is `<Name>Props`.
- Name each enum exactly as the task says, and keep every variant, class string and
  default the spec gives, character for character. Do not invent or drop variants.
- A slug is the spec's kebab-case variant name (`"icon-sm"`).
- `"data-slot"` on the root names the component in kebab-case; sub-parts use
  `<name>-<part>`. The root also carries `"data-portal"` when the spec gives one, and
  `"data-variant"` / `"data-size"` carry the enum's `slug()`.
- Every public item has a `///` doc comment.
- One self-contained file: no `mod` declarations and no crates other than `dioxus`.
- Classes use the spec's semantic tokens (`bg-primary`, `text-muted-foreground`), exactly
  as the spec writes them.

## Reading the checker's output

The check prints each error as rustc renders it, with its `help:` and `note:` lines, and
nothing else. This file is wrong on purpose:

```rust
// WRONG ON PURPOSE: a variant with no classes.
use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagSize {
    #[default]
    Snug,
    Roomy,
}

impl TagSize {
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Snug => "h-13 px-3",
        }
    }
}

#[component]
pub fn Tag(size: TagSize) -> Element {
    rsx! { span { "data-slot": "tag", class: size.classes() } }
}
```

`check.sh tag.rs` exits 1 and prints:

```text
error[E0004]: non-exhaustive patterns: `TagSize::Roomy` not covered
  --> tag.rs:13:15
   |
13 |         match self {
   |               ^^^^ pattern `TagSize::Roomy` not covered
   |
note: `TagSize` defined here
  --> tag.rs:5:10
   |
 5 | pub enum TagSize {
   |          ^^^^^^^
...
 8 |     Roomy,
   |     ----- not covered
   = note: the matched value is of type `TagSize`
help: ensure that all possible cases are being handled by adding a match arm with a wildcard pattern or an explicit pattern as shown
   |
14 ~             Self::Snug => "h-13 px-3",
15 ~             TagSize::Roomy => todo!(),
   |


For more information about this error, try `rustc --explain E0004`.
```

`-->` gives `file:line:column`. `help:` lines often carry the fix, and `~` lines show the
replacement text. Here the fix is to add the missing arm with the variant's real classes,
not `todo!()`. Fix every error, then check again: rustc reports several at once, and a
later one can be a consequence of an earlier one. Warnings, such as an unused variable, are printed
and do not fail the check.

## Worked example: avatar

The registry's avatar (`n2-primitives/avatar`), which is not one of the tasks:

```rust
//! AVATAR — N2 primitive, Dioxus.
//!
//! A person or entity image, with initials as the fallback when no image is given.

use dioxus::prelude::*;

/// Avatar diameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AvatarSize {
    /// 40px.
    #[default]
    Default,
    /// 32px.
    Sm,
    /// 56px.
    Lg,
}

impl AvatarSize {
    /// The Tailwind classes for this size.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Default => "size-10",
            Self::Sm => "size-8",
            Self::Lg => "size-14",
        }
    }

    /// The fallback's text size.
    pub const fn text_classes(self) -> &'static str {
        match self {
            Self::Default => "text-sm",
            Self::Sm => "text-xs",
            Self::Lg => "text-base",
        }
    }

    /// The `data-size` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Sm => "sm",
            Self::Lg => "lg",
        }
    }
}

const BASE: &str = "group/avatar relative flex shrink-0 overflow-hidden rounded-full bg-muted";

/// Props for [`Avatar`].
#[derive(Props, Clone, PartialEq)]
pub struct AvatarProps {
    /// Diameter.
    #[props(default)]
    pub size: AvatarSize,
    /// Image URL; the initials show when it is `None`.
    #[props(default)]
    pub image: Option<String>,
    /// Fallback initials.
    pub initials: String,
    /// Alternative text for the image.
    pub alt: String,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
}

/// A round avatar.
#[component]
pub fn Avatar(props: AvatarProps) -> Element {
    let class = format!("{BASE} {} {}", props.size.classes(), props.class)
        .trim_end()
        .to_string();
    rsx! {
        figure {
            "data-slot": "avatar",
            "data-portal": "https://mzizi.dev/components/avatar",
            "data-size": props.size.slug(),
            class,
            if let Some(src) = props.image {
                img {
                    "data-slot": "avatar-image",
                    src,
                    alt: props.alt,
                    class: "aspect-square size-full rounded-full object-cover",
                }
            } else {
                span {
                    "data-slot": "avatar-fallback",
                    class: format!(
                        "flex size-full items-center justify-center rounded-full bg-muted text-muted-foreground {}",
                        props.size.text_classes()
                    ),
                    {props.initials}
                }
            }
        }
    }
}
```
