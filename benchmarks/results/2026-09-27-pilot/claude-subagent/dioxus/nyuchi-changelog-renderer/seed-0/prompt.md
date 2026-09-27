<!-- mzbench: SYSTEM MESSAGE BEGIN -->
# Writing a registry component in Dioxus 0.7.10

You are writing one Mzizi registry component as a single Rust file against **Dioxus
0.7.10**. Your file is compiled on its own as a module of a small crate whose only
dependency is `dioxus` (features `macro`, `signals`, `hooks`, `html`; no renderer). You
cannot add crates, and there is no shared helper module: everything the component needs
lives in the file. Only `use dioxus::prelude::*;` is available.

Your file is checked with `cargo check`. Warnings are allowed; errors are not.

## What a component is

A component is a function marked `#[component]` that takes a props struct and returns an
`Element`. The markup comes from the `rsx!` macro. A file starts with a `//!` header
naming the component and its tier, then `use dioxus::prelude::*;`.

```rust
//! DOT — N2 primitive, Dioxus.

use dioxus::prelude::*;

/// Props for [`Dot`].
#[derive(Props, Clone, PartialEq)]
pub struct DotProps {
    /// Accessible label.
    pub label: String,
}

/// A small status dot.
#[component]
pub fn Dot(props: DotProps) -> Element {
    rsx! {
        span {
            "data-slot": "dot",
            "aria-label": props.label,
            class: "inline-block size-2 rounded-full bg-primary",
        }
    }
}
```

Name the function in `PascalCase` and the props struct `<Name>Props`. Props must derive
`Props`, `Clone` and `PartialEq`: Dioxus compares props to decide whether to re-render.

## Variant enums

A visual variant is a plain enum. One variant carries `#[default]`, so a prop of that
type can be left out. Each enum has a `classes()` method, a `match` returning the
Tailwind classes for each variant, and a `slug()` method returning the string the
component emits as `data-variant` (or `data-size`). Both are `pub const fn` returning
`&'static str`.

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

The `match` is the whole variant system. It replaces `class-variance-authority` in the
TypeScript: the compiler rejects a variant with no classes. Do not add a `_ =>` arm; it
turns that check off. Variant names are `PascalCase` (`IconSm`), slugs are the
kebab-case strings the `.tsx` uses (`"icon-sm"`). Keep the set of variants, and every
class string, exactly as the `.tsx` has them.

## Props

Each field is one prop. Attributes on the field set its default:

- `#[props(default)]` uses the type's `Default`: the `#[default]` variant for an enum,
  `""` for a `String`, `false` for a `bool`, `None` for an `Option`.
- `#[props(default = value)]` gives an explicit default.
- A field with no attribute is required.
- `pub children: Element` receives whatever the caller nests inside the component.
- `#[props(extends = GlobalAttributes)]` on a `Vec<Attribute>` field collects every
  other HTML attribute the caller passes (`id`, `aria-*`, `onclick`, …). Add the
  element too, as in `extends = GlobalAttributes, extends = a`, to accept that
  element's own attributes (`href`, `target`).
- An event callback is `Option<EventHandler<T>>` with `#[props(default)]`.

Every registry component takes a `class: String` prop that defaults to empty. The caller's
classes are appended last, so they win over the component's own.

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

## The view: `rsx!`

- An element is its tag name followed by braces: `div { … }`.
- An attribute is `name: value`. HTML attribute names that are not Rust identifiers
  (`data-*`, `aria-*`) are written as string literals: `"data-slot": "tag"`.
- A value can be any expression: a literal, a method call, a `String`. An
  `Option<&str>` value renders the attribute only when it is `Some`.
- When a local variable has the attribute's name, write it once: `class,`.
- `..props.attributes` spreads the extended attributes. Put it after the component's
  own attributes and before the children.
- Text is a string literal: `"Remove"`. Format arguments work inline: `"{count} items"`.
- `{props.children}` renders the nested content.
- `if` and `for` work directly inside `rsx!`. `if let Some(x) = …` renders a piece only
  when an optional prop is set.
