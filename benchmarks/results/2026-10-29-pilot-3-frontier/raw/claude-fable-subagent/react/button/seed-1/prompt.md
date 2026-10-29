<!-- mzbench: SYSTEM MESSAGE BEGIN -->
# Writing one registry component in TypeScript with React

You are writing one registry component as a single `.tsx` file, in TypeScript with
**React 19.2** and **`class-variance-authority` 0.7.1**. Your file is checked with
`tsc --noEmit` under `strict: true`, as one file of a small project. Type errors fail it.

## File shape

A file starts with a comment naming the component and its tier, then its imports, then
one `cva(…)` call per variant group, then the props type and the component function, and
ends with a named `export { … }` of the component and its variant calls. A component is a
function taking one props object and returning JSX.

## What is available

These imports, and nothing else:

- `import * as React from "react"`
- `import { cva, type VariantProps } from "class-variance-authority"`
- `import { cn } from "@/lib/utils"`, which joins class strings and resolves Tailwind
  conflicts in favour of the later class.

No other package is installed (no `radix-ui`, no `@radix-ui/*`): importing one is
`TS2307`. The spec asks for no `asChild`.

**Variant groups are `cva` calls.** The first argument is the base classes. `variants`
maps each group's key to an object of variant name → class string, and `defaultVariants`
gives each group's default. Write every class as one string literal, exactly as the spec
gives it: a variable, a concatenation or a `${…}` template is not read as that variant's
classes. Calling the result with the chosen variants (`avatarVariants({ size })`) returns
the classes. `VariantProps<typeof avatarVariants>` is the props type of its groups, so
`size?: "default" | "sm" | "lg"`, each also accepting `null`. Every variant group the spec
names is a key of a `cva` call, even one whose classes are all empty strings (`""`).

**Props.** One object type per component: `React.ComponentProps<"<tag>">` (every attribute
of the root element, including `className` and `children`), `&` the variant props, `&` the
component's own fields. Destructure it with defaults for the variant props, and pass the
rest through with `{...props}` on the root element. An optional field is `x?: T`, and
an event callback is a function type such as `(event: React.MouseEvent) => void`. Write
components as `function` declarations, not `React.FC`.

```tsx
import * as React from "react"

import { cn } from "@/lib/utils"

type TagProps = React.ComponentProps<"a"> & {
  selected?: boolean
  onSelect?: (event: React.MouseEvent<HTMLAnchorElement>) => void
}

function Tag({ className, selected = false, onSelect, children, ...props }: TagProps) {
  return (
    <a
      data-slot="tag"
      data-selected={selected ? "true" : undefined}
      className={cn("inline-flex rounded-md px-2", className)}
      onClick={onSelect}
      {...props}
    >
      {children}
    </a>
  )
}

export { Tag }
```

**The view, JSX.**

- An element is a tag: `<div>…</div>`, or self-closing `<img … />`.
- An attribute is `name={expression}` or `name="text"`. `class` is `className`; `data-*`
  and `aria-*` are written as they are.
- An attribute whose value is `undefined` is not rendered. A valueless attribute such as
  `data-loading` is written `data-loading=""`, and a boolean one such as `disabled` as
  `disabled={disabled}`.
- DOM names are React's: `htmlFor` for `for`, `tabIndex`, and camelCase events
  (`onClick`, `onChange`). Void elements (`img`, `input`, `br`) self-close.
- `{children}` renders nested content.
- `{open && <X />}` renders only when `open` is true, and `{a ? <X /> : <Y />}` renders
  one or the other. Test a list's length with `> 0`: `{xs.length && <X />}` renders a `0`
  when the list is empty.
- `{items.map((item) => <X key={item.id} … />)}` renders a list. Every element a `map`
  returns needs a `key` that is unique among its siblings.
- An optional field (`note?: string`) is `undefined` when absent: test it before use, or
  `strict` reports `TS18048`.

```tsx
// A release list: an object type, a list of them, an optional field, and nested lists.
import * as React from "react"

type Release = {
  version: string
  note?: string
  tags?: string[]
}

function Releases({ items }: { items: Release[] }) {
  return (
    <div data-slot="releases">
      {items.map((item) => (
        <article key={item.version}>
          {item.version}
          {item.note ? <div>{item.note}</div> : null}
          {item.tags && item.tags.length > 0
            ? item.tags.map((tag) => <span key={tag}>#{tag}</span>)
            : null}
        </article>
      ))}
    </div>
  )
}

export { Releases }
```

