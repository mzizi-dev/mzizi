// A known-good B1 solution in Go with net/http, written to check the Go arm (README.md,
// "Verifying the arm"). Never shown to an author.
package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"strings"
)

const (
	allow   = "GET, HEAD, OPTIONS"
	noStore = "private, no-cache, no-store, max-age=0, must-revalidate"
)

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func notFound(w http.ResponseWriter) {
	w.Header().Set("cache-control", noStore)
	writeJSON(w, http.StatusNotFound, map[string]string{"error": "Not found"})
}

func handler(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("x-mzizi-source", "registry")
	path := r.URL.Path

	if len(path) > 1 && strings.HasSuffix(path, "/") {
		location := strings.TrimSuffix(path, "/")
		if r.URL.RawQuery != "" || r.URL.ForceQuery {
			location += "?" + r.URL.RawQuery
		}
		w.Header().Set("location", location)
		w.WriteHeader(http.StatusPermanentRedirect)
		return
	}

	name := ""
	known := false
	switch {
	case path == "/v1/health", path == "/v1/ui":
		known = true
	case strings.HasPrefix(path, "/v1/ui/") && len(path) > len("/v1/ui/") && !strings.Contains(path[len("/v1/ui/"):], "/"):
		known = true
		name = path[len("/v1/ui/"):]
	}
	if !known {
		notFound(w)
		return
	}

	switch r.Method {
	case http.MethodGet, http.MethodHead:
		switch {
		case path == "/v1/health":
			writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
		case path == "/v1/ui":
			writeJSON(w, http.StatusOK, map[string]int{"count": 2})
		case name == "badge" || name == "button":
			writeJSON(w, http.StatusOK, map[string]string{"name": name})
		default:
			notFound(w)
		}
	case http.MethodOptions:
		w.Header().Set("allow", allow)
		w.WriteHeader(http.StatusNoContent)
	default:
		w.Header().Set("allow", allow)
		w.WriteHeader(http.StatusMethodNotAllowed)
	}
}

func main() {
	addr := "0.0.0.0:" + os.Getenv("PORT")
	log.Fatal(http.ListenAndServe(addr, http.HandlerFunc(handler)))
}
