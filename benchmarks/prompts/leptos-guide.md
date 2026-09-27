# Writing a registry component in Leptos 0.8.21

You are writing one Mzizi registry component as a single Rust file against **Leptos
0.8.21** (the `csr` feature — client-side rendering). Your file is compiled on its own
as a module of a small crate whose only dependency is `leptos`; no renderer beyond
`csr`. You cannot add crates, and there is no shared helper module: everything the
component needs lives in the file. Only `use leptos::prelude::*;` (and, for typed event
handlers, `use leptos::ev::...;`) is available.

Your file is checked with `cargo check`. Warnings are allowed; errors are not.

## What a component is

A component is a function marked `#[component]` whose parameters are its props
directly — there is no separate props struct — returning `impl IntoView`. The markup
comes from the `view!` macro. A file starts with a `//!` header naming the component
and its tier, then `use leptos::prelude::*;`.

```rust
//! DOT — N2 primitive, Leptos.

use leptos::prelude::*;

/// A small status dot.
#[component]
pub fn Dot(
    /// Accessible label.
    #[prop(into)]
    label: String,
) -> impl IntoView {
    view! {
        <span
            data-slot="dot"
            aria-label=label
            class="inline-block size-2 rounded-full bg-primary"
        />
    }
}
```

Name the function in `PascalCase`. There is no separate props type to name.

## Variant enums

A visual variant is a plain enum — the same convention regardless of framework. One
variant carries `#[default]`, so a prop of that type can be left out. Each enum has a
`classes()` method, a `match` returning the Tailwind classes for each variant, and a
`slug()` method returning the string the component emits as `data-variant` (or
`data-size`). Both are `pub const fn` returning `&'static str`.

```rust
/// Visual treatment of a chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChipTone {
    /// Neutral fill — the default.
    #[default]
    Neutral,
    /// Positive state.
    Success,
    /// Needs attention.
    Warning,
}

impl ChipTone {
    /// The Tailwind classes for this tone.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "bg-muted text-foreground",
            Self::Success => "bg-success/10 text-success",
            Self::Warning => "bg-warning/10 text-warning",
        }
    }

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Success => "success",
            Self::Warning => "warning",
        }
    }
}
```

The `match` is the whole variant system, exactly as in the Dioxus arm: it replaces
`class-variance-authority` in the TypeScript, and the compiler rejects a variant with
no classes. Do not add a `_ =>` arm; it turns that check off. Variant names are
`PascalCase` (`IconSm`), slugs are the kebab-case strings the `.tsx` uses (`"icon-sm"`).
Keep the set of variants, and every class string, exactly as the `.tsx` has them.

## Props

Each function parameter is one prop. `#[prop(...)]` attributes control its default and
conversion:

- `#[prop(optional)]` uses the type's `Default` — the `#[default]` variant for an enum,
  `""` for a `String`, `false` for a `bool`, `None` for an `Option`.
- `#[prop(default = value)]` gives an explicit default.
- `#[prop(into)]` accepts anything that converts into the field's type (e.g. `&str`
  into `String`) at the call site; combine as `#[prop(optional, into)]`.
- A parameter with no attribute is required.
- `children: Children` receives whatever the caller nests inside the component; call it
  as `{children()}` in the view. Omit the parameter entirely for a component that takes
  no children.
- An event callback is `Option<Callback<T>>` with `#[prop(optional)]`, where `T` is the
  event type (`leptos::ev::MouseEvent`, or `()` for a callback with no event data).
  Invoke it as `handler.run(event)`.

**Known gap, not an oversight**: Leptos 0.8's macro crate has a `dyn_attrs` mechanism
for accepting an arbitrary bag of extra HTML attributes (the `Vec<Attribute>` +
`extends = GlobalAttributes` equivalent the Dioxus arm uses), but it is commented out in
the shipped `leptos_macro` 0.8.18 source (`// TODO restore dyn attrs`). Do not attempt
to use it. A registry component in this arm accepts `class` and named event props only —
no generic attributes passthrough.

Every registry component takes a `class: String` prop that defaults to empty. The
caller's classes are appended last, so they win over the component's own.

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

## The view: `view!`

- An element is a JSX-like tag: `<div>…</div>`, or self-closing `<span … />`.
- An attribute is `name=value`, written directly in kebab-case — `data-slot="tag"` and
  `aria-label=label` both need no extra quoting of the name (unlike Dioxus's `rsx!`,
  which needs `"data-slot": "tag"` for non-identifier names).