- An event handler is a closure: `onclick: move |event| { … }`.

## Composing class strings

Keep the classes every instance carries in one `const BASE: &str`. Build the final
string in a small function that joins the base, each enum's `classes()`, and the
caller's `class`, with single spaces and no trailing space when `class` is empty.
Use `String::with_capacity` and `push_str`, or `format!`; both are fine.

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
  than `dioxus`.
- **Same contract as the `.tsx`.** Same variants, same defaults, same element, same
  Tailwind classes character for character, same `data-*` attributes. Port the
  contract; do not invent or drop variants.
- **`data-slot`** on the root element names the component in kebab-case
  (`"data-slot": "chip"`); sub-parts use `<name>-<part>` (`"chip-remove"`).
  **`data-variant`** / **`data-size`** carry the enum's `slug()`. The root also carries
  `"data-portal": "https://mzizi.dev/components/<name>"`.
- **Every public item has a `///` doc comment**: enums, each variant, each prop field,
  each function.
- **`class` last, attributes spread, children rendered.** Every component accepts
  `class`, `attributes` and, when it wraps content, `children`.
- **Semantic tokens only** in classes (`bg-primary`, `text-muted-foreground`,
  `ring-ring`), exactly as the `.tsx` writes them.

## A complete example

```rust
//! CHIP — N2 primitive, Dioxus.
//!
//! A compact, optionally removable label. Shares its contract with `chip.tsx`.

use dioxus::prelude::*;

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

/// Props for [`Chip`].
#[derive(Props, Clone, PartialEq)]
pub struct ChipProps {
    /// Visual treatment.
    #[props(default)]
    pub tone: ChipTone,
    /// Called when the remove control is pressed; no control is shown without it.
    #[props(default)]
    pub onremove: Option<EventHandler<MouseEvent>>,
    /// Extra classes, appended last.
    #[props(default)]
    pub class: String,
    /// Any other HTML attribute.
    #[props(extends = GlobalAttributes)]
    pub attributes: Vec<Attribute>,
    /// Chip label.
    pub children: Element,
}

/// A compact label.
#[component]
pub fn Chip(props: ChipProps) -> Element {
    rsx! {
        span {
            "data-slot": "chip",
            "data-portal": "https://mzizi.dev/components/chip",
            "data-variant": props.tone.slug(),
            class: chip_classes(props.tone, &props.class),
            ..props.attributes,
            {props.children}
            if let Some(handler) = props.onremove {
                button {
                    "data-slot": "chip-remove",
                    r#type: "button",
                    "aria-label": "Remove",
                    class: "size-4 rounded-full",
                    onclick: move |event| handler.call(event),
                    "×"
                }
            }
        }
    }
}
```

<!-- mzbench: SYSTEM MESSAGE END -->

<!-- mzbench: USER MESSAGE BEGIN -->
Port this React component to Dioxus. Keep the same variants, sizes, defaults and behaviour. The result must be a single Rust source file (.rs) using Dioxus. Name the variant enums exactly `NodeAccent`, each with a `classes()` method returning its Tailwind classes. Reply with exactly one fenced code block containing the complete file and nothing else.

