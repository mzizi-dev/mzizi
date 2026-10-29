// Known-bad, for the check: a type error and an unused import. `check.sh` must exit 1.
package main

import (
	"log"
	"net/http"
	"os"
)

func main() {
	var count int = "2"
	http.HandleFunc("/v1/ui", func(w http.ResponseWriter, r *http.Request) {})
	log.Fatal(http.ListenAndServe("0.0.0.0:"+os.Getenv("PORT"), nil), count)
}
