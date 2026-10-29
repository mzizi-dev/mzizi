# Writing one C++ service with cpp-httplib

You write one C++20 (`.cpp`) file holding one HTTP service, built on `cpp-httplib` (v0.63)
and `nlohmann/json` (v3.12). Your file is checked with `clang++ -std=c++20 -fsyntax-only
-Wall -Wextra`; errors fail it, warnings print and do not. It is then compiled, linked,
run, and answers real HTTP requests. If a construct is not shown here, use what you know
of C++20 and these two libraries.

## File shape

- One file with a `main`. Include `<httplib.h>` and `<nlohmann/json.hpp>` (both are on the
  include path), and the standard headers you use. No other library exists.
- Make a `httplib::Server svr;`, register handlers, then call
  `svr.listen("0.0.0.0", port)` last. Read the port from the `PORT` environment variable
  (`std::getenv("PORT")` returns `nullptr` when unset). `listen` blocks until the server
  stops and returns `bool`.
- `-Wall -Wextra` warns about unused parameters and variables; leave a parameter unnamed
  (`const httplib::Request&`) when you do not use it.

## What is available

**Routes.** `svr.Get(pattern, handler)`, and `Post`, `Put`, `Patch`, `Delete`, `Options`.
A handler is `[](const httplib::Request& req, httplib::Response& res) { ... }`. A pattern
is a path with `:name` segments (`/v1/items/:id`), read with `req.path_params.at("id")`;
or a regular expression, whose groups are `req.matches[1].str()`. A `Get` route also
answers `HEAD`, with the body dropped. Routes are tried in the order registered.

**The request `req`.**

| Member                               | Gives                                          |
| ------------------------------------ | ---------------------------------------------- |
| `req.method`, `req.path`, `req.body` | `std::string`s; `path` has no query, undecoded |
| `req.get_param_value("k")`           | a query parameter (`""` when absent)           |
| `req.has_param("k")`                 | whether a query parameter is present           |
| `req.params`                         | every query parameter, as a multimap           |
| `req.get_header_value("name")`       | a request header (`""` when absent)            |
| `req.has_header("name")`             | whether it is present                          |

**The response `res`.** Set `res.status = 404;` (an `int`), `res.set_header("name",
"value");` and `res.set_content(body, "application/json");`, which sets the body and its
`content-type`. A response with no `set_content` has an empty body and no `content-type`.
`res.set_redirect(location, 308)` sets `location` and the status.

**JSON.** `using json = nlohmann::json;`. Build with `json j = {{"id", "all"},
{"readings", 2}, {"unit", nullptr}};` (`nullptr` is JSON `null`; an array is
`json::array({1, 2})`), send with `j.dump()`. Read with `j.at("key").get<int>()`, which
throws on a missing key or wrong type. `json::parse(text)` throws on malformed input; use
`json::parse(text, nullptr, false)` and test `.is_discarded()` to avoid the throw. Keys
come out of `dump()` in sorted order.

**Hooks.** `svr.set_pre_routing_handler(f)` runs before routing, with
`(const Request&, Response&)`, and returns `httplib::Server::HandlerResponse::Handled` to
send the response it filled in, or `Unhandled` to route normally. `svr.set_post_routing_handler(f)`
runs after a handler, with the same arguments, and returns nothing; set headers there.
`svr.set_error_handler(f)` runs when the status is an error with no body (an unmatched
route is `404`). `svr.set_exception_handler(f)` takes `(req, res, std::exception_ptr)`.

**Defaults.** A path no route matches gets an empty `404`. There is no automatic `OPTIONS`
answer, no `405`, and no handling of a trailing slash: `/a/` does not match `/a`.

## Naming rules

- Types are `PascalCase` or `snake_case` per your taste; be consistent. Functions and
  variables are `snake_case`. Put helpers in an anonymous `namespace { }`.
- JSON keys are whatever the task says, written as string literals.
- Qualify library names: `httplib::Server`, `nlohmann::json`, `std::string`.

