// Known-bad, for the probes: builds and vets clean, but serves only the three GET routes. It
// has no x-mzizi-source header, no HEAD or OPTIONS or 405 handling, and no trailing-slash
// redirect, so most of B1's probes must fail on it.
package main

import (
	"fmt"
	"log"
	"net/http"
	"os"
)

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("content-type", "application/json")
		fmt.Fprint(w, `{"status":"ok"}`)
	})
	mux.HandleFunc("GET /v1/ui", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("content-type", "application/json")
		fmt.Fprint(w, `{"count":2}`)
	})
	mux.HandleFunc("GET /v1/ui/{name}", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("content-type", "application/json")
		fmt.Fprintf(w, `{"name":%q}`, r.PathValue("name"))
	})
	log.Fatal(http.ListenAndServe("0.0.0.0:"+os.Getenv("PORT"), mux))
}
