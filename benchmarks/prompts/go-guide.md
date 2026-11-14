# Writing one Go service with net/http

You write one Go (`.go`) file holding one HTTP service, in `package main`, using only the
standard library (Go 1.24). Your file is checked with `go build ./...` and then `go vet
./...`; build errors and vet findings both fail it. It is then built, run, and answers real
HTTP requests. If a construct is not shown here, use what you know of Go and `net/http`.

## File shape

- One file, `package main`, with its imports in one block and a `func main()`. No module
  outside the standard library exists, so import only std packages (`net/http`,
  `encoding/json`, `strings`, `strconv`, `os`, `log`, `fmt`).
- `main` reads the port from the `PORT` environment variable and listens on all
  interfaces: `http.ListenAndServe("0.0.0.0:"+os.Getenv("PORT"), handler)`. It must not
  return; `log.Fatal(http.ListenAndServe(...))` is the usual shape.
- An unused import or an unused local variable is a compile error in Go, not a warning.

## What is available

**Handlers.** A handler is `func(w http.ResponseWriter, r *http.Request)`. In `r`:
`r.Method`, `r.URL.Path` (decoded), `r.URL.RawQuery`, `r.URL.Query().Get("k")` (`""` when
absent), `r.Header.Get("name")`, and `r.Body`. In `w`: set headers first, then call
`w.WriteHeader(status)`, then write the body; a header set after `WriteHeader` is lost.
Without a `WriteHeader` call, the first write sends `200`.

**Header names** passed to `w.Header().Set` are canonicalised (`x-a` is sent as `X-A`);
HTTP header names are case-insensitive, so this is fine.

**A router.** `mux := http.NewServeMux()` and `mux.HandleFunc("GET /v1/items/{id}", f)`.
A pattern is an optional method, a space, and a path; `{id}` matches one non-empty
segment, read with `r.PathValue("id")`, and a pattern ending in `/` matches everything
under it. Pass `mux` to `ListenAndServe`. Or skip the mux and pass
`http.HandlerFunc(f)` to answer every request yourself.

**What the mux does for you.** A pattern with a method answers other methods with `405`,
`allow` and a text body; `GET` patterns also answer `HEAD`. `OPTIONS` is not answered.
A path no pattern matches gets `404` with the text `404 page not found`. A request path
holding `//` or `.` segments is redirected to the cleaned path with `301`. A path ending in
`/` is not matched to a pattern without it. A handler passed directly gets none of this.

**JSON.** `json.NewEncoder(w).Encode(v)` writes `v` as JSON followed by a newline;
`json.Marshal(v)` returns the bytes. Structs use field tags for names:

```text
type Station struct {
    ID       string  `json:"id"`
    Readings int     `json:"readings"`
    Unit     *string `json:"unit"` // nil encodes as null
}
```

Only exported (capitalised) fields are encoded. `map[string]any` also works.
`json.NewDecoder(r.Body).Decode(&v)` returns an error for malformed JSON.
`strconv.Atoi("12")` returns `(int, error)`.

**Responses.** The standard pieces: `w.Header().Set("content-type", "application/json")`
before `WriteHeader`; `http.Error(w, "msg", status)` writes a plain-text body; a status
constant such as `http.StatusNotFound` is an `int`; `http.Redirect(w, r, url, code)`
writes a small HTML body for `GET`.

**Middleware** is a function from handler to handler. Wrap the mux in one to add headers or
rewrite requests before routing:

```text
func withHeader(next http.Handler) http.Handler {
    return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        w.Header().Set("x-a", "1")
        next.ServeHTTP(w, r)
    })
}
```

A header set before `next.ServeHTTP` is on every response `next` sends, unless `next`
replaces it. Return from a handler after writing a response; code after `WriteHeader`
still runs, and a second `WriteHeader` is ignored with a log line.

**Bodies and errors.** The server drops the body of a response to `HEAD` and of a `204` or
`304`, so a handler may write one without harm. `http.MaxBytesReader(w, r.Body, n)` caps a
request body at `n` bytes. `r.ContentLength` is `-1` when unknown, and `r.Header.Get("content-type")`
gives the media type. A handler that panics is recovered by the server, which closes the
connection with no response; return an error response yourself instead. `r.URL.Path` is
decoded (`%2F` becomes `/`); `r.URL.EscapedPath()` is as sent. `r.URL.RawQuery` is the
query string without its `?`, and `r.URL.ForceQuery` is true for a bare trailing `?`.

