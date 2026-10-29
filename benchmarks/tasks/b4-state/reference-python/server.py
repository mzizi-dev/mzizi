"""B4's Python reference (benchmarks/tasks/b4-state/spec.md), standard library only."""

import json
import os
import re
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ALLOW = "GET, HEAD, POST, OPTIONS"
ROUTE = re.compile(r"^/v1/ui/([^/]+)/versions$")
HISTORY = {}
LOCK = threading.Lock()


def version_of(raw):
    """{"version": "<non-empty string>"}, else None."""
    try:
        value = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(value, dict):
        return None
    version = value.get("version")
    return version if isinstance(version, str) and version else None


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def send(self, status, headers=None, body=None):
        payload = b"" if body is None else json.dumps(body, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("x-mzizi-source", "registry")
        if body is not None:
            self.send_header("content-type", "application/json")
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(payload)

    def handle_any(self):
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b""
        path = self.path.split("?", 1)[0]
        m = ROUTE.match(path)
        if not m or m.group(1) not in ("badge", "button"):
            return self.send(404, body={"error": "Not found"})
        name = m.group(1)
        method = self.command
        if method == "OPTIONS":
            return self.send(204, {"allow": ALLOW})
        if method not in ("GET", "HEAD", "POST"):
            return self.send(405, {"allow": ALLOW})
        with LOCK:
            versions = HISTORY.setdefault(name, [])
            if method in ("GET", "HEAD"):
                return self.send(200, body={"component": name, "versions": versions, "count": len(versions)})
            version = version_of(raw)
            if version is None:
                return self.send(400, body={"error": "Invalid body"})
            if version in versions:
                return self.send(409, body={"error": "Version already recorded"})
            versions.append(version)
            self.send(
                201,
                {"location": f"/v1/ui/{name}/versions"},
                {"component": name, "version": version, "count": len(versions)},
            )

    do_GET = do_HEAD = do_POST = do_PUT = do_PATCH = do_DELETE = do_OPTIONS = handle_any


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8080"))
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"listening on http://127.0.0.1:{port}", flush=True)
    server.serve_forever()
