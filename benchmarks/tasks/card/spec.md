# Card

A container for grouped content, made of seven parts. It is a Layer 2 primitive, the most
composed one: every component that shows content in a container renders a card. Every part
is a `div`, marks itself with a `data-slot`, and appends any extra classes the caller passes
after its own. Any other attribute the caller gives a part is passed through to its `div`.

## Variant groups

### `size`: the card's spacing. Default: `default`

| Variant   | Effect                                                                                   |
| --------- | ---------------------------------------------------------------------------------------- |
| `default` | the spacing below                                                                        |
| `sm`      | tighter spacing, applied by the `data-[size=sm]` and `group-data-[size=sm]/card` classes |

The size has no classes of its own. The card writes it to its `data-size` attribute, and
the classes below select on that attribute.

The card also takes a `loading` flag, off by default.

## Parts

| Part        | `data-slot`        | Classes                                                                                                                                                                                                                                                                                                                              |
| ----------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| card        | `card`             | `group/card flex flex-col gap-6 overflow-hidden rounded-[var(--radius-lg,14px)] bg-card py-6 text-sm text-card-foreground ring-1 ring-foreground/10 has-[>img:first-child]:pt-0 data-[size=sm]:gap-4 data-[size=sm]:py-4 *:[img:first-child]:rounded-t-[var(--radius-md,12px)] *:[img:last-child]:rounded-b-[var(--radius-md,12px)]` |
| header      | `card-header`      | `group/card-header @container/card-header grid auto-rows-min items-start gap-2 rounded-t-[var(--radius-md,12px)] px-6 group-data-[size=sm]/card:px-4 has-data-[slot=card-action]:grid-cols-[1fr_auto] has-data-[slot=card-description]:grid-rows-[auto_auto] [.border-b]:pb-6 group-data-[size=sm]/card:[.border-b]:pb-4`            |
| title       | `card-title`       | `text-base font-medium`                                                                                                                                                                                                                                                                                                              |
| description | `card-description` | `text-sm text-muted-foreground`                                                                                                                                                                                                                                                                                                      |
| action      | `card-action`      | `col-start-2 row-span-2 row-start-1 self-start justify-self-end`                                                                                                                                                                                                                                                                     |
| content     | `card-content`     | `px-6 group-data-[size=sm]/card:px-4`                                                                                                                                                                                                                                                                                                |
| footer      | `card-footer`      | `flex items-center rounded-b-[var(--radius-md,12px)] px-6 group-data-[size=sm]/card:px-4 [.border-t]:pt-6 group-data-[size=sm]/card:[.border-t]:pt-4`                                                                                                                                                                                |

The card renders its children inside it. The other six parts are placed inside a card by the
caller, and each renders its own children.

## The card element

When `loading` is off, the card is one `div` with `data-slot="card"`, `data-size` set to the
size in use, and the card classes above.

When `loading` is on, the card renders a skeleton instead of its children: one `div` with
`data-slot="card"`, `data-portal="https://mzizi.dev/components/card"`, a `data-loading`
attribute, and these classes:

```text
group/card flex animate-pulse flex-col gap-4 overflow-hidden rounded-[var(--radius-lg,14px)] bg-card p-6 text-sm ring-1 ring-foreground/10
```

followed by `gap-3 p-4` when the size is `sm`, and then any extra classes. Inside it are three
placeholder bars, each a `div`:

| Bar | Classes                       |
| --- | ----------------------------- |
| 1   | `h-4 w-2/3 rounded bg-muted`  |
| 2   | `h-3 w-full rounded bg-muted` |
| 3   | `h-3 w-4/5 rounded bg-muted`  |