**Rules that trip authors.** A returned error you do not need is dropped with `_ =` by
convention; `:=` declares and `=` assigns; a variable declared and never read is an error;
a function with a result must return on every path; `any` is the empty interface;
`for range n` loops `n` times; `slices.Contains` and `strings.HasPrefix` exist.

## Naming rules

- Exported names are `PascalCase`; unexported names are `camelCase`. Acronyms keep their
  case (`ID`, `URL`). A file has one `package main`.
- JSON field names come from struct tags or map keys: use the spelling the task says.
- `go vet` objects to a `Printf`-style call whose verbs do not match its arguments, a
  struct tag that is malformed, and a lock copied by value.

## Reading the checker's output

A file wrong on purpose:

```go
// WRONG ON PURPOSE: three mistakes.
package main

import (
    "fmt"
    "net/http"
)

func item(w http.ResponseWriter, r *http.Request) {
    var total int = r.URL.Path
    fmt.Fprintln(w, count)
}

func main() {
    http.HandleFunc("/v1/items", item)
}
```

`go build ./...` prints `# candidate`, then one line per error:

```text
# candidate
./tag.go:10:9: declared and not used: total
./tag.go:10:21: cannot use r.URL.Path (variable of type string) as int value in variable declaration
./tag.go:11:21: undefined: count
```

- `./file:line:column: message` locates the error. Go stops after ten errors and says
  `too many errors`.
- Fix every error in one pass: each line is a separate mistake. If the build passes,
  `go vet` runs and prints its findings the same way.
- The checker proves that the program compiles and vets. It cannot see that a route
  answers the right status, so re-read the task for every status, header and body before
  you finish.

## A worked example

A weather-station service. It is not one of the tasks.

```go
// Two stations, their readings, and a JSON 404.
package main

import (
    "encoding/json"
    "log"
    "net/http"
    "os"
    "slices"
    "strconv"
)

type station struct {
    ID       string  `json:"id"`
    Readings int     `json:"readings"`
    Unit     *string `json:"unit"`
}

type problem struct {
    Error string `json:"error"`
}

var stations = []string{"harare", "bulawayo"}

func send(w http.ResponseWriter, status int, body any) {
    w.Header().Set("content-type", "application/json")
    w.WriteHeader(status)
    _ = json.NewEncoder(w).Encode(body)
}

func stationList(w http.ResponseWriter, r *http.Request) {
    raw := r.URL.Query().Get("limit")
    if raw == "" {
        send(w, http.StatusOK, station{ID: "all", Readings: 2})
        return
    }
    if n, err := strconv.Atoi(raw); err == nil && n >= 1 {
        send(w, http.StatusOK, station{ID: "all", Readings: n})
        return
    }
    send(w, http.StatusBadRequest, problem{Error: "limit must be 1 or more"})
}

func stationItem(w http.ResponseWriter, r *http.Request) {
    id := r.PathValue("id")
    if !slices.Contains(stations, id) {
        send(w, http.StatusNotFound, problem{Error: "no " + id})
        return
    }
    s := station{ID: id, Readings: 24}
    if unit := r.URL.Query().Get("unit"); unit != "" {
        s.Unit = &unit
    }
    w.Header().Set("cache-control", "max-age=60")
    send(w, http.StatusOK, s)
}

func main() {
    mux := http.NewServeMux()
    mux.HandleFunc("GET /v1/stations", stationList)
    mux.HandleFunc("GET /v1/stations/{id}", stationItem)
    mux.HandleFunc("POST /v1/stations/{id}/reset", func(w http.ResponseWriter, r *http.Request) {
        w.WriteHeader(http.StatusNoContent)
    })
    handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
        w.Header().Set("x-service", "stations")
        mux.ServeHTTP(w, r)
    })
    log.Fatal(http.ListenAndServe("0.0.0.0:"+os.Getenv("PORT"), handler))
}
```