## Reading the checker's output

A file wrong on purpose:

```cpp
// WRONG ON PURPOSE: two mistakes.
#include <httplib.h>

int main() {
  httplib::Server svr;
  svr.Get("/v1/items", [](const httplib::Request& req, httplib::Response& res) {
    int total = req.path;
    res.set_content(count, "application/json");
  });
  svr.listen("0.0.0.0", 8080);
}
```

`clang++` prints each diagnostic with its source line and a caret, then a count:

```text
tag.cpp:7:9: error: no viable conversion from 'const std::string' (aka 'const basic_string<char>') to 'int'
    7 |     int total = req.path;
      |         ^       ~~~~~~~~
/usr/lib/gcc/aarch64-alpine-linux-musl/14.2.0/../../../../include/c++/14.2.0/bits/basic_string.h:949:7: note: candidate function
  949 |       operator __sv_type() const noexcept
      |       ^
tag.cpp:8:21: error: use of undeclared identifier 'count'
    8 |     res.set_content(count, "application/json");
      |                     ^
2 errors generated.
```

- `file:line:column: error: message` locates the error; `warning:` lines are not
  failures. A `note:` explains the diagnostic above it.
- Fix every error in one pass: each is a separate mistake, though one early mistake (a
  missing header, a misspelt type) can cause several lines below it.
- The checker parses and type-checks only; linking happens later. It cannot see that a
  route answers the right status, so re-read the task for every status, header and body
  before you finish.

## A worked example

A weather-station service. It is not one of the tasks.

```cpp
// Two stations, their readings, and a JSON 404.
#include <algorithm>
#include <array>
#include <cstdlib>
#include <string>

#include <httplib.h>
#include <nlohmann/json.hpp>

using json = nlohmann::json;

namespace {

constexpr std::array<const char*, 2> kStations = {"harare", "bulawayo"};

void send(httplib::Response& res, int status, const json& body) {
  res.status = status;
  res.set_content(body.dump(), "application/json");
}

void problem(httplib::Response& res, int status, const std::string& error) {
  send(res, status, {{"error", error}});
}

}  // namespace

int main() {
  httplib::Server svr;

  svr.Get("/v1/stations", [](const httplib::Request& req, httplib::Response& res) {
    if (!req.has_param("limit")) {
      send(res, 200, {{"id", "all"}, {"readings", 2}, {"unit", nullptr}});
      return;
    }
    const std::string raw = req.get_param_value("limit");
    const bool digits = !raw.empty() && std::all_of(raw.begin(), raw.end(), ::isdigit);
    if (digits && raw.size() < 9 && std::stoi(raw) >= 1) {
      send(res, 200, {{"id", "all"}, {"readings", std::stoi(raw)}, {"unit", nullptr}});
      return;
    }
    problem(res, 400, "limit must be 1 or more");
  });

  svr.Get("/v1/stations/:id", [](const httplib::Request& req, httplib::Response& res) {
    const std::string id = req.path_params.at("id");
    const bool known = std::any_of(kStations.begin(), kStations.end(),
                                   [&](const char* s) { return id == s; });
    if (!known) {
      problem(res, 404, "no " + id);
      return;
    }
    json unit = req.has_param("unit") ? json(req.get_param_value("unit")) : json(nullptr);
    send(res, 200, {{"id", id}, {"readings", 24}, {"unit", unit}});
    res.set_header("cache-control", "max-age=60");
  });

  svr.Post("/v1/stations/:id/reset",
           [](const httplib::Request&, httplib::Response& res) { res.status = 204; });

  svr.set_error_handler([](const httplib::Request&, httplib::Response& res) {
    if (res.status == 404 && res.body.empty()) problem(res, 404, "Not found");
  });

  svr.set_post_routing_handler([](const httplib::Request&, httplib::Response& res) {
    res.set_header("x-service", "stations");
  });

  const char* port = std::getenv("PORT");
  svr.listen("0.0.0.0", port ? std::stoi(port) : 8080);
  return 0;
}
```
