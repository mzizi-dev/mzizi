//! RFC-0011 §6–§7: running a service in process, and the contract evaluator.
//!
//! The algorithm's steps each get a test through [`Runtime::handle`], and every contract
//! code a service can produce (`MZ0605`, `MZ0611`, `MZ0612`) is triggered by a mutation of
//! the example service, `examples/registry.mz`, which must itself evaluate clean.

use mzizi_lang_compiler::check_contract;
use mzizi_lang_compiler::parse::{Program, parse_program};
use mzizi_lang_compiler::resolve::Ty;
use mzizi_lang_compiler::serve::{
    Json, Request, Response, Runtime, Val, allow, canonical_redirect, decode_query, parse_json,
    parse_query, request_set,
};
use mzizi_lang_compiler::service::{Method, Service};

const REGISTRY: &str = "../examples/registry.mz";

fn registry() -> (String, Service) {
    let src = std::fs::read_to_string(REGISTRY).unwrap();
    let (program, _) = parse_program(&src, REGISTRY);
    let Some(Program::Service(s)) = program else {
        panic!("a service")
    };
    (src, s)
}

fn get(s: &Service, method: &str, target: &str) -> Response {
    Runtime::new(s, REGISTRY).handle(&Request {
        method: method.to_string(),
        target: target.to_string(),
    })
}

fn body(r: &Response) -> String {
    String::from_utf8(r.body.clone()).unwrap()
}

#[test]
fn the_example_service_evaluates_clean() {
    let (src, _) = registry();
    let (report, tally) = check_contract(&src, REGISTRY);
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    assert_eq!(tally.clauses, 22);
    assert_eq!(tally.failed, 0);
    assert_eq!(tally.tested, Some(61));
}