- A value can be any expression: a literal, a method call, a `String`. An
  `Option<&str>` value (e.g. `selected.then_some("true")`) renders the attribute only
  when it is `Some`.
- Text is a string literal: `"Remove"`. Format arguments work through `format!`, passed
  as the attribute or child expression.
- `{children()}` renders the nested content.
- `{expr}` embeds any `impl IntoView` inline, including the result of `.map(...)` on an
  `Option` for content that renders only when a prop is set.
- An event handler is `on:<event>=closure`: `on:click=move |ev| { … }`.
- A boolean-flag class uses `class:<name>=condition`, e.g. `class:hidden=is_hidden`,
  when you need to toggle one class rather than build the whole string yourself.

## Composing class strings

Keep the classes every instance carries in one `const BASE: &str`. Build the final
string in a small function that joins the base, each enum's `classes()`, and the
caller's `class`, with single spaces and no trailing space when `class` is empty. Use
`String::with_capacity` and `push_str`, or `format!`; both are fine — identical to the
Dioxus arm's convention.

```rust
const BASE: &str = "inline-flex items-center gap-1 rounded-full text-xs font-medium";

/// Join the base classes, a tone's classes and the caller's extra classes.
pub fn compose(tone_classes: &str, extra: &str) -> String {
    let mut out = String::with_capacity(BASE.len() + tone_classes.len() + extra.len() + 2);
    out.push_str(BASE);
    out.push(' ');
    out.push_str(tone_classes);
    if !extra.is_empty() {
        out.push(' ');
        out.push_str(extra);
    }
    out
}
```

## Conventions

- **One self-contained file.** No shared helpers, no `mod` declarations, no crates other
  than `leptos`.
- **Same contract as the `.tsx`.** Same variants, same defaults, same element, same
  Tailwind classes character for character, same `data-*` attributes. Port the
  contract; do not invent or drop variants.
- **`data-slot`** on the root element names the component in kebab-case
  (`data-slot="chip"`); sub-parts use `<name>-<part>` (`data-slot="chip-remove"`).
  **`data-variant`** / **`data-size`** carry the enum's `slug()`. The root also carries
  `data-portal="https://mzizi.dev/components/<name>"`.
- **Every public item has a `///` doc comment**: enums, each variant, each prop
  parameter, each function.
- **`class` last, no attribute passthrough, children rendered.** Every component accepts
  `class` and, when it wraps content, `children`. There is no `attributes` prop (see
  "Known gap" above).
- **Semantic tokens only** in classes (`bg-primary`, `text-muted-foreground`,
  `ring-ring`), exactly as the `.tsx` writes them.

## A complete example

```rust
//! CHIP — N2 primitive, Leptos.
//!
//! A compact, optionally removable label. Shares its contract with `chip.tsx`.

use leptos::prelude::*;

/// Visual treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChipTone {
    /// Neutral fill.
    #[default]
    Neutral,
    /// Positive state.
    Success,
}

impl ChipTone {
    /// The Tailwind classes for this tone.
    pub const fn classes(self) -> &'static str {
        match self {
            Self::Neutral => "bg-muted text-foreground",
            Self::Success => "bg-success/10 text-success",
        }
    }

    /// The `data-variant` value.
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Success => "success",
        }
    }
}

const BASE: &str = "inline-flex h-7 items-center gap-1 rounded-full px-3 text-xs";

/// Compose the full class string for a chip.
pub fn chip_classes(tone: ChipTone, extra: &str) -> String {
    if extra.is_empty() {
        format!("{BASE} {}", tone.classes())
    } else {
        format!("{BASE} {} {extra}", tone.classes())
    }
}

/// A compact, optionally removable label.
#[component]
pub fn Chip(
    /// Visual treatment.
    #[prop(optional)]
    tone: ChipTone,
    /// Extra classes, appended last.
    #[prop(optional, into)]
    class: String,
    /// Called when the remove control is pressed; no control is shown without it.
    #[prop(optional)]
    onremove: Option<Callback<()>>,
    /// Chip label.
    children: Children,
) -> impl IntoView {
    view! {
        <span
            data-slot="chip"
            data-portal="https://mzizi.dev/components/chip"
            data-variant=tone.slug()
            class=chip_classes(tone, &class)
        >
            {children()}
            {onremove.map(|handler| view! {
                <button
                    data-slot="chip-remove"
                    type="button"
                    aria-label="Remove"
                    class="size-4 rounded-full"
                    on:click=move |_| handler.run(())
                >
                    "\u{d7}"
                </button>
            })}
        </span>
    }
}
```
