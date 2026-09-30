# Changelog renderer

Renders a changelog as a vertical timeline, one entry per release. It is an N10
documentation component. Its entries mirror a database table, `public.changelog`, whose
column for the affected nodes is `nodes_affected` (an integer array).

## Input

A list of entries, and optional extra classes for the outer element. Each entry has:

| Field                 | Type             | Required |
| --------------------- | ---------------- | -------- |
| version               | text             | yes      |
| title                 | text             | yes      |
| description           | text             | yes      |
| date                  | text             | yes      |
| nodes affected        | list of integers | no       |
| components added      | list of text     | no       |
| components modified   | list of text     | no       |
| components deprecated | list of text     | no       |

## Nodes

The ecosystem has ten nodes. Each has a label and an axis:

| Node | Label         | Axis         |
| ---- | ------------- | ------------ |
| 1    | Tokens        | `vertical`   |
| 2    | Primitives    | `horizontal` |
| 3    | Brand         | `horizontal` |
| 4    | Safety        | `vertical`   |
| 5    | Resilience    | `vertical`   |
| 6    | Pages         | `horizontal` |
| 7    | Shell         | `horizontal` |
| 8    | Assurance     | `depth`      |
| 9    | Fundi         | `outlier`    |
| 10   | Documentation | `outlier`    |

## Variant groups

### The node accent: a node badge's colour, chosen by its node's axis. Default: `horizontal`

A node with no axis in the table above takes the default.

| Variant      | Classes                                                        |
| ------------ | -------------------------------------------------------------- |
| `horizontal` | `bg-[var(--color-cobalt)]/10 text-[var(--color-cobalt)]`       |
| `vertical`   | `bg-[var(--color-tanzanite)]/10 text-[var(--color-tanzanite)]` |
| `depth`      | `bg-[var(--color-malachite)]/10 text-[var(--color-malachite)]` |
| `outlier`    | `bg-[var(--color-gold)]/10 text-[var(--color-gold)]`           |

## Rendered elements

The outer element is a `div` with `data-slot="nyuchi-changelog-renderer"`,
`data-portal="https://mzizi.dev/components/nyuchi-changelog-renderer"`, `role="feed"`,
`aria-label="Changelog"`, and the classes `flex flex-col gap-8` followed by any extra classes.

Inside it, each entry, in order, is an `article` with the classes
`relative border-l-2 border-border pl-6`, containing:

1. A timeline dot: a `div` with `absolute top-1 -left-[5px] size-2 rounded-full bg-primary`.
2. A `div` with `flex flex-wrap items-baseline gap-3` holding the version, a `span` with
   `font-mono text-sm font-bold text-primary`, and the date, a `time` element with
   `text-xs text-muted-foreground`.
3. The title, an `h3` with `mt-1 text-lg font-semibold`.
4. The description, a `p` with `mt-1 text-sm text-muted-foreground`.
5. Only when the entry has at least one affected node: a `div` with
   `mt-2 flex flex-wrap gap-1` and `aria-label="Nodes affected"`, holding one `span` per
   node. Each has the classes `rounded-full px-2 py-0.5 text-xs font-medium` followed by its
   node accent's classes, a `title` of `N<n> — <label> (<axis> axis)` (with `Unknown` for a
   node with no label, and `unknown` for one with no axis), and the text `N<n>`.
6. Only when the entry has at least one added component: a `div` with
   `mt-2 flex flex-wrap gap-1` and `aria-label="Components added"`, holding one `span` per
   component with the classes
   `rounded bg-[var(--status-success,#64FFDA)]/10 px-1.5 py-0.5 text-xs text-[var(--status-success,#22C55E)]`
   and the text `+<name>`.
7. Only when the entry has at least one modified component: a `div` with
   `mt-1 flex flex-wrap gap-1` and `aria-label="Components modified"`, holding one `span`
   per component with the classes
   `rounded bg-[var(--color-cobalt)]/10 px-1.5 py-0.5 text-xs text-[var(--color-cobalt)]`
   and the text `~<name>`.
8. Only when the entry has at least one deprecated component: a `div` with
   `mt-1 flex flex-wrap gap-1` and `aria-label="Components deprecated"`, holding one `span`
   per component with the classes
   `rounded bg-[var(--status-warning,#F59E0B)]/10 px-1.5 py-0.5 text-xs text-[var(--status-warning,#F59E0B)] line-through`
   and the text `<name>`.
