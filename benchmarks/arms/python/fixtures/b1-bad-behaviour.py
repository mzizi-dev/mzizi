"""Known-bad, for the probes: type-checks clean, but serves only the three GET routes with
FastAPI's defaults. It has no x-mzizi-source header, no OPTIONS handling, a bare
Starlette 405 and 404, and a 307 for trailing slashes, so many of B1's probes must fail."""

from fastapi import FastAPI

app = FastAPI()


@app.get("/v1/health")
async def health() -> dict[str, str]:
    return {"status": "ok"}


@app.get("/v1/ui")
async def ui_list() -> dict[str, int]:
    return {"count": 2}


@app.get("/v1/ui/{name}")
async def ui_item(name: str) -> dict[str, str]:
    return {"name": name}
