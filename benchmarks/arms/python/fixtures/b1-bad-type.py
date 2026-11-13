"""Known-bad, for the check: a type error pyright reports. `check.sh` must exit 1."""

from fastapi import FastAPI

app = FastAPI()
count: int = "2"


@app.get("/v1/ui")
async def ui_list() -> dict[str, int]:
    return {"count": count}
