# Writing one registry component in Leptos 0.8.21

You are writing one registry component as a single Rust file against **Leptos 0.8.21**
(the `csr` feature, client-side rendering). Your file is checked with `cargo check`, as a
module of a small crate whose only dependency is `leptos`. Errors fail it; warnings do
not.

## File shape

A file starts with a `//!` header naming the component and its tier, then
`use leptos::prelude::*;`, then the variant enums, the class constants and the component
function, in that order. A component is a function marked `#[component]` whose parameters
are its props, directly: there is no separate props struct. It returns `impl IntoView`,
and its markup comes from the `view!` macro.

## What is available

Only `use leptos::prelude::*;`, and `use leptos::ev::...;` for typed event handlers. You
cannot add crates, and there is no shared helper module: everything the component needs
lives in the file.

**Variant enums.** A visual variant is a plain enum deriving
`Debug, Clone, Copy, PartialEq, Eq, Default`, with `#[default]` on the default variant.
Each enum has a `classes()` method, a `match` returning each variant's Tailwind classes,
and a `slug()` method returning the string the component emits as `data-variant` (or
`data-size`). Both are `pub const fn` returning `&'static str`. The `match` is the whole
variant system, and the compiler rejects a variant with no classes. Do not add a `_ =>`
arm; it turns that check off.

**Props.** Each function parameter is one prop, and `#[prop(...)]` sets its default:

- `#[prop(optional)]` uses the type's `Default`: the `#[default]` variant for an enum,
  `""` for a `String`, `false` for a `bool`, `None` for an `Option`.
- `#[prop(default = value)]` gives an explicit default. A parameter with neither is
  required.
- `#[prop(into)]` accepts anything that converts into the type (`&str` into `String`);
  combine as `#[prop(optional, into)]`.
- `children: Children` receives nested content, rendered as `{children()}`. Omit it for
  a component that takes none.
- An event callback is `Option<Callback<T>>` with `#[prop(optional)]`, where `T` is the
  event (`MouseEvent`, or `()`), called as `handler.run(event)`.
- Every registry component takes `class: String`, default empty, appended last.
- There is no pass-through of arbitrary HTML attributes: Leptos 0.8's `dyn_attrs` is
  disabled in the shipped macro crate. Accept `class` and named props only.

```rust
use leptos::ev::MouseEvent;
use leptos::prelude::*;

/// A selectable tag, rendered as a link.
#[component]
pub fn Tag(
    /// Show the tag as selected.
    #[prop(optional)]
    selected: bool,
    /// Extra classes, appended last.
    #[prop(optional, into)]
    class: String,
    /// Called when the tag is clicked.
    #[prop(optional)]
    onselect: Option<Callback<MouseEvent>>,
    /// Tag label.
    children: Children,
) -> impl IntoView {
    view! {
        <a
            data-slot="tag"
            data-selected=selected.then_some("true")
            class=format!("inline-flex rounded-md px-2 {class}").trim_end().to_string()
            on:click=move |ev| {
                if let Some(handler) = onselect {
                    handler.run(ev);
                }
            }
        >
            {children()}
        </a>
    }
}
```

**The view, `view!`.**

- An element is a JSX-like tag: `<div>…</div>`, or self-closing `<span … />`.
- An attribute is `name=value`, written directly in kebab-case: `data-slot="tag"`,
  `aria-label=label`.
- A value can be any expression: a literal, a method call, a `String`. An `Option<&str>`
  value renders the attribute only when it is `Some`.
- Text is a string literal: `"Remove"`. Build formatted text with `format!`.
- `{expr}` embeds any `impl IntoView`: `.map(...)` on an `Option` renders only when it is
  set, and a `match` whose arms end in `.into_any()` renders one thing or another.
- An event handler is `on:<event>=closure`: `on:click=move |ev| { … }`.
- `class:<name>=condition` toggles one class.

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

| Spec says                       | Leptos                                              |
| ------------------------------- | --------------------------------------------------- |
| `data-slot="x"`                 | `data-slot="x"`                                     |
| `data-size` is the size in use  | `data-size=size.slug()`                             |
| its children                    | `{children()}`                                      |
| a click handler                 | `on:click=` with a closure, as in `Tag`             |
| shown only when a flag is set   | `{open.then(...)}`, the closure returning a `view!` |
| one thing or another            | `{match … { … }}`, each arm `.into_any()`           |
| once per item of a list         | `{items.into_iter().map(…).collect_view()}`         |
| an optional value, when present | `{x.map(...)}`, the closure returning a `view!`     |

## Naming rules

- Types, enums and variants are `PascalCase` (`IconSm`). Parameters and functions are
  `snake_case`.
- Name each enum exactly as the task says, and keep every variant, class string and
  default the spec gives, character for character. Do not invent or drop variants.
- A slug is the spec's kebab-case variant name (`"icon-sm"`).
- `data-slot` on the root names the component in kebab-case; sub-parts use
  `<name>-<part>`. The root also carries `data-portal` when the spec gives one, and
  `data-variant` / `data-size` carry the enum's `slug()`.
- Every public item and every prop parameter has a `///` doc comment.
- One self-contained file: no `mod` declarations and no crates other than `leptos`.
- Classes use the spec's semantic tokens (`bg-primary`, `text-muted-foreground`), exactly
  as the spec writes them.

## Reading the checker's output

The check prints each error as rustc renders it, with its `help:` and `note:` lines, and
nothing else. This file is wrong on purpose:

```rust
// WRONG ON PURPOSE: a variant with no classes.
use leptos::prelude::*;

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
pub fn Tag(#[prop(optional)] size: TagSize) -> impl IntoView {
    view! { <span data-slot="tag" class=size.classes() /> }
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
later one can be a consequence of an earlier one. Warnings, such as an unused variable,
are printed and do not fail the check.

## Worked example: avatar

The registry's avatar (`n2-primitives/avatar`), which is not one of the tasks:

```rust
//! AVATAR — N2 primitive, Leptos.
//!
//! A person or entity image, with initials as the fallback when no image is given.

use leptos::prelude::*;

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

/// A round avatar.
#[component]
pub fn Avatar(
    /// Diameter.
    #[prop(optional)]
    size: AvatarSize,
    /// Image URL; the initials show when it is `None`.
    #[prop(optional, into)]
    image: Option<String>,
    /// Fallback initials.
    #[prop(into)]
    initials: String,
    /// Alternative text for the image.
    #[prop(into)]
    alt: String,
    /// Extra classes, appended last.
    #[prop(optional, into)]
    class: String,
) -> impl IntoView {
    let root = format!("{BASE} {} {class}", size.classes()).trim_end().to_string();
    let fallback = format!(
        "flex size-full items-center justify-center rounded-full bg-muted text-muted-foreground {}",
        size.text_classes()
    );
    view! {
        <figure
            data-slot="avatar"
            data-portal="https://mzizi.dev/components/avatar"
            data-size=size.slug()
            class=root
        >
            {match image {
                Some(src) => view! {
                    <img
                        data-slot="avatar-image"
                        src=src
                        alt=alt
                        class="aspect-square size-full rounded-full object-cover"
                    />
                }
                .into_any(),
                None => view! {
                    <span data-slot="avatar-fallback" class=fallback>
                        {initials}
                    </span>
                }
                .into_any(),
            }}
        </figure>
    }
}
```