#[test]
fn a_route_answers_with_its_json_and_the_service_header() {
    let (_, s) = registry();
    let r = get(&s, "GET", "/v1/ui/button");
    assert_eq!(r.status, 200);
    assert_eq!(body(&r), r#"{"name":"button"}"#);
    assert_eq!(r.header("content-type"), Some("application/json"));
    assert_eq!(r.header("x-mzizi-source"), Some("fixture"));
}

#[test]
fn a_fixture_body_is_served_byte_for_byte() {
    let (_, s) = registry();
    let r = get(&s, "GET", "/v1/ui");
    let want = std::fs::read("../examples/fixtures/ui.json").unwrap();
    assert_eq!(r.body, want);
}

#[test]
fn a_non_canonical_path_redirects_keeping_its_query_and_never_off_site() {
    assert_eq!(
        canonical_redirect("/v1/ui/", None).as_deref(),
        Some("/v1/ui")
    );
    assert_eq!(
        canonical_redirect("/v1/ui/", Some("a=1")).as_deref(),
        Some("/v1/ui?a=1")
    );
    assert_eq!(
        canonical_redirect("//evil.com/", None).as_deref(),
        Some("/evil.com")
    );
    assert_eq!(canonical_redirect("/a//b", None).as_deref(), Some("/a/b"));
    assert_eq!(canonical_redirect("/", None), None);
    assert_eq!(canonical_redirect("/v1/ui", None), None);
    let (_, s) = registry();
    let r = get(&s, "GET", "//evil.com/");
    assert_eq!(r.status, 308);
    assert_eq!(r.header("location"), Some("/evil.com"));
}

#[test]
fn head_is_get_without_a_body() {
    let (_, s) = registry();
    let g = get(&s, "GET", "/v1/ui/button");
    let h = get(&s, "HEAD", "/v1/ui/button");
    assert_eq!(h.status, g.status);
    assert_eq!(h.headers, g.headers);
    assert!(h.body.is_empty());
}

#[test]
fn options_and_undeclared_methods_carry_the_computed_allow() {
    assert_eq!(allow([Method::Get].into_iter()), "GET, HEAD, OPTIONS");
    assert_eq!(
        allow([Method::Delete, Method::Post].into_iter()),
        "POST, DELETE, OPTIONS"
    );
    let (_, s) = registry();
    let o = get(&s, "OPTIONS", "/v1/ui/button");
    assert_eq!(
        (o.status, o.header("allow")),
        (204, Some("GET, HEAD, OPTIONS"))
    );
    let d = get(&s, "DELETE", "/v1/ui/button");
    assert_eq!(
        (d.status, d.header("allow")),
        (405, Some("GET, HEAD, OPTIONS"))
    );
    assert!(d.body.is_empty());
    // An unknown method token is a 405 too.
    assert_eq!(get(&s, "BREW", "/v1/ui").status, 405);
}

#[test]
fn an_unmatched_path_runs_the_fallback() {
    let (_, s) = registry();
    let r = get(&s, "GET", "/nope/deeper");
    assert_eq!(r.status, 404);
    assert_eq!(body(&r), r#"{"error":"Not found"}"#);
}

#[test]
fn a_literal_segment_beats_a_parameter() {
    let src = "service t\n  route by_name\n    get \"/x/{name}\"\n    respond 200 text \"param {name}\"\n  end\n  route fixed\n    get \"/x/fixed\"\n    respond 200 text \"literal\"\n  end\n  contract\n    example get \"/x/fixed\" body is \"literal\"\n    example get \"/x/other\" body is \"param other\"\n    example post \"/x/fixed\" status is 405\n  end\nend service t\n";
    let (report, tally) = check_contract(src, "t.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    assert_eq!(tally.failed, 0);
}

#[test]
fn a_path_parameter_is_percent_decoded() {
    let src = "service t\n  route r\n    get \"/x/{name}\"\n    respond 200 text \"{name}\"\n  end\n  contract\n    example get \"/x/a%20b\" body is \"a b\"\n  end\nend service t\n";
    let (report, _) = check_contract(src, "t.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
}

#[test]
fn the_query_rule_is_rfc_0011_section_4_2() {
    assert_eq!(
        parse_query("a=1&b=x+y&c=%41&a=2&d"),
        vec![
            ("a".into(), "1".into()),
            ("b".into(), "x y".into()),
            ("c".into(), "A".into()),
            ("a".into(), "2".into()),
            ("d".into(), "".into()),
        ]
    );
    for (v, want) in [
        ("7", Val::Int(7)),
        ("-1", Val::Int(-1)),
        ("0", Val::Int(0)),
        ("0x10", Val::None),
        ("1e2", Val::None),
        (" 7 ", Val::None),
        ("1.5", Val::None),
        ("", Val::None),
        ("-", Val::None),
        ("99999999999999999999", Val::None),
    ] {
        assert_eq!(decode_query(&Ty::Int, v), want, "{v:?}");
    }
    assert_eq!(decode_query(&Ty::Bool, "true"), Val::Bool(true));
    assert_eq!(decode_query(&Ty::Bool, "1"), Val::None);
    assert_eq!(decode_query(&Ty::Text, ""), Val::Text(String::new()));
}

#[test]
fn the_first_occurrence_of_a_query_name_wins() {
    let src = "service t\n  route r\n    get \"/\"\n    query n: option(int)\n    when n is none\n      respond 200 text \"none\"\n    else\n      respond 200 text \"{n}\"\n    end\n  end\n  contract\n    example get \"/?n=3&n=4\" body is \"3\"\n    example get \"/?n=0x10\" body is \"none\"\n  end\nend service t\n";
    let (report, _) = check_contract(src, "t.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
}

#[test]
fn a_record_serialises_in_declaration_order_with_null_for_none() {
    let src = "service t\n  record page\n    field limit: option(int)\n    field note: text\n    field ok: bool\n  end\n  route r\n    get \"/\"\n    query limit: option(int)\n    respond 200 json page ok true note \"a\\b\" limit limit\n  end\nend service t\n";
    let (program, _) = parse_program(src, "t.mz");
    let Some(Program::Service(s)) = program else {
        panic!()
    };
    let r = Runtime::new(&s, "t.mz").handle(&Request {
        method: "GET".into(),
        target: "/?limit=4".into(),
    });
    assert_eq!(body(&r), r#"{"limit":4,"note":"a\\b","ok":true}"#);
    let r = Runtime::new(&s, "t.mz").handle(&Request {
        method: "GET".into(),
        target: "/".into(),
    });
    assert!(body(&r).starts_with(r#"{"limit":null,"#), "{}", body(&r));
}

#[test]
fn json_reads_back_what_it_should() {
    assert_eq!(
        parse_json(r#" {"a":[1,-2.5e3,true,null],"b":"é\n"} "#),
        Some(Json::Object(vec![
            (
                "a".into(),
                Json::Array(vec![
                    Json::Number("1".into()),
                    Json::Number("-2.5e3".into()),
                    Json::Bool(true),
                    Json::Null
                ])
            ),
            ("b".into(), Json::Str("é\n".into())),
        ]))
    );
    assert_eq!(parse_json("{"), None);
    assert_eq!(parse_json("[1,]"), None);
    assert_eq!(parse_json("1 2"), None);
}

#[test]
fn the_request_set_is_deterministic_and_covers_every_method() {
    let (_, s) = registry();
    let a = request_set(&s);
    let b = request_set(&s);
    assert_eq!(a, b);
    assert_eq!(a.len(), 61);
    for m in Method::ALL {
        assert!(
            a.iter()
                .any(|r| r.method == m.http() && r.target == "/v1/ui")
        );
    }
    // Each literal the handler compares `name` with, and one that matches none.
    for t in [
        "/v1/ui/badge",
        "/v1/ui/button",
        "/v1/ui/zz",
        "/v1/ui/",
        "//v1/ui",
        "/mz-unknown",
    ] {
        assert!(a.iter().any(|r| r.target == t), "{t}");
    }
}

fn mutate(from: &str, to: &str) -> String {
    let (src, _) = registry();
    assert!(src.contains(from), "{from}");
    src.replacen(from, to, 1)
}

fn codes(src: &str) -> Vec<&'static str> {
    check_contract(src, REGISTRY)
        .0
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn a_failed_example_is_mz0612() {
    let src = mutate(
        "respond 200 json item name name",
        "respond 200 json item name \"x\"",
    );
    assert_eq!(codes(&src), ["MZ0612"]);
}

#[test]
fn a_failed_ensure_is_mz0611_and_names_the_request() {
    // The fallback's JSON 404 loses its body: the `when status is 404` ensure breaks.
    // The fallback's 404 says something else: the `when status is 404` ensure breaks on
    // a path only the fallback answers.
    let src = mutate(
        "    respond 404 json problem error \"Not found\"\n  end\n\n  contract",
        "    respond 404 json problem error \"Gone\"\n  end\n\n  contract",
    );
    let (report, _) = check_contract(&src, REGISTRY);
    let ensure: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "MZ0611")
        .collect();
    assert_eq!(ensure.len(), 1, "{:#?}", report.diagnostics);
    assert!(ensure[0].say.contains("\"/\""), "{}", ensure[0].say);
}

#[test]
fn an_example_about_a_non_json_body_is_mz0605() {
    let src = mutate(
        "example get \"/v1/health\" body.status is \"ok\"",
        "example get \"/v1/ui/\" body.status is \"ok\"",
    );
    assert_eq!(codes(&src), ["MZ0605"]);
}

#[test]
fn a_file_that_does_not_compile_has_its_contract_skipped() {
    let src = mutate(
        "respond 200 json item name name",
        "respond 200 json item nam name",
    );
    let (report, tally) = check_contract(&src, REGISTRY);
    assert_eq!(tally.clauses, 0);
    assert!(report.diagnostics.iter().all(|d| d.code == "MZ0708"));
}

#[test]
fn the_agent_summary_carries_contract_tested() {
    let (src, _) = registry();
    let (report, tally) = check_contract(&src, REGISTRY);
    let out = report.to_ndjson_with(0, Some((tally.clauses, tally.failed)), tally.tested);
    assert!(
        out.ends_with(
            "\"contract_clauses\":22,\"contract_failures\":0,\"contract_tested\":61,\"ms\":0}\n"
        ),
        "{out}"
    );
}
