"""B5's Python reference (benchmarks/tasks/b5-errors/spec.md), standard library only."""

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BODY = 1024
GET_ALLOW = "GET, HEAD, OPTIONS"
POST_ALLOW = "POST, OPTIONS"
ROUTES = {
    "/v1/health": ("GET", GET_ALLOW),
    "/v1/fail": ("GET", GET_ALLOW),
    "/v1/messages": ("POST", POST_ALLOW),
}


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

    def error(self, status, message):
        self.send(status, body={"error": message})

    def messages(self, raw):
        media = (self.headers.get("content-type") or "").split(";")[0].strip().lower()
        if media != "application/json":
            return self.error(415, "Expected application/json")
        if len(raw) > MAX_BODY:
            return self.error(413, "Payload too large")
        try:
            value = json.loads(raw)
        except ValueError:
            return self.error(400, "Malformed JSON")
        text = value.get("text") if isinstance(value, dict) else None
        if not isinstance(text, str) or not text:
            return self.error(400, "Invalid message")
        self.send(201, body={"text": text})

    def handle_any(self):
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b""
        # http.server rewrites a target that starts with `//` in Python 3.13 (seen with
        # `//evil.com` on 3.13.3); the request line keeps what the client sent.
        target = self.requestline.split(" ")[1]
        path, _, query = target.partition("?")
        canonical = "/" + "/".join(s for s in path.split("/") if s)
        if canonical != path:
            return self.send(308, {"location": canonical + ("?" + query if "?" in target else "")})
        route = ROUTES.get(path)
        if route is None:
            return self.error(404, "Not found")
        method, allow = route
        if self.command == "OPTIONS":
            return self.send(204, {"allow": allow})
        if self.command == method or (method == "GET" and self.command == "HEAD"):
            if path == "/v1/health":
                return self.send(200, body={"status": "ok"})
            if path == "/v1/fail":
                print("internal error: the work behind /v1/fail broke", file=sys.stderr, flush=True)
                return self.error(500, "Internal server error")
            return self.messages(raw)
        self.send(405, {"allow": allow})

    do_GET = do_HEAD = do_POST = do_PUT = do_PATCH = do_DELETE = do_OPTIONS = handle_any


if __name__ == "__main__":
    port = int(os.environ.get("PORT", "8080"))
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"listening on http://127.0.0.1:{port}", flush=True)
    server.serve_forever()
