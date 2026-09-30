# Writing one Mzizi service

Mzizi (`.mz`) is a small language. One file holds one HTTP service. It has one form per
intent: if a construct is not shown here, assume it does not exist. Your file is checked
with `mz check --agent`; errors fail it, warnings do not. It is then built and served, and
answers real HTTP requests.

## File shape

Write the parts in this order: doc lines, service `header` lines, `record`s, `route`s,
`fallback`, `contract`.

- `service <name>` opens the file and `end service <name>` closes it, repeating the name.
  Every other block (`record`, `route`, `fallback`, `when`, `contract`) closes with a bare
  `end`. Never `end route`.
- `##` to end of line is the only comment. `//` and `/* */` are errors.
- There are no imports, no `main`, no server setup and no port: the build serves the file.

## What is available

**Records** shape JSON bodies: one `field <name>: <type>` per line, closed by `end`. A
field is `text`, `int`, `bool`, or `option(...)` of one of those. A record becomes one JSON
object, its fields in declaration order; an absent option is `null`.

**A route** is one method and one path, then its handler:

```text
route <name>
  <method> "<path>"
  query <name>: option(<type>)
  <statements>
end
```

- The method is one of `get`, `head`, `post`, `put`, `patch`, `delete`, `options`, lower
  case. A second method on the same path is a second route.
- A path is `/` or `/`-separated segments. A segment is lower-case letters, digits and
  `-._~`, or a whole `{name}`, which binds `name` as `text` for the handler. A path never
  ends with `/` and never holds `?`.
- `query` lines come right after the method line. A query parameter is always an option,
  because a client may leave it out: `option(int)`, `option(text)` or `option(bool)`. An
  `int` is `-` and digits only, so `"0x10"`, `"1e2"` and `" 7 "` are `none`.

**Three statements** make a handler, and every path through it must end in `respond`:

| Statement                      | Means                                                |
| ------------------------------ | ---------------------------------------------------- |
| `when <cond>` … `end`          | run the lines inside when the condition holds        |
| `when <cond>` … `else` … `end` | one branch or the other                              |
| `header "<name>" "<value>"`    | set a header on the response this path sends         |
| `respond <status> [<body>]`    | send the response; nothing after it on the path runs |

A condition is `p` or `not p` (a `bool`), `p is <value>`, `p in <value> <value> …`,
`p at_least <n>`, `p at_most <n>` (an `int`), or `p is none` (an option). Inside the
`else` of `when p is none`, `p` is its value; anywhere else, using an option is an error.
There is no `if`, `==`, `return`, loop, variable or function.

A body is one of:

| Body                          | Sends              | `content-type`              |
| ----------------------------- | ------------------ | --------------------------- |
| _(nothing)_                   | no body            | none                        |
| `json <record> <field> <v> …` | the record as JSON | `application/json`          |
| `text "<string>"`             | the string         | `text/plain; charset=utf-8` |

`json` names the record, then every field and its value, with no punctuation:
`respond 200 json item name name count 2`. A value is a literal or a parameter. A string
interpolates a parameter with `{...}`: `"no {name}"`. Header names are lower case. A
`header` line at service level, outside any route, applies to every response the service
sends.

**The runtime already answers**, so do not write them:

- `HEAD` for a `get` route: the `GET` handler's status and headers, with no body.
- `OPTIONS` on a route's path: `204`, with `allow` listing its methods, `HEAD` and
  `OPTIONS` (`GET, HEAD, OPTIONS` for a path with one `get` route).
- Any other method on a route's path: `405`, with the same `allow`, and no body.
- A path ending in `/`, or holding `//`: `308` to the path without them, query kept.

A path no route matches runs `fallback`, or gets an empty `404` when there is none.

**The contract** is optional here. `example <method> "<path>" <check>` sends one request;
`ensure <check>` must hold for every request. A check is `status`, `header "<name>"`,
`body` or `body.<field>`, then `is <value>`, `in <values>`, `not_empty` or
`contains "<s>"`.

## Naming rules

- Every name you write is `snake_case`: services, records, fields, routes, parameters.
- A route's name is yours to choose; only its method and path are observable.
- Parameters are named in the path (`{id}`) or by `query`; nothing else is in scope.

## Reading the checker's output

A file wrong on purpose:

```mz
## WRONG ON PURPOSE: three mistakes.
service shop

  record problem
    field error: text
  end

  route item
    GET "/v1/items/{id}/"
    when id is "1"
      respond 200 text "one"
    end
  end

end service shop
```

`mz check --agent` prints one line per error, then a summary:

```text
{"code":"MZ0613","severity":"warning","file":"candidate.mz","span":[2,9,2,13],"say":"`service shop` has no `contract` block — its behaviour is unverified (RFC-0010 §8)"}
{"code":"MZ0801","severity":"error","file":"candidate.mz","span":[9,5,9,8],"say":"`GET` is not a method — a route declares one of get, head, post, put, patch, delete, options","fix":{"span":[9,5,9,8],"replace":"get","confidence":"exact"}}
{"code":"MZ0802","severity":"error","file":"candidate.mz","span":[9,9,9,26],"say":"`\"/v1/items/{id}/\"` ends with `/` — patterns have no trailing slash; the runtime redirects `…/` to the path without it","fix":{"span":[9,9,9,26],"replace":"\"/v1/items/{id}\"","confidence":"exact"}}
{"code":"MZ0804","severity":"error","file":"candidate.mz","span":[13,3,13,6],"say":"a path through `route item` reaches `end` without a `respond` — when the `when` on line 10 does not respond on both branches; every path must respond"}
{"summary":true,"errors":3,"warnings":1,"exact_fixable":2,"ms":0}
```

- `span` is `[line, column, end line, end column]`. `say` quotes the source, so you need
  not re-read the file.
- A `fix` marked `exact` is the whole repair: apply it as written. A `guess` needs a look.
- Fix every error in one pass: each line is a separate mistake, never a cascade.
- Here the last error needs a decision: add `respond 404 json problem error "Not found"`
  after the `when`'s `end`, so the false path responds too.

## A worked example

A weather-station service. It is not one of the tasks.

```mz
## Two stations, their readings, and a JSON 404.
service stations

  header "x-service" "stations"

  record station
    field id: text
    field readings: int
    field unit: option(text)
  end

  record problem
    field error: text
  end

  route station_list
    get "/v1/stations"
    query limit: option(int)
    when limit is none
      respond 200 json station id "all" readings 2 unit none
    else
      when limit at_least 1
        respond 200 json station id "all" readings limit unit none
      end
      respond 400 json problem error "limit must be 1 or more"
    end
  end

  route station_item
    get "/v1/stations/{id}"
    query unit: option(text)
    when id in "harare" "bulawayo"
      header "cache-control" "max-age=60"
      respond 200 json station id id readings 24 unit unit
    end
    respond 404 json problem error "no station {id}"
  end

  route station_reset
    post "/v1/stations/{id}/reset"
    respond 204
  end

  fallback
    respond 404 json problem error "Not found"
  end

  contract
    example get "/v1/stations/harare" body.readings is 24
    example get "/v1/stations?limit=0" status is 400
    example delete "/v1/stations/harare" status is 405
    ensure header "x-service" is "stations"
  end

end service stations
```
