// A known-good B1 solution in C++20 with cpp-httplib and nlohmann/json, written to check the
// C++ arm (README.md, "Verifying the arm"). Never shown to an author.
#include <cstdlib>
#include <string>

#include <httplib.h>
#include <nlohmann/json.hpp>

using json = nlohmann::json;

namespace {

const char* const kAllow = "GET, HEAD, OPTIONS";
const char* const kNoStore = "private, no-cache, no-store, max-age=0, must-revalidate";

void send_json(httplib::Response& res, int status, const json& body) {
  res.status = status;
  res.set_content(body.dump(), "application/json");
}

void not_found(httplib::Response& res) {
  res.set_header("cache-control", kNoStore);
  send_json(res, 404, {{"error", "Not found"}});
}

}  // namespace

int main() {
  httplib::Server svr;

  svr.set_pre_routing_handler([](const httplib::Request& req, httplib::Response& res) {
    res.set_header("x-mzizi-source", "registry");
    const std::string& path = req.path;

    if (path.size() > 1 && path.back() == '/') {
      std::string location = path.substr(0, path.size() - 1);
      if (!req.params.empty()) {
        location += "?" + httplib::detail::params_to_query_str(req.params);
      }
      res.status = 308;
      res.set_header("location", location);
      return httplib::Server::HandlerResponse::Handled;
    }

    const std::string items = "/v1/ui/";
    std::string name;
    bool known = path == "/v1/health" || path == "/v1/ui";
    if (path.rfind(items, 0) == 0 && path.size() > items.size() &&
        path.find('/', items.size()) == std::string::npos) {
      known = true;
      name = path.substr(items.size());
    }
    if (!known) {
      not_found(res);
      return httplib::Server::HandlerResponse::Handled;
    }

    if (req.method == "GET" || req.method == "HEAD") {
      if (path == "/v1/health") {
        send_json(res, 200, {{"status", "ok"}});
      } else if (path == "/v1/ui") {
        send_json(res, 200, {{"count", 2}});
      } else if (name == "badge" || name == "button") {
        send_json(res, 200, {{"name", name}});
      } else {
        not_found(res);
      }
    } else {
      res.status = req.method == "OPTIONS" ? 204 : 405;
      res.set_header("allow", kAllow);
    }
    return httplib::Server::HandlerResponse::Handled;
  });

  const char* port = std::getenv("PORT");
  svr.listen("0.0.0.0", port ? std::stoi(port) : 8080);
  return 0;
}