| Spec says                       | React                                                 |
| ------------------------------- | ----------------------------------------------------- |
| `data-slot="x"`                 | `data-slot="x"`                                       |
| `data-size` is the size in use  | `data-size={size}`                                    |
| its classes, then extra classes | `className={cn(avatarVariants({ size }), className)}` |
| other attributes pass through   | `{...props}` on the root                              |
| its children                    | `{children}`                                          |

## Naming rules

- Components and types are `PascalCase`; props and variables are `camelCase`.
- The task names each variant group as `<call>.<key>`, such as `avatarVariants.size`:
  write a `cva` call bound to exactly `avatarVariants`, with a `size` key under
  `variants`. Group keys are one lower-case word.
- Variant names are the spec's own, quoted when they are not identifiers (`"icon-sm"`).
  Keep every variant, class string and default the spec gives, character for character.
  Do not invent or drop variants.
- `data-slot` on the root names the component in kebab-case; sub-parts use
  `<name>-<part>`. The root also carries `data-portal` when the spec gives one, and
  `data-variant` / `data-size` carry the variant in use.
- One self-contained file. Classes use the spec's semantic tokens (`bg-primary`,
  `text-muted-foreground`), exactly as the spec writes them.

## Reading the checker's output

The check prints tsc's diagnostics, one per line, and nothing else. This file is wrong on
purpose:

```tsx
// WRONG ON PURPOSE: a default that is not a variant.
import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

const tagVariants = cva("rounded-full", {
  variants: {
    size: { snug: "h-13 px-3", roomy: "h-15 px-4" },
  },
  defaultVariants: { size: "tight" },
})

function Tag({ size }: VariantProps<typeof tagVariants>) {
  return <span data-slot="tag" className={tagVariants({ size })} />
}

export { Tag }
```

`check.sh tag.tsx` exits 1 and prints:

```text
tag.tsx(9,22): error TS2322: Type '"tight"' is not assignable to type '"snug" | "roomy" | null | undefined'.
```

Each line is `file(line,column): error TS<code>: <message>`. The message names the type
that was expected. Here `"tight"` is not one of the `size` variants, so the default must
be `"snug"` or `"roomy"`, whichever the spec says. Fix every error, then check again: tsc
reports all of them at once, and a later one can follow from an earlier one. `TS2307` is
an import that is not available, `TS2304` a name that does not exist, `TS2322` a value of
the wrong type, `TS2339` a property that does not exist on that type, and `TS7006` a
parameter that needs a type annotation under `strict`.

A second file, wrong on purpose in the ways `strict` catches most often:

```tsx
// WRONG ON PURPOSE: four mistakes strict mode catches.
import * as React from "react"
import { Slot } from "radix-ui"

type Item = { id: string; note?: string }

function List({ items, onPick }) {
  return <ul>{items.map((item: Item) => <li key={item.id}>{item.note.trim()}</li>)}</ul>
}

export { List }
```

`check.sh list.tsx` exits 1 and prints:

```text
list.tsx(3,22): error TS2307: Cannot find module 'radix-ui' or its corresponding type declarations.
list.tsx(7,17): error TS7031: Binding element 'items' implicitly has an 'any' type.
list.tsx(7,24): error TS7031: Binding element 'onPick' implicitly has an 'any' type.
list.tsx(8,60): error TS18048: 'item.note' is possibly 'undefined'.
```

Delete the import that is not available. Give the props object a type, as every example
here does, so no binding is an implicit `any`. Test an optional field before using it:
`item.note ? item.note.trim() : ""`.

## Worked example: avatar

The registry's avatar (`n2-primitives/avatar`), which is not one of the tasks. The task
would name its group `avatarVariants.size`:

