// Known-bad, for the check: a type error and an unused variable. `check.sh` must exit 1.
#include <string>

#include <httplib.h>

int main() {
  httplib::Server svr;
  int count = "2";
  std::string unused;
  svr.Get("/v1/ui", [](const httplib::Request&, httplib::Response& res) {
    res.set_content("{\"count\":2}", "application/json");
  });
  svr.listen("0.0.0.0", count);
  return 0;
}
