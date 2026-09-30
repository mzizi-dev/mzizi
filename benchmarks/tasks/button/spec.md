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