```tsx
// AVATAR — N2 primitive, React.
// A person or entity image, with initials as the fallback when no image is given.
import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const avatarVariants = cva(
  "group/avatar relative flex shrink-0 overflow-hidden rounded-full bg-muted",
  {
    variants: {
      size: {
        default: "size-10",
        sm: "size-8",
        lg: "size-14",
      },
    },
    defaultVariants: {
      size: "default",
    },
  }
)

const fallbackVariants = cva(
  "flex size-full items-center justify-center rounded-full bg-muted text-muted-foreground",
  {
    variants: {
      size: {
        default: "text-sm",
        sm: "text-xs",
        lg: "text-base",
      },
    },
    defaultVariants: {
      size: "default",
    },
  }
)

type AvatarProps = React.ComponentProps<"figure"> &
  VariantProps<typeof avatarVariants> & {
    image?: string
    initials: string
    alt: string
  }

function Avatar({ className, size = "default", image, initials, alt, ...props }: AvatarProps) {
  return (
    <figure
      data-slot="avatar"
      data-portal="https://mzizi.dev/components/avatar"
      data-size={size}
      className={cn(avatarVariants({ size }), className)}
      {...props}
    >
      {image ? (
        <img
          data-slot="avatar-image"
          src={image}
          alt={alt}
          className="aspect-square size-full rounded-full object-cover"
        />
      ) : (
        <span data-slot="avatar-fallback" className={fallbackVariants({ size })}>
          {initials}
        </span>
      )}
    </figure>
  )
}

export { Avatar, avatarVariants }
```

<!-- mzbench: SYSTEM MESSAGE END -->

<!-- mzbench: USER MESSAGE BEGIN -->
Implement the component specified below in TypeScript with React. Keep exactly the variants, classes, defaults, slots and elements it specifies. The result must be a single TypeScript React source file (.tsx). Name the variant enums exactly `buttonVariants.variant`, `buttonVariants.size`, each a variant key of a `cva` call whose values are its Tailwind classes. Reply with exactly one fenced code block containing the complete file and nothing else.

````markdown
# Button

A pill-shaped button: a single `button` element. It is a Layer 2 primitive, so it
styles itself from semantic colour tokens and does not wire into any observability layer.

Its touch target is 56px by default and never less than 48px.

## Variant groups

Variant names are given in kebab case. Write them in your language's own convention for
enum variants.

### `variant`: the colour treatment. Default: `default`

| Variant       | Classes                                                                                                                                                                                                                       |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`     | `bg-primary text-primary-foreground hover:bg-primary/80`                                                                                                                                                                      |
| `outline`     | `border-border bg-input/30 hover:bg-input/50 hover:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground`                                                                                                      |
| `secondary`   | `bg-secondary text-secondary-foreground hover:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground`                                                                                             |
| `ghost`       | `hover:bg-muted hover:text-foreground dark:hover:bg-muted/50 aria-expanded:bg-muted aria-expanded:text-foreground`                                                                                                            |
| `destructive` | `bg-destructive/10 hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/20 text-destructive focus-visible:border-destructive/40 dark:hover:bg-destructive/30` |
| `link`        | `text-primary underline-offset-4 hover:underline`                                                                                                                                                                             |

### `size`: height and padding. Default: `default`

| Variant   | Classes                                                                               | Renders      |
| --------- | ------------------------------------------------------------------------------------- | ------------ |
| `default` | `h-14 gap-2 px-5 has-data-[icon=inline-end]:pr-4 has-data-[icon=inline-start]:pl-4`   | 56px         |
| `sm`      | `h-12 gap-1.5 px-4 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3` | 48px         |
| `lg`      | `h-14 gap-2 px-6 has-data-[icon=inline-end]:pr-5 has-data-[icon=inline-start]:pl-5`   | 56px         |
| `icon`    | `size-14`                                                                             | 56px, square |
| `icon-sm` | `size-12`                                                                             | 48px, square |

`sm` is the minimum size allowed. `lg` is as tall as `default`, with wider padding.

## Base classes

Every button has these classes, then its `variant` classes, then its `size` classes, then
any extra classes the caller passes:

```text
focus-visible:border-ring focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive dark:aria-invalid:border-destructive/50 rounded-full border border-transparent bg-clip-padding text-sm font-medium focus-visible:ring-[3px] aria-invalid:ring-[3px] [&_svg:not([class*='size-'])]:size-4 inline-flex items-center justify-center whitespace-nowrap transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none shrink-0 [&_svg]:shrink-0 outline-none group/button select-none
```

## Rendered element

One `button` element with these attributes:

| Attribute      | Value                                 |
| -------------- | ------------------------------------- |
| `data-slot`    | `button`                              |
| `data-portal`  | `https://mzizi.dev/components/button` |
| `data-variant` | the `variant` in use                  |
| `data-size`    | the `size` in use                     |
| `class`        | the classes above, in that order      |

Its content is the button's children. Any other attribute the caller gives the button (a
`type`, `disabled`, an `aria-label`, an event handler) is passed through to the element.
````

<!-- mzbench: USER MESSAGE END -->