```tsx
"use client"
import * as React from "react"
import { cn } from "@/lib/utils"

// NYUCHI CHANGELOG RENDERER — N10: Documentation Outlier
//
// Renders the database changelog as a visual timeline.
// Reads from: public.changelog (nodes_affected, components_added, etc.)
// Note: The changelog table uses nodes_affected (integer[]) —
// this interface mirrors that column name exactly.

export interface ChangelogEntry {
  version: string
  title: string
  description: string
  date: string
  nodesAffected?: number[]
  componentsAdded?: string[]
  componentsModified?: string[]
  componentsDeprecated?: string[]
}

export interface ChangelogRendererProps {
  entries: ChangelogEntry[]
  className?: string
}

// Ecosystem node labels — maps node number to its title
const NODE_LABELS: Record<number, string> = {
  1: "Tokens",
  2: "Primitives",
  3: "Brand",
  4: "Safety",
  5: "Resilience",
  6: "Pages",
  7: "Shell",
  8: "Assurance",
  9: "Fundi",
  10: "Documentation",
}

// Node axis — determines badge colour
const NODE_AXIS: Record<number, "vertical" | "horizontal" | "depth" | "outlier"> = {
  1: "vertical",
  2: "horizontal",
  3: "horizontal",
  4: "vertical",
  5: "vertical",
  6: "horizontal",
  7: "horizontal",
  8: "depth",
  9: "outlier",
  10: "outlier",
}

const AXIS_COLOURS: Record<string, string> = {
  horizontal: "bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]",
  vertical: "bg-[var(--color-tanzanite)]/10 text-[var(--color-tanzanite)]",
  depth: "bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]",
  outlier: "bg-[var(--color-gold)]/10 text-[var(--color-gold)]",
}

export function ChangelogRenderer({ entries, className }: ChangelogRendererProps) {
  return (
    <div
      data-slot="nyuchi-changelog-renderer"
      data-portal="https://mzizi.dev/components/nyuchi-changelog-renderer"
      className={cn("flex flex-col gap-8", className)}
      role="feed"
      aria-label="Changelog"
    >
      {entries.map((entry) => (
        <article key={entry.version} className="relative border-l-2 border-border pl-6">
          {/* Timeline dot */}
          <div className="absolute top-1 -left-[5px] size-2 rounded-full bg-primary" />

          {/* Version + date */}
          <div className="flex flex-wrap items-baseline gap-3">
            <span className="font-mono text-sm font-bold text-primary">{entry.version}</span>
            <time className="text-xs text-muted-foreground">{entry.date}</time>
          </div>

          <h3 className="mt-1 text-lg font-semibold">{entry.title}</h3>
          <p className="mt-1 text-sm text-muted-foreground">{entry.description}</p>

          {/* Nodes affected badges */}
          {entry.nodesAffected && entry.nodesAffected.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1" aria-label="Nodes affected">
              {entry.nodesAffected.map((n) => (
                <span
                  key={n}
                  className={cn(
                    "rounded-full px-2 py-0.5 text-xs font-medium",
                    AXIS_COLOURS[NODE_AXIS[n] ?? "horizontal"]
                  )}
                  title={`N${n} — ${NODE_LABELS[n] ?? "Unknown"} (${NODE_AXIS[n] ?? "unknown"} axis)`}
                >
                  N{n}
                </span>
              ))}
            </div>
          )}

          {/* Component changes */}
          {entry.componentsAdded && entry.componentsAdded.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1" aria-label="Components added">
              {entry.componentsAdded.map((c) => (
                <span
                  key={c}
                  className="rounded bg-[var(--status-success,#64FFDA)]/10 px-1.5 py-0.5 text-xs text-[var(--status-success,#22C55E)]"
                >
                  +{c}
                </span>
              ))}
            </div>
          )}

          {entry.componentsModified && entry.componentsModified.length > 0 && (
            <div className="mt-1 flex flex-wrap gap-1" aria-label="Components modified">
              {entry.componentsModified.map((c) => (
                <span
                  key={c}
                  className="rounded bg-[var(--color-cobalt)]/10 px-1.5 py-0.5 text-xs text-[var(--color-cobalt)]"
                >
                  ~{c}
                </span>
              ))}
            </div>
          )}

          {entry.componentsDeprecated && entry.componentsDeprecated.length > 0 && (
            <div className="mt-1 flex flex-wrap gap-1" aria-label="Components deprecated">
              {entry.componentsDeprecated.map((c) => (
                <span
                  key={c}
                  className="rounded bg-[var(--status-warning,#F59E0B)]/10 px-1.5 py-0.5 text-xs text-[var(--status-warning,#F59E0B)] line-through"
                >
                  {c}
                </span>
              ))}
            </div>
          )}
        </article>
      ))}
    </div>
  )
}

export type { ChangelogRendererProps as NyuchiChangelogRendererProps }
```

<!-- mzbench: USER MESSAGE END -->
