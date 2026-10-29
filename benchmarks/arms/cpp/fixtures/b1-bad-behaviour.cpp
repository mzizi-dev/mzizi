// Known-bad, for the probes: compiles clean, but serves only the three GET routes. It has
// no x-mzizi-source header, no OPTIONS or 405 handling, and no trailing-slash redirect,
// so most of B1's probes must fail on it.
#include <cstdlib>
#include <string>

#include <httplib.h>

int main() {
  httplib::Server svr;
  svr.Get("/v1/health", [](const httplib::Request&, httplib::Response& res) {
    res.set_content("{\"status\":\"ok\"}", "application/json");
  });
  svr.Get("/v1/ui", [](const httplib::Request&, httplib::Response& res) {
    res.set_content("{\"count\":2}", "application/json");
  });
  svr.Get(R"(/v1/ui/([^/]+))", [](const httplib::Request& req, httplib::Response& res) {
    res.set_content("{\"name\":\"" + req.matches[1].str() + "\"}", "application/json");
  });
  const char* port = std::getenv("PORT");
  svr.listen("0.0.0.0", port ? std::stoi(port) : 8080);
  return 0;
}
