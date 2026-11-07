# RFC-0017 — Rendering a component: the element tree, typed props, and a test oracle

**Status:** draft for review. Waves 0, 1 and 1b are built (§9); every other wave is design.
**Author:** the machine author (Claude), for the owner, on issue
[#153](https://github.com/mzizi-dev/mzizi/issues/153) (stream N2 UI core).
**Scope:** the first six of #153's ten UI language gaps, in the order the first static primitives
meet them: the element tree (U1), children and slots (U2), pass-through attributes (U3), explicit raw
output (U5), a render as a test oracle (U7), and typed props (U9, confirmed in Wave 0).
Event handlers, local state, callbacks that return content and the browser attach point (#153 gaps
4, 5, 6 and 9) are out of scope and have rows in the tracker.

**Nothing here is a claim that a component renders in a browser or on a server, or lowers to Rust.**
Wave 0 turns a component's `view` into a markup string in a library function and in tests. There is
no `mz` command for it and no Workers, browser or WebAssembly target. Mzizi stays labelled Phase 0
until the owner changes `CHARTER.md`.

## 0. What is already there

A `view` is parsed into `ast::Element`s: a word, `name = value` attributes (literals, interpolated
strings `"…{state.label}"`, or a path), `text = …`, children, `when` / `else`, `nothing`. The nine
primitives in `primitives/` are written in it, and `mz contract` checks them **statically** against
their own declarations (RFC-0006 §5: "not a rendered DOM"). The gap is that nothing turns a view
into output, so no clause can be checked against output and no component can be installed from it.

## 1. The element tree is a value (U1)

`render::Node` is `Element { tag, attrs, children } | Text`. `render::tree(component, props)`
evaluates a view against prop values and returns `Vec<Node>`; `render::to_html` serialises it.

Decisions, each with the failure it removes:

1. **The view vocabulary stays semantic; one closed table maps it to markup.** `strip`, `notice`,
   `row`, `surface`, `rule`, `field` are `div`; `mark`, `busy` are `span`; `caption` is `label`;
   `figure`, `picture` (`img`), `button`. A word not in the table is an error, never a guessed
   `div` (a model that invents an element word gets a refusal, not wrong markup). The table grows
   one word at a time, with a test, as components need it.
2. **Attribute names.** `slot` and `portal` are `data-slot` and `data-portal`; `source` is `src`;
   `class`, `role`, `id`, `href`, `src`, `alt`, `for`, `type`, `name`, `value`, `placeholder`,
   `title`, `lang`, `dir`, `tabindex`, `width`, `height`, `target`, `rel` pass through; `aria_x` is
   `aria-x`; any other word is `data-<word>` with `_` as `-`. `tap` and `change` are event wires,
   not markup, and are left out (U4). An attribute whose value is an absent `option` is omitted.
3. **`text` is the element's first child**, escaped; child elements follow it.
4. **Escaping is total and happens once, at serialisation.** Text nodes and attribute values both
   escape `&`, `<`, `>`, `"` and `'` as decimal references (`&#38;` …), the form the registry's
   `html` module uses. The tree holds unescaped values, so interpolation cannot double-escape, and
   there is no node that serialises unescaped (§5 adds the one explicit exception, later).
5. **Void elements** (`img`) have no closing tag and no children.
6. **Refusals.** `for each`, composing another component, and an event prop given a value return
   `RenderError::Unsupported` naming the tracker row. Partial output is worse than none.

## 2. Typed props and defaults (U9)

Props are supplied as `render::Value` (`Bool`, `Int`, `Text`, `Variant`, `None`) and checked against
the declared type before anything renders: a wrong type, an enum variant the enum lacks, an unknown
prop, and a missing prop with no default are each a distinct `RenderError`. Defaults are parsed from
the `prop … = default` text as written. An `option(T)` prop defaults to absent. This confirms #153's
item 10 for the forms the tracker's C-rows already cover (bool, int, text, enums with columns,
`option`); records and `list` props are not suppliable to a render yet.

## 3. Children and slots (U2) — designed, not built

A component needs to take content from its caller (`card`, `alert`, the shell components). Proposal:

```mz
component card
  prop heading: text
  prop body: content           ## a typed child-content parameter
  view
    surface
      row
        slot = "card-header"
        text = heading
      end
      body                     ## a prop of type `content` placed by name
    end
  end
```

`content` is a new prop type: an element tree supplied by the caller. A bare word on its own line in
a view that names a `content` prop places it; this reuses the existing "a bare word opens a child"
rule, resolved against the props before the element table, so a prop can never be shadowed by a tag
or the reverse (a clash is a diagnostic). The unnamed form is a prop called `children`. Because
`slot = "…"` already means the `data-slot` attribute, named slots are `content` props, not a second
word. A caller supplies content as a node list in the library API; a `.mz` call-site syntax for
composition is a separate decision (§8, Q2), since `uses button` composition is not rendered today.

## 4. Pass-through attributes (U3) — designed, not built

RFC-roots-renderer's rule is that the render path takes the contract's props only. Proposal,
contract first: a component that accepts extra attributes declares it, and the contract then lists
what is allowed: `prop attributes: attrs allow "aria-*" "data-*" "id"`. A name outside the allow
list is a render error, so a pass-through is never an open door for `onclick` or `style`. Values are
escaped like any other attribute. Without the declaration, extra attributes are
`RenderError::UnknownProp`, as in Wave 0.

## 5. Explicit raw output (U5) — designed, not built

