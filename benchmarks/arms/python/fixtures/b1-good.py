"""A known-good B1 solution in Python with FastAPI, written to check the Python arm
(README.md, "Verifying the arm"). Never shown to an author."""

from typing import Awaitable, Callable

from fastapi import FastAPI, Request, Response
from fastapi.responses import JSONResponse

ALLOW = "GET, HEAD, OPTIONS"
NO_STORE = "private, no-cache, no-store, max-age=0, must-revalidate"
HELD = ("badge", "button")
METHODS = ["GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH", "DELETE"]

app = FastAPI(docs_url=None, redoc_url=None, openapi_url=None, redirect_slashes=False)


def not_found() -> Response:
    return JSONResponse({"error": "Not found"}, status_code=404, headers={"cache-control": NO_STORE})


@app.middleware("http")
async def edge(request: Request, call_next: Callable[[Request], Awaitable[Response]]) -> Response:
    path = request.url.path
    if len(path) > 1 and path.endswith("/"):
        location = path[:-1] + (f"?{request.url.query}" if request.url.query else "")
        response: Response = Response(status_code=308, headers={"location": location})
    else:
        response = await call_next(request)
    response.headers["x-mzizi-source"] = "registry"
    return response


def other_method(method: str) -> Response:
    status = 204 if method == "OPTIONS" else 405
    return Response(status_code=status, headers={"allow": ALLOW})


@app.api_route("/v1/health", methods=METHODS)
async def health(request: Request) -> Response:
    if request.method in ("GET", "HEAD"):
        return JSONResponse({"status": "ok"})
    return other_method(request.method)


@app.api_route("/v1/ui", methods=METHODS)
async def ui_list(request: Request) -> Response:
    if request.method in ("GET", "HEAD"):
        return JSONResponse({"count": 2})
    return other_method(request.method)


@app.api_route("/v1/ui/{name}", methods=METHODS)
async def ui_item(name: str, request: Request) -> Response:
    if request.method in ("GET", "HEAD"):
        return JSONResponse({"name": name}) if name in HELD else not_found()
    return other_method(request.method)


@app.exception_handler(404)
async def missing(request: Request, exc: Exception) -> Response:
    return not_found()
