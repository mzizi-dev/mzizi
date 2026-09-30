# Badge

A small inline label: a single `span` element. It is a Layer 2 primitive.

## Variant groups

### `variant`: the colour treatment. Default: `default`

| Variant       | Classes                                                                                                                                                          |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`     | `bg-primary text-primary-foreground [a]:hover:bg-primary/80`                                                                                                     |
| `secondary`   | `bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80`                                                                                               |
| `destructive` | `bg-destructive/10 [a]:hover:bg-destructive/20 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 text-destructive dark:bg-destructive/20` |
| `outline`     | `border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground bg-input/30`                                                                   |
| `ghost`       | `hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50`                                                                                              |
| `link`        | `text-primary underline-offset-4 hover:underline`                                                                                                                |

## Base classes

Every badge has these classes, then its `variant` classes, then any extra classes the caller
passes:

```text
h-5 gap-1 rounded-md border border-transparent px-2 py-0.5 text-xs font-medium transition-all has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&>svg]:size-3! inline-flex items-center justify-center w-fit whitespace-nowrap shrink-0 [&>svg]:pointer-events-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive overflow-hidden group/badge
```

## Rendered element

One `span` element with these attributes:

| Attribute      | Value                                |
| -------------- | ------------------------------------ |
| `data-slot`    | `badge`                              |
| `data-portal`  | `https://mzizi.dev/components/badge` |
| `data-variant` | the `variant` in use                 |
| `class`        | the classes above, in that order     |

Its content is the badge's children. Any other attribute the caller gives the badge is
passed through to the element.