One narrow door, for a pre-sanitised CSS string in a `<style>` element (`mzizi-theme-provider`), and
nothing else: a `raw` text attribute, legal only on the `style` element word, whose value must come
from a prop of type `css` — a newtype that only a sanitising constructor makes. A `raw` anywhere
else is a diagnostic. There is no raw HTML, so `markdown-renderer`, which has none by design, cannot
acquire one. The renderer has no code path from `text` to unescaped output.

## 6. Conditional attributes and expressions in a view

Writing `separator` against the registry's contract showed two things the view could not say:
`role="none"` versus `role="separator"` chosen by a `bool` prop (U10), and
`progress`'s `transform: translateX(-{100 - value}%)`, because an interpolation held one name and
no operators.

**Conditional attributes (U10), built.** An attribute line may carry a condition:

```mz
role = "none" when decorative else "separator"
aria_orientation = none when decorative else "vertical" when axis is vertical
```

`<value> when <condition> [else <value> when <condition>]… [else <value>]`. A condition is a `when`
element's: `<name>` (a `bool`), `not <name>`, or `<name> is <variant or none>`. The first link
whose condition holds is the attribute's value; with no plain final `else`, the attribute is
absent when none holds, and `none` as a value omits it. `text = …` takes the same form. The checker
reads every condition and every value (`MZ0707`, `MZ0708`, `MZ0711`, `MZ0712`); an `else` with no
`when` before it, or after a plain `else`, is `MZ0404`. The chain is part of the attribute's
canonical value, so the IR hash of a component moves when a condition does and is unchanged for
every component without one. A static contract clause about a conditional attribute is
unevaluable (there is no single value), and a `render` clause tests it. `separator` is written
with it and now renders `role="none"` or `role="separator"` and an `aria-orientation` only for a
semantic vertical rule, as the registry's contract has it; `data-decorative` is gone. Not covered:
`and` and `or` in a condition, and option narrowing inside the chain.

**Expressions in a view string (U11), built.** A `{…}` in a view attribute string holds a path, or
`int` arithmetic: integer literals, prop and enum-column paths, unary `-`, `+ - * / %` with the
usual precedence, parentheses, and `min(a, b)` and `max(a, b)` (`compiler/src/viewexpr.rs`; the
checker and the renderer read a body with the same parser, so what checks is what renders). Every
name in an expression must be an `int` (`MZ0711` otherwise); a malformed body is `MZ0714`. All
arithmetic is checked: a render that divides by zero or overflows is `RenderError::Arithmetic`,
never a wrapped number. There are no comparisons, no text operators and no other calls. `progress`
is written with it: `style = "transform: translateX(-{100 - min(max(value, 0), 100)}%);"`, and the
clamp is `min` and `max`, so an out-of-range value never renders a negative offset. `style` joins
the attributes that pass through to markup.

## 7. The render oracle (U7) — Wave 1, built

`mz contract` evaluates render-backed clauses: the component is rendered with the named props, and
the clause is tested against what came out.

```mz
render [with <prop> = <value> …] [of <attr>] <predicate>
```

- `of <attr>` reads one thing from the rendered root element, by its view name (`slot`, `role`,
  `class`, `axis`), or `text` for the root's own text. With no `of`, the predicate is tested against
  the whole markup string.
- Predicates: `is`, `contains`, `not_empty`, and the new `omits "<text>"` (absence), which only a
  `render` clause takes.
- A prop not named takes its default. A prop with no default that the clause does not supply, an
  unknown prop, a wrong-typed value, or a view the renderer refuses is **MZ0605, cannot be
  evaluated**: a contract that cannot render never passes.
- `render` opens this form only when `with`, `of`, `contains`, `omits` or `not_empty` follows, so a
  prop named `render` still reads as a name.
- A string in a component has no escape for `"`, so a clause cannot quote markup with attributes in
  it. That is why `of <attr>` exists: attribute values are asserted exactly, and `contains` is for
  class fragments and tags.

## 8. Open questions for the owner

**Q1 is accepted** (owner, recorded on #153, comment 6703585202, 2026-11-07): the closed
element-to-tag table of §1.1 stands. **Q2 is not decided**, so children and slots at the call site
(§3, U2) wait for the owner.

- **Q1.** Is the closed element table (§1.1) the right shape, or should an element word carry its tag
  in the component (a `tag = "section"` attribute)? The table keeps generation from picking an
  unlisted tag; it costs one table edit per new word.
- **Q2.** Call-site syntax for composing one component inside another (`uses button` is read today
  but not rendered).
- **Q3.** Answered by U10: `separator` reproduces the registry's `role` rule.

## 9. Waves

| Wave | Adds                                                                                 | Tracker | State                                                       |
| ---- | ------------------------------------------------------------------------------------ | ------- | ----------------------------------------------------------- |
| 0    | Element tree, serialiser, escaping, typed props, `when`/`else`, the closed tag table | U1, U9  | built, `compiler/tests/render.rs`                           |
| 1    | Render-backed contract clauses                                                       | U7      | built, `compiler/tests/contracts.rs`                        |
| 1c   | Conditional attributes (`value when c else value`)                                   | U10     | built, `primitives/separator.mz`                            |
| 1b   | Integer arithmetic in a view string                                                  | U11     | built, `compiler/src/viewexpr.rs`, `primitives/progress.mz` |
| 2    | `content` props, named placement                                                     | U2      | designed; waits for the owner's call-site decision (Q2)     |
| 3    | `attrs` pass-through with an allow list                                              | U3      | designed                                                    |
| 4    | `raw` on `style`, from a `css` value                                                 | U5      | designed                                                    |
