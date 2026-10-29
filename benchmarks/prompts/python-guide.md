# Writing one Python service with FastAPI

You write one Python (`.py`) file holding one HTTP service, built on FastAPI with pydantic
v2. Your file is checked with `pyright` in its default `standard` mode; errors fail it,
warnings do not. It is then imported and served by uvicorn, and answers real HTTP
requests. If a construct is not shown here, use what you know of Python, FastAPI and
Starlette.

## File shape

- One file. Import from the standard library, `fastapi` (which re-exports Starlette's
  `Request` and `Response`), `fastapi.responses` and `pydantic`. No other package is
  installed.
- Create one module-level `app = FastAPI()`. The name `app` is how the server finds it.
  Do not call `uvicorn.run`, read a port or add `if __name__ == "__main__"`: the build
  serves `app`.
- Annotate every function's parameters and return type. Pyright reports what it can prove
  from them, and a handler with no annotations proves nothing.

## What is available

**Routing.** Decorators on the app: `@app.get(path)`, `@app.post(path)`, `put`, `patch`,
`delete`, `options`, and `@app.api_route(path, methods=["GET", "HEAD"])` for several
methods on one handler. A path is `/`-separated segments and `{name}` parameters
(`/v1/items/{item_id}`). A handler is `def` or `async def`; a parameter named in the path
receives it as the annotated type (`item_id: str`), and any other simple parameter is a
query parameter (`limit: int | None = None`). `Request` as a parameter gives the raw request:
`request.method`, `request.url.path`, `request.url.query`, `request.headers.get("name")`,
`await request.json()`.

**Parameters in detail.** `from fastapi import Header, Query`. `limit: int = Query(default=10,
ge=1)` adds a default and a bound; `agent: str | None = Header(default=None)` reads the
`user-agent` header (underscores become hyphens). A parameter annotated with a model
(`body: Station`) is parsed from the JSON request body. Whatever the source, a value that
does not fit its annotation is answered `422` with `{"detail": [...]}` before your handler
runs, so annotate as `str` when you need to see the raw text. `await request.body()` gives
the raw bytes, and `await request.json()` raises `ValueError` on malformed JSON.

**Responses.** Return a `dict`, a list or a pydantic model and FastAPI sends it as JSON with
`200`. To control the status or headers, return a response object:

| Return                                            | Sends                                  |
| ------------------------------------------------- | -------------------------------------- |
| `JSONResponse(body, status_code=404, headers={})` | JSON, `content-type: application/json` |
| `Response(status_code=204, headers={})`           | no body                                |
| `PlainTextResponse("text", status_code=200)`      | `text/plain; charset=utf-8`            |
| `RedirectResponse(url, status_code=308)`          | a redirect with a `location` header    |

A decorator argument `status_code=201` sets the default status. Header names you pass are
sent as given; use lower case.

**Models.** `class Station(BaseModel): id: str; readings: int; unit: str | None = None`
(`from pydantic import BaseModel`). A model is a typed JSON object; `model.model_dump()`
gives a `dict`. A route annotated `-> Station` or `response_model=Station` is filtered to
the model's fields.

**Middleware** is `@app.middleware("http")` on `async def f(request: Request, call_next)`.
Await `call_next(request)` for the response, change its `headers`, and return it; returning
a response without calling `call_next` stops the request there. Annotate `call_next` as
`Callable[[Request], Awaitable[Response]]`.

**Errors.** `raise HTTPException(status_code=404, detail="x")` answers `{"detail": "x"}`.
Replace a handler with `@app.exception_handler(404)`, taking `(request, exc)` and returning
a response. A query or path value of the wrong type is answered `422` before your handler
runs.

**Defaults you must know.** `FastAPI()` also serves `/docs`, `/redoc` and `/openapi.json`;
pass `docs_url=None, redoc_url=None, openapi_url=None` to remove them. A `@app.get` route
answers `GET` only: a `HEAD` or `OPTIONS` request to it gets Starlette's `405` with
`allow: GET` and a JSON body `{"detail":"Method Not Allowed"}`. A path with no route gets
`404` and `{"detail":"Not Found"}`. A request for `/a/` when `/a` is the route is answered
`307` to the absolute URL of `/a`; `FastAPI(redirect_slashes=False)` turns that off.

