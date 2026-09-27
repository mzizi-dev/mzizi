<!-- mzbench: SYSTEM MESSAGE BEGIN -->
# Writing one Mzizi component

Mzizi (`.mz`) is a small UI language. One file holds one component. It has one form
per intent: if a construct is not shown here, assume it does not exist. Your file is done
when `mz check --agent <file>` and `mz contract --agent <file>` both exit 0.

## A complete component

```mz
## A filter chip: a label, with an optional remove control.
## Tappable parts never go below the 48px touch floor.
component chip

  enum chip_tone
    neutral  class "bg-muted text-foreground"
    accent   class "bg-accent text-accent-foreground"
    warn     class "bg-amber-100 text-amber-900"
  end

  ## `height` is the pixel height of the `h-N` class (N * 4).
  enum chip_size
    snug   class "h-13 px-3 text-xs"  height 52
    roomy  class "h-15 px-4 text-sm"  height 60
  end

  prop tone: chip_tone = neutral
  prop size: chip_size = snug
  prop label: text
  prop removable: bool = false
  prop on_remove: event(none)

  view
    row
      slot = "chip"
      class = "inline-flex items-center gap-1 rounded-full {tone.class} {size.class}"
      text = label
      when removable
        control
          slot = "chip-remove"
          role = "button"
          class = "min-h-[48px] min-w-[48px]"
          text = "Remove"
          tap = on_remove
        end
      end
    end
  end

  contract
    every chip_size height at_least 48
    chip_size.snug height is 52
    every chip_tone class not_empty
    chip_tone.accent class contains "bg-accent"
    slot is "chip"
    when removable shows control "Remove"
    control "Remove" min_height 48
  end

end component chip
```

## The parts, in order

Write them in this order: doc lines, `use`, `enum`, `prop`, `view`, `contract`.

- **Shape.** `component <name>` opens the file; `end component <name>` closes it and must
  repeat the name. Every other block (`enum`, `view`, `contract`, each element, each
  `when`) closes with a bare `end`.
- **Doc comments.** `##` to end of line, before or inside the component. It is the only
  comment form; `//` and `/* */` are errors.
- **Names.** Everything you name is `snake_case`. A `camelCase` name is a compile error
  (`MZ0101`) whose `exact` fix is the snake form: `onRemove` becomes `on_remove`. Map
  kebab-case the same way: `extra-small` becomes `extra_small`.
- **Capabilities.** `use motion` if the component animates. Nothing else needs `use`, and
  there are no imports: other components are used by bare name.

### Enums are variant tables

Each variant is one line: its name, then `column value` pairs. This replaces `cva`,
`Record<Variant, string>` maps and variant `switch`es; one row holds everything about a
variant. Every variant must have every column (`MZ0303` otherwise). Values are strings
`"..."` or integers. In a size table, `height` is the pixels the class renders: `h-N` or
`size-N` is `N * 4`, so `h-10` pairs with `height 40`.

### Props

`prop <name>: <type>`, optionally `= <default>`, one per line.

| Type            | Default written as          | For                           |
| --------------- | --------------------------- | ----------------------------- |
| an enum name    | a bare variant: `= neutral` | variant / size props          |
| `text`          | a string: `= "Remove"`      | labels, children text, values |
| `bool`          | `= true` or `= false`       | flags such as `disabled`      |
| `int`           | an integer: `= 3`           | counts                        |
| `event(none)`   | none                        | `onClick`-style callbacks     |
| `event(<type>)` | none                        | callbacks carrying a value    |

The compiler does not check type names or the names inside `{...}`, so spell them exactly.

### The view

`view ... end` holds exactly one tree. An element is a bare word on its own line, closed by
`end`. Element words are free; use `row` for layout containers and `control` for anything
tappable. Inside an element, every line is either:

- an attribute, `name = value`, where the `=` is required; or
- a child element, a `when` block, or `nothing`.

Attribute values are a string, an integer, `true`/`false`, a prop name (`text = label`),
or a dotted cell (`role = variant.announce`). A string may interpolate cells and props with
`{...}`: `class = "base {tone.class}"`. Interpolation inside a string is the only way to
build a string; there is no `+`, and no `{...}` outside a string.

| React                              | Mzizi                                      |
| ---------------------------------- | ------------------------------------------ |
| `className={cn(base, v[variant])}` | `class = "base {variant.class}"`           |
| `data-slot="x"`                    | `slot = "x"`                               |
| `role="status"`                    | `role = "status"`                          |
| `{label}` / `children` as text     | `text = label`                             |
| `onClick={onTap}`                  | `tap = on_tap`                             |
| `onChange`                         | `change = on_change`                       |
| `disabled={disabled}`              | `disabled = disabled`                      |
| `{open && <X/>}`                   | `when open` ... `end`                      |
| `a ? <X/> : <Y/>`                  | `when a` ... `end`, `when not a` ... `end` |
| `{v === "x" && <X/>}`              | `when v is x` ... `end`                    |
| `return null`                      | `nothing`                                  |

