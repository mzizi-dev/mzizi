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

Write them in this order: doc lines, `use`, `enum` and `record`, `prop`, `view`, `contract`.

- **Shape.** `component <name>` opens the file; `end component <name>` closes it and must
  repeat the name. Every other block (`enum`, `record`, `view`, `contract`, each element,
  each `when`, each `for each`) closes with a bare `end`.
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
`size-N` is `N * 4`, so `h-10` pairs with `height 40`. A column may hold another enum's
variant (`accent gold`); `{node.accent.class}` then reads through both tables.

### Records

A record is a named group of fields, one `field <name>: <type>` per line, closed by `end`.
The example under "The view" declares one.

### Props

`prop <name>: <type>`, optionally `= <default>`, one per line.

| Type             | Default written as          | For                           |
| ---------------- | --------------------------- | ----------------------------- |
| an enum name     | a bare variant: `= neutral` | variant / size props          |
| `text`           | a string: `= "Remove"`      | labels, children text, values |
| `bool`           | `= true` or `= false`       | flags such as `disabled`      |
| `int`            | an integer: `= 3`           | counts                        |
| `event(none)`    | none                        | `onClick`-style callbacks     |
| `event(<type>)`  | none                        | callbacks carrying a value    |
| `list(<type>)`   | none: omitted means empty   | arrays, `T[]`                 |
| `option(<type>)` | none: omitted means `none`  | an optional scalar, `x?: T`   |
| a record name    | none                        | an object prop                |

An optional array is `list(T)`, never `option(list(T))`: the empty list is its absence. An
optional callback is plain `event(...)`. Every type and name is checked; a misspelling is
an error whose fix is the nearest name.

### The view

`view ... end` holds exactly one tree. An element is a bare word on its own line, closed by
`end`. Element words are free; use `row` for layout containers and `control` for anything
tappable. Inside an element, every line is either:

- an attribute, `name = value`, where the `=` is required; or
- a child element, a `when` block, or `nothing`.

Attribute values are a string, an integer, `true`/`false`, a prop name (`text = label`),
or a dotted path (`role = variant.announce`, `text = entry.title`). A string may interpolate
them with `{...}`: `class = "base {tone.class}"`. Interpolation inside a string is the only
way to build a string; there is no `+`, and no `{...}` outside a string. A list or a whole
record is never a value; iterate the list, name the field.

Lists render with `for each`, whose first line is its `key`, a path from the item.
`when x is none` ... `else` ... `end` tests an option or a list. Inside the `else` an option
is its value; anywhere else, using it is an error. For a list, `is none` means empty.

```mz
## A release list: a record, a list of them, an option, and nested `for each`.
component releases

  record release
    field version: text
    field note: option(text)
    field tags: list(text)
  end

  prop items: list(release)

  view
    row
      slot = "releases"
      for each item in items
        key = item.version
        article
          text = item.version
          when item.note is none
            nothing
          else
            row
              text = item.note
            end
          end
          for each tag in item.tags
            key = tag
            chip
              text = "#{tag}"
            end
          end
        end
      end
    end
  end

  contract
    slot is "releases"
  end

end component releases
```

| React                              | Mzizi                                         |
| ---------------------------------- | --------------------------------------------- |
| `className={cn(base, v[variant])}` | `class = "base {variant.class}"`              |
| `data-slot="x"`                    | `slot = "x"`                                  |
| `role="status"`                    | `role = "status"`                             |
| `{label}` / `children` as text     | `text = label`                                |
| `onClick={onTap}`                  | `tap = on_tap`                                |
| `onChange`                         | `change = on_change`                          |
| `disabled={disabled}`              | `disabled = disabled`                         |
| `{open && <X/>}`                   | `when open` ... `end`                         |
| `a ? <X/> : <Y/>`                  | `when a` ... `else` ... `end`                 |
| `{v === "x" && <X/>}`              | `when v is x` ... `end`                       |
| `{xs.map(x => <X key={x.id}/>)}`   | `for each x in xs`, `key = x.id`, ... `end`   |
| `{x && x.length > 0 && <X/>}`      | `when x is none`, `nothing`, `else` ... `end` |
| `return null`                      | `nothing`                                     |

Drop `...props` spreading, `asChild` and `className` pass-through: none has an equivalent.
A spread is an error (`MZ0106`) and an `as_child` prop a warning (`MZ0312`); delete the
line, and keep only what the `as_child = false` branch renders.

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

- **`=` in attributes.** `class "flex"` without `=` is `MZ0406`; its fix inserts the `=`.
- **Attributes go on an element.** Nothing but one element tree sits directly inside
  `view`; `slot = "x"` there is `MZ0408`. Put it inside the root element.
- **`is` in contracts.** `chip_size.snug height 52` is `MZ0602`; write `height is 52`.
- **`is` compares as written.** `height is "52"` fails against `height 52`.
- **No `match`/`case`.** An error in a view. Write `when v is x` blocks.
- **No `some`, no `when x` on an option.** Presence is `when x is none` / `else`.
- **No `if`.** `if open` is `MZ0407`, and its `exact` fix is `when open`.
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