## Naming rules

- Functions, variables and parameters are `snake_case`; classes are `PascalCase`; constants
  are `UPPER_SNAKE`. JSON field names are whatever the task says: use the same spelling in
  a model's fields or a dict's keys.
- A handler's name is yours to choose; only its method and path are observable.
- A path parameter's name in the function must equal its name in the path.

## Reading the checker's output

A file wrong on purpose:

```python
# WRONG ON PURPOSE: three mistakes.
from fastapi import FastAPI

app = FastAPI()


@app.get("/v1/items/{item_id}")
async def item(item_id: str) -> dict[str, int]:
    count: int = item_id
    return {"id": item_id, "total": total}
```

`pyright` prints each file's errors, then a count line:

```text
tag.py
  tag.py:9:18 - error: Type "str" is not assignable to declared type "int"
    "str" is not assignable to "int" (reportAssignmentType)
  tag.py:10:19 - error: Type "dict[str, str | Unknown]" is not assignable to return type "dict[str, int]"
    "str" is not assignable to "int" (reportReturnType)
  tag.py:10:37 - error: "total" is not defined (reportUndefinedVariable)
3 errors, 0 warnings, 0 informations
```

- `file:line:column - error:` locates the error; the rule in parentheses at the end
  (`reportAssignmentType`) names the kind. Lines indented under an error explain it.
- Fix every error in one pass: each is a separate mistake.
- Rules you will meet: `reportAssignmentType` and `reportReturnType` a value of the wrong
  type; `reportArgumentType` a wrong argument; `reportCallIssue` the wrong arguments;
  `reportUndefinedVariable` an unknown name; `reportAttributeAccessIssue` a member that
  does not exist; `reportOptionalMemberAccess` a member read on a possible `None`;
  `reportMissingImports` a module that is not installed.
- The checker proves types only. It cannot see that a route answers the right status, so
  re-read the task for every status, header and body before you finish.

## A worked example

A weather-station service. It is not one of the tasks.

```python
"""Two stations, their readings, and a JSON 404."""

from typing import Awaitable, Callable

from fastapi import FastAPI, Request, Response
from fastapi.responses import JSONResponse
from pydantic import BaseModel

STATIONS = ("harare", "bulawayo")


class Station(BaseModel):
    id: str
    readings: int
    unit: str | None = None


class Problem(BaseModel):
    error: str


app = FastAPI(docs_url=None, redoc_url=None, openapi_url=None)


def problem(status: int, error: str) -> Response:
    return JSONResponse(Problem(error=error).model_dump(), status_code=status)


@app.middleware("http")
async def tag_service(
    request: Request, call_next: Callable[[Request], Awaitable[Response]]
) -> Response:
    response = await call_next(request)
    response.headers["x-service"] = "stations"
    return response


@app.get("/v1/stations")
async def station_list(limit: str | None = None) -> Response:
    if limit is None:
        return JSONResponse(Station(id="all", readings=2).model_dump())
    if limit.isdigit() and int(limit) >= 1:
        return JSONResponse(Station(id="all", readings=int(limit)).model_dump())
    return problem(400, "limit must be 1 or more")


@app.get("/v1/stations/{station_id}")
async def station_item(station_id: str, unit: str | None = None) -> Response:
    if station_id in STATIONS:
        body = Station(id=station_id, readings=24, unit=unit).model_dump()
        return JSONResponse(body, headers={"cache-control": "max-age=60"})
    return problem(404, f"no {station_id}")


@app.post("/v1/stations/{station_id}/reset")
async def station_reset(station_id: str) -> Response:
    return Response(status_code=204)


@app.exception_handler(404)
async def not_found(request: Request, exc: Exception) -> Response:
    return problem(404, "Not found")
```
