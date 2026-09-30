# Writing one Mzizi component

Mzizi (`.mz`) is a small UI language. One file holds one component. It has one form per
intent: if a construct is not shown here, assume it does not exist. Your file is checked
with `mz check --agent`; errors fail it, warnings do not.

## File shape

Write the parts in this order: doc lines, `use`, `enum` and `record`, `prop`, `view`,
`contract`.

- `component <name>` opens the file and `end component <name>` closes it, repeating the
  name. Every other block (`enum`, `record`, `view`, `contract`, each element, each
  `when`, each `for each`) closes with a bare `end`. Never `end enum` or `end row`.
- `##` to end of line is the only comment. `//` and `/* */` are errors.
- `use motion` if the component animates. Nothing else needs `use`, and there are no
  imports.

## What is available

**Enums are variant tables.** Each variant is one line: its name, then `column value`
pairs, and every variant has every column (`MZ0303` otherwise). This replaces `cva`,
class maps and variant `switch`es. Values are strings `"..."` or integers. Do not write a
`height` column: a variant's height is what its class renders (`h-N` or `size-N` is
`N * 4` pixels), and a written `height` that disagrees is `MZ0313`.

**Records** group fields, one `field <name>: <type>` per line, closed by `end`.

**Props** are `prop <name>: <type>`, optionally `= <default>`, one per line.

| Type             | Default written as         | For                           |
| ---------------- | -------------------------- | ----------------------------- |
| an enum name     | a bare variant: `= sm`     | variant / size props          |
| `text`           | a string: `= "Remove"`     | labels, children text, values |
| `bool`           | `= true` or `= false`      | flags such as `disabled`      |
| `int`            | an integer: `= 3`          | counts                        |
| `event(none)`    | none                       | click-style callbacks         |
| `event(<type>)`  | none                       | callbacks carrying a value    |
| `list(<type>)`   | none: omitted means empty  | lists                         |
| `option(<type>)` | none: omitted means `none` | an optional scalar            |
| a record name    | none                       | an object                     |

An optional list is `list(T)`, never `option(list(T))`.

**The view.** `view ... end` holds exactly one element tree. An element is a bare word on
its own line, closed by `end`; use `row` for layout and `control` for anything tappable.
Inside an element, each line is an attribute `name = value` (the `=` is required), a child
element, a `when` block, a `for each`, or `nothing`. A value is a string, an integer,
`true`/`false`, a prop name, or a dotted path (`size.class`, `item.title`). A string
interpolates with `{...}`: `class = "base {size.class}"`. That is the only way to build a
string: there is no `+`.

| Spec says                       | Mzizi                                            |
| ------------------------------- | ------------------------------------------------ |
| `data-slot="x"`                 | `slot = "x"`                                     |
| `data-portal="…"`               | `portal = "…"`                                   |
| `data-size` is the size in use  | `size = size`                                    |
| its children as text            | `text = label`                                   |
| a click handler                 | `tap = on_tap` (`change = on_change`)            |
| shown only when a flag is set   | `when open` ... `end`                            |
| one thing or another            | `when a` ... `else` ... `end`                    |
| for one variant only            | `when v is x` ... `end`                          |
| once per item of a list         | `for each x in xs`, then `key = x.id`, ... `end` |
| an optional value, when present | `when x is none`, `nothing`, `else` ... `end`    |

Inside the `else` of `when x is none`, an option is its value; anywhere else, using it is
an error. For a list, `is none` means empty. A list or a whole record is never a value:
iterate the list, and name the field.

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

Drop attribute spreading, `asChild` and `className` pass-through: none exists. A
spread is `MZ0106` and an `as_child` prop `MZ0312`.

**The contract** is one assertion per line, `<subject> <predicate>`, checked by
`mz contract` against the file's own enums, attributes and defaults. Subjects:
`every <enum> <column>`, `<enum>.<variant> <column>`, a root attribute (`slot`), a prop
with a default, or `<element> "<text>"`. Predicates: `is <value>`, `contains "<s>"`,
`not_empty`, `at_least <n>`, `in "<a>" "<b>"`, `min_height <n>`. Assert what the spec
guarantees: the `slot`, key cells, and `every <size enum> height at_least 48` for anything
tappable.

## Naming rules

- Everything you name is `snake_case`. `onRemove` is `MZ0101`, with `on_remove` as its
  exact fix. Kebab-case maps the same way: `icon-sm` is `icon_sm`.
- Name each enum exactly as the task says, and keep every variant, class string and
  default the spec gives, character for character.
- The root element carries the component's `slot`; sub-parts use `<name>-<part>`.
- Event attributes are `tap` and `change`. `on_click = on_tap` also compiles, so the
  checker will not catch that name.
- `is` is required in a contract (`height is 52`, not `height 52`), and compares as
  written (`is "52"` fails against `52`). There is no `if` (`MZ0407`, fix: `when`) and no
  `some`. There is no `match` either, and a `match` line is not always an error, so write
  `when v is x` blocks.

## Reading the checker's output

`--agent` prints NDJSON: one diagnostic per line, then one summary line. This file is
wrong on purpose:

```mz
## WRONG ON PURPOSE: three mistakes.
component tag

  enum tag_size
    snug  class "h-13 px-3"
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

`span` is `[line, col, end_line, end_col]` and `say` quotes the problem. A `fix` puts
`replace` at its `span`: apply an `exact` fix as given, and check a `guess` first. Errors
are independent, so fix them all, then check again. `mz contract` runs only on a file that
checks clean: `MZ0603` means an assertion is false, and `MZ0605` that it could not be
evaluated, which counts as failing (an assertion on a prop with no default is one).

The errors seen most often, and what they mean:

- `MZ0406`: an attribute without `=` (`class "flex"`). The fix inserts it.
- `MZ0408`: an attribute directly inside `view`. Put it inside the root element.
- `MZ0204` / `MZ0206` / `MZ0208`: a block left open or closed wrongly. Rewrite the whole
  `end` line as `say` spells it.
- `MZ0701`: an unknown type. `say` lists the types there are.
- `MZ0707`: a name that is not a prop, loop binding or variant in this file.

## Worked example: avatar

The registry's avatar (`n2-primitives/avatar`), which is not one of the tasks:

```mz
## A person or entity image, with initials as the fallback when no image loads.
component avatar

  enum avatar_size
    default  class "size-10"  text_class "text-sm"
    sm       class "size-8"   text_class "text-xs"
    lg       class "size-14"  text_class "text-base"
  end

  prop size: avatar_size = default
  prop image: option(text)
  prop initials: text
  prop alt: text

  view
    figure
      slot = "avatar"
      portal = "https://mzizi.dev/components/avatar"
      size = size
      class = "group/avatar relative flex shrink-0 overflow-hidden rounded-full bg-muted {size.class}"
      when image is none
        row
          slot = "avatar-fallback"
          class = "flex size-full items-center justify-center rounded-full bg-muted text-muted-foreground {size.text_class}"
          text = initials
        end
      else
        picture
          slot = "avatar-image"
          source = image
          alt = alt
          class = "aspect-square size-full rounded-full object-cover"
        end
      end
    end
  end

  contract
    every avatar_size text_class not_empty
    avatar_size.sm height is 32
    slot is "avatar"
  end

end component avatar
```