Drop `...props` spreading and `className` pass-through: there is no equivalent.

### The contract

One assertion per line, `<subject> <predicate>`, no colon. It is checked against this file's
own enums, view attributes and prop defaults.

| Subject                     | Means                                     |
| --------------------------- | ----------------------------------------- |
| `every <enum> <column>`     | that column in every variant              |
| `<enum>.<variant> <column>` | one cell                                  |
| `<attribute>`               | the outermost view element's attribute    |
| `<prop>`                    | the prop's default (a prop with one only) |
| `<element> "<text>"`        | the element carrying that text            |
| `when <name>`               | the `when` branch that mentions that name |

| Predicate               | Holds when                                      |
| ----------------------- | ----------------------------------------------- |
| `is <value>`            | exactly equal, as written                       |
| `contains "<s>"`        | the string contains `s`                         |
| `not_empty`             | the string is not blank                         |
| `at_least <n>`          | the number is `n` or more                       |
| `in "<a>" "<b>"`        | the string is one of those                      |
| `uses "--token"`        | the string reads `var(--token...)`              |
| `uses <component>`      | (no subject) the view contains that component   |
| `shows <element> "<t>"` | (after `when <name>`) that branch renders it    |
| `min_height <n>`        | the element's `height` or `min-h-[Npx]` is `n`+ |

Assert what the source guarantees: every interactive size clears 48
(`every <enum> height at_least 48`), the `slot`, and the key cells.

## One form per intent: the traps

- **`=` in attributes.** `class "flex"` without `=` is read as a child element. The errors
  then appear later, at `contract` ("cannot start a view line") and at the file's last line
  (unclosed blocks). When you see those, look for a missing `=` first.
- **`is` in contracts.** `chip_size.snug height 52` is `MZ0602`; write `height is 52`.
- **`is` compares as written.** `height is "52"` fails against `height 52`.
- **No `else`, no `match`/`case`.** Both are errors in a view. Write two `when` blocks.
- **No `if`.** `if open` is not an error: it silently becomes an element named `if`. Use
  `when`.
- **Event attribute names.** Use `tap` and `change`. `on_click = on_tap` also compiles,
  so the compiler will not catch the wrong name.
- **No assertion on a prop without a default.** `label is "x"` is `MZ0605` (unevaluable),
  and unevaluable counts as failure.
- **Closers.** Only `component` echoes: `end component chip`. Never `end enum`, `end view`
  or `end row`; write a bare `end`.

## Reading diagnostics

`--agent` prints NDJSON: one diagnostic per line, then one summary line. This file is wrong
on purpose.

```mz
## WRONG ON PURPOSE: three mistakes.
component tag

  enum tag_size
    snug  class "h-13 px-3"  height 52
  end

  prop onPick: event(none)
  prop size: tag_size = snug

  view
    row
      slot = "tag"
      class = "rounded-full {size.class}"
      tap = on_pick
    end
  end

  contract
    slot is "tag"
    tag_size.snug height 52
  end

end
```

`mz check --agent tag.mz` exits 1 and prints:

```text
{"code":"MZ0101","severity":"error","file":"tag.mz","span":[8,8,8,14],"say":"`onPick` is not snake_case — Mzizi names are snake_case, so write `on_pick`","fix":{"span":[8,8,8,14],"replace":"on_pick","confidence":"exact"}}
{"code":"MZ0602","severity":"error","file":"tag.mz","span":[21,26,21,28],"say":"a contract clause needs a predicate (is / in / contains / not_empty / at_least / uses / min_height / shows), found `52`","fix":{"span":[21,26,21,26],"replace":"is ","confidence":"exact"}}
{"code":"MZ0208","severity":"error","file":"tag.mz","span":[24,1,24,4],"say":"top-level blocks close with their name: write `end component tag`","fix":{"span":[24,1,24,4],"replace":"end component tag","confidence":"exact"}}
{"summary":true,"errors":3,"warnings":0,"exact_fixable":3,"ms":0}
```

Each line has `code`, `severity`, `span` `[line, col, end_line, end_col]`, and `say`, which
quotes the problem. `fix` is optional: `replace` goes at `span`. Apply an `exact` fix as
given; check a `guess` fix first. For a closer error, rewrite the whole `end` line as `say`
spells it. Warnings (`MZ0501`: no contract) do not fail. Errors are independent, so fix them
all, then run again. `mz contract` runs only on a file that checks clean; `MZ0603` means an
assertion is false, `MZ0605` means it could not be evaluated.

<!-- mzbench: SYSTEM MESSAGE END -->

<!-- mzbench: USER MESSAGE BEGIN -->
Port this React component to Mzizi. Keep the same variants, sizes, defaults and behaviour. The result must be a single Mzizi source file (.mz). Name the variant enums exactly `node_accent`, each with a `class` column. Reply with exactly one fenced code block containing the complete file and nothing else.

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
