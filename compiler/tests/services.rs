//! RFC-0011: services, routes and handlers — the front end.
//!
//! One test (at least) per `MZ08xx` code, and for each reused code in its service form.
//! Where a diagnostic carries an `exact` fix, the test applies it blind and re-checks,
//! because that is what `exact` promises (RFC-0001 §4.3).

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::parse::Program;
use mzizi_lang_compiler::service::{Body, Method, Stmt};
use mzizi_lang_compiler::{apply_exact_fixes, check, check_contract, check_program};

/// A service with one record and one contract clause, around the given lines.
fn service(body: &str) -> String {
    format!(
        "service t\n  record problem\n    field error: text\n  end\n{body}  contract\n    example get \"/\" status is 404\n  end\nend service t\n"
    )
}

/// A route named `r` answering `get "/r"` with the given handler lines.
fn route(lines: &str) -> String {
    service(&format!("  route r\n    get \"/r\"\n{lines}  end\n"))
}

fn errors(src: &str) -> Vec<Diagnostic> {
    check(src, "t.mz")
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn one(src: &str, code: &str) -> Diagnostic {
    let found = errors(src);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {code}, got: {:#?}\n{src}",
        found
            .iter()
            .map(|d| format!("{} {}", d.code, d.say))
            .collect::<Vec<_>>()
    );
    assert_eq!(found[0].code, code, "{}", found[0].say);
    found.into_iter().next().unwrap()
}

fn clean(src: &str) {
    let found = errors(src);
    assert!(
        found.is_empty(),
        "expected no errors, got: {:#?}\n{src}",
        found
            .iter()
            .map(|d| format!("{} {}", d.code, d.say))
            .collect::<Vec<_>>()
    );
}

/// Exactly one error with this code and an `exact` fix; applying every exact fix leaves
/// the file clean.
fn fixes_clean(src: &str, code: &str) -> String {
    let d = one(src, code);
    let fix = d.fix.as_ref().expect("an exact fix");
    assert_eq!(fix.confidence, Confidence::Exact, "{}", d.say);
    let fixed = apply_exact_fixes(src, &check(src, "t.mz"));
    clean(&fixed);
    fixed
}

const B1: &str = r#"## B1's registry.
service registry

  header "x-mzizi-source" "fixture"

  record problem
    field error: text
  end

  record item
    field name: text
  end

  route ui_item
    get "/v1/ui/{name}"
    when name in "badge" "button"
      respond 200 json item name name
    end
    header "cache-control" "no-store"
    respond 404 json problem error "Not found"
  end

  fallback
    respond 404 json problem error "Not found"
  end

  contract
    example get "/v1/ui/button" status is 200
    ensure header "x-mzizi-source" is "fixture"
  end

end service registry
"#;

#[test]
fn the_rfc_example_checks_clean_and_parses_into_its_parts() {
    let (program, report) = check_program(B1, "registry.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    let Some(Program::Service(s)) = program else {
        panic!("a service")
    };
    assert_eq!(s.name, "registry");
    assert_eq!(s.headers.len(), 1);
    assert_eq!(s.records.len(), 2);
    assert_eq!(s.routes.len(), 1);
    let r = &s.routes[0];
    assert_eq!(r.method.map(|m| m.0), Some(Method::Get));
    assert_eq!(r.pattern.as_ref().unwrap().0, "/v1/ui/{name}");
    assert!(matches!(r.handler.body[0], Stmt::When { .. }));
    let Stmt::Respond(last) = r.handler.body.last().unwrap() else {
        panic!("ends with respond")
    };
    assert_eq!(last.status.map(|s| s.0), Some(404));
    assert!(matches!(last.body, Body::Json(_)));
    assert!(s.fallback.is_some());
    assert_eq!(s.contract.unwrap().clauses.len(), 2);
}

#[test]
fn a_component_file_still_parses_as_a_component() {
    let (program, _) = check_program("component a\nend component a\n", "a.mz");
    assert!(matches!(program, Some(Program::Component(_))));
}

#[test]
fn a_service_without_a_contract_is_the_mz0613_warning() {
    let src = "service t\n  route r\n    get \"/\"\n    respond 204\n  end\nend service t\n";
    let report = check(src, "t.mz");
    assert_eq!(report.error_count(), 0);
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(report.diagnostics[0].code, "MZ0613");
    assert_eq!(report.diagnostics[0].severity, Severity::Warning);
}

// MZ0801 ------------------------------------------------------------------------------

#[test]
fn a_route_without_a_method_line_is_mz0801() {
    one(&service("  route r\n    respond 200\n  end\n"), "MZ0801");
}

#[test]
fn an_upper_case_method_is_one_diagnostic_with_an_exact_fix() {
    let fixed = fixes_clean(
        &service("  route r\n    GET \"/r\"\n    respond 200\n  end\n"),
        "MZ0801",
    );
    assert!(fixed.contains("    get \"/r\""));
}

#[test]
fn an_unknown_method_near_a_real_one_is_only_a_guess() {
    let d = one(
        &service("  route r\n    fetch \"/r\"\n    respond 200\n  end\n"),
        "MZ0801",
    );
    assert!(
        d.fix
            .as_ref()
            .is_none_or(|f| f.confidence == Confidence::Guess)
    );
}

#[test]
fn a_second_method_line_is_mz0801() {
    one(&route("    post \"/r\"\n    respond 200\n"), "MZ0801");
}

// MZ0802 ------------------------------------------------------------------------------

#[test]
fn malformed_patterns_are_mz0802_with_exact_repairs() {
    for (bad, good) in [
        ("/v1/items/", "/v1/items"),
        ("v1/items", "/v1/items"),
        ("/v1//items", "/v1/items"),
        ("/v1/items?limit=3", "/v1/items"),
        ("/V1/Items", "/v1/items"),
    ] {
        let src = service(&format!(
            "  route r\n    get \"{bad}\"\n    respond 200\n  end\n"
        ));
        let fixed = fixes_clean(&src, "MZ0802");
        assert!(fixed.contains(&format!("get \"{good}\"")), "{bad}: {fixed}");
    }
}

#[test]
fn a_partial_segment_capture_is_mz0802_without_a_fix() {
    let d = one(
        &service("  route r\n    get \"/v1/nyuchi-{rest}\"\n    respond 200\n  end\n"),
        "MZ0802",
    );
    assert!(d.fix.is_none());
}

#[test]
fn a_parameter_bound_twice_is_mz0802() {
    one(
        &service("  route r\n    get \"/{a}/{a}\"\n    respond 200\n  end\n"),
        "MZ0802",
    );
}

#[test]
fn the_root_pattern_is_legal() {
    clean(&service(
        "  route r\n    get \"/\"\n    respond 200\n  end\n",
    ));
}

// MZ0803 ------------------------------------------------------------------------------

#[test]
fn two_routes_answering_the_same_request_are_mz0803() {
    one(
        &service(
            "  route a\n    get \"/v1/{x}\"\n    respond 200\n  end\n  route b\n    get \"/v1/{y}\"\n    respond 200\n  end\n",
        ),
        "MZ0803",
    );
}

#[test]
fn the_same_pattern_under_two_methods_is_two_routes() {
    clean(&service(
        "  route a\n    get \"/v1\"\n    respond 200\n  end\n  route b\n    post \"/v1\"\n    respond 201\n  end\n",
    ));
}

// MZ0804 / MZ0805 ---------------------------------------------------------------------

#[test]
fn a_handler_with_no_respond_is_mz0804() {
    one(&route("    header \"a\" \"b\"\n"), "MZ0804");
}

#[test]
fn a_when_without_else_does_not_cover_the_false_path() {
    let src = service(
        "  route r\n    get \"/r\"\n    query q: option(bool)\n    when q is none\n      respond 400\n    end\n  end\n",
    );
    one(&src, "MZ0804");
}

#[test]
fn a_when_that_responds_on_both_branches_ends_the_path() {
    clean(&service(
        "  route r\n    get \"/r\"\n    query q: option(bool)\n    when q is none\n      respond 400\n    else\n      respond 200\n    end\n  end\n",
    ));
}

#[test]
fn a_statement_after_respond_is_mz0805_and_its_fix_deletes_it() {
    let fixed = fixes_clean(
        &route("    respond 200\n    header \"a\" \"b\"\n"),
        "MZ0805",
    );
    assert!(!fixed.contains("header \"a\""));
}

#[test]
fn an_unreachable_when_block_is_deleted_whole() {
    let fixed = fixes_clean(
        &route("    respond 200\n    when x\n      respond 201\n    end\n"),
        "MZ0805",
    );
    assert!(!fixed.contains("201"));
}

// MZ0806 ------------------------------------------------------------------------------

#[test]
fn a_status_out_of_range_is_mz0806() {
    one(&route("    respond 700\n"), "MZ0806");
}

#[test]
fn a_respond_without_a_status_is_mz0806() {
    one(&route("    respond json problem error \"x\"\n"), "MZ0806");
}

#[test]
fn a_body_on_204_is_mz0806_and_its_fix_deletes_the_body() {
    let fixed = fixes_clean(&route("    respond 204 text \"gone\"\n"), "MZ0806");
    assert!(fixed.contains("    respond 204\n"));
}

#[test]
fn a_record_without_its_json_word_is_repaired_exactly() {
    let fixed = fixes_clean(&route("    respond 404 problem error \"x\"\n"), "MZ0806");
    assert!(fixed.contains("respond 404 json problem error \"x\""));
}

#[test]
fn an_unknown_body_form_is_mz0806() {
    one(&route("    respond 200 html \"<p>\"\n"), "MZ0806");
}

// MZ0807 ------------------------------------------------------------------------------

#[test]
fn an_upper_case_header_name_is_repaired_to_lower_case() {
    let fixed = fixes_clean(
        &route("    header \"Cache-Control\" \"no-store\"\n    respond 200\n"),
        "MZ0807",
    );
    assert!(fixed.contains("header \"cache-control\""));
}

#[test]
fn a_header_the_server_owns_is_mz0807() {
    one(
        &route("    header \"content-length\" \"3\"\n    respond 200\n"),
        "MZ0807",
    );
}

#[test]
fn a_header_set_twice_on_one_path_is_mz0807() {
    one(
        &route("    header \"a\" \"1\"\n    header \"a\" \"2\"\n    respond 200\n"),
        "MZ0807",
    );
}

#[test]
fn a_header_with_an_equals_sign_is_repaired() {
    fixes_clean(
        &route("    header \"a\" = \"1\"\n    respond 200\n"),
        "MZ0807",
    );
}

// MZ0808 ------------------------------------------------------------------------------

#[test]
fn a_record_literal_missing_a_field_is_mz0808() {
    one(&route("    respond 200 json problem\n"), "MZ0808");
}

#[test]
fn a_misspelt_field_is_one_mz0708_not_also_a_missing_field() {
    let d = one(
        &route("    respond 200 json problem eror \"x\"\n"),
        "MZ0708",
    );
    assert_eq!(d.fix.unwrap().replace, "error");
}

#[test]
fn a_field_of_the_wrong_type_is_mz0808() {
    one(&route("    respond 200 json problem error 3\n"), "MZ0808");
}

#[test]
fn a_field_given_twice_is_mz0808() {
    one(
        &route("    respond 200 json problem error \"a\" error \"b\"\n"),
        "MZ0808",
    );
}

#[test]
fn a_list_field_cannot_be_built_literally_yet() {
    let src = "service t\n  record many\n    field names: list(text)\n  end\n  route r\n    get \"/r\"\n    respond 200 json many names \"x\"\n  end\n  contract\n    example get \"/r\" status is 200\n  end\nend service t\n";
    one(src, "MZ0808");
}

// MZ0809 ------------------------------------------------------------------------------

#[test]
fn a_query_that_cannot_be_absent_is_wrapped_in_option() {
    let fixed = fixes_clean(
        &service("  route r\n    get \"/r\"\n    query limit: int\n    respond 200\n  end\n"),
        "MZ0809",
    );
    assert!(fixed.contains("query limit: option(int)"));
}

#[test]
fn a_list_query_is_mz0809() {
    one(
        &service("  route r\n    get \"/r\"\n    query tags: list(text)\n    respond 200\n  end\n"),
        "MZ0809",
    );
}

#[test]
fn a_query_type_from_another_language_is_mz0701_with_its_mzizi_name() {
    let fixed = fixes_clean(
        &service(
            "  route r\n    get \"/r\"\n    query q: option(string)\n    respond 200\n  end\n",
        ),
        "MZ0701",
    );
    assert!(fixed.contains("query q: option(text)"));
}

// MZ0810 ------------------------------------------------------------------------------

#[test]
fn a_missing_fixture_is_mz0810_and_a_present_one_is_not() {
    let dir = std::env::temp_dir().join(format!("mz-services-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("fixtures")).unwrap();
    std::fs::write(dir.join("fixtures/ui.json"), "{}").unwrap();
    let file = dir.join("t.mz");
    let file = file.to_str().unwrap();
    let present = route("    respond 200 file \"fixtures/ui.json\"\n");
    assert_eq!(check(&present, file).error_count(), 0);
    let missing = route("    respond 200 file \"fixtures/nope.json\"\n");
    let report = check(&missing, file);
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.diagnostics[0].code, "MZ0810");
    let escape = route("    respond 200 file \"../t.mz\"\n");
    assert_eq!(check(&escape, file).diagnostics[0].code, "MZ0810");
    std::fs::remove_dir_all(dir).unwrap();
}

// MZ0811 / MZ0812 ---------------------------------------------------------------------

#[test]
fn component_declarations_in_a_service_are_mz0811() {
    for line in ["  prop x: int\n", "  view\n    row\n    end\n  end\n"] {
        one(&service(line), "MZ0811");
    }
}

#[test]
fn return_is_mz0811_naming_respond() {
    let d = errors(&route("    return 200\n    respond 200\n"));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].code, "MZ0811");
    assert!(d[0].say.contains("respond"));
}

#[test]
fn a_comparison_with_equals_is_repaired_to_is() {
    let src = service(
        "  route r\n    get \"/{name}\"\n    when name == \"a\"\n      respond 200\n    end\n    respond 404\n  end\n",
    );
    let fixed = fixes_clean(&src, "MZ0811");
    assert!(fixed.contains("when name is \"a\""));
}

#[test]
fn duplicates_among_routes_and_parameters_are_mz0812() {
    one(
        &service(
            "  route r\n    get \"/a\"\n    respond 200\n  end\n  route r\n    get \"/b\"\n    respond 200\n  end\n",
        ),
        "MZ0812",
    );
    one(
        &service(
            "  route r\n    get \"/{id}\"\n    query id: option(text)\n    respond 200\n  end\n",
        ),
        "MZ0812",
    );
    one(
        &service("  fallback\n    respond 404\n  end\n  fallback\n    respond 404\n  end\n"),
        "MZ0812",
    );
}

// Reused codes ------------------------------------------------------------------------

#[test]
fn an_unknown_name_is_mz0707_with_a_nearest_fix() {
    let src = service(
        "  route r\n    get \"/{name}\"\n    when nme is \"a\"\n      respond 200\n    end\n    respond 404\n  end\n",
    );
    let fixed = fixes_clean(&src, "MZ0707");
    assert!(fixed.contains("when name is"));
}

#[test]
fn an_unnarrowed_option_is_mz0710_and_the_else_narrows_it() {
    one(
        &service(
            "  route r\n    get \"/r\"\n    query q: option(text)\n    respond 200 text \"{q}\"\n  end\n",
        ),
        "MZ0710",
    );
    clean(&service(
        "  route r\n    get \"/r\"\n    query q: option(text)\n    when q is none\n      respond 400\n    else\n      respond 200 text \"{q}\"\n    end\n  end\n",
    ));
}

#[test]
fn a_condition_that_does_not_fit_its_parameter_is_mz0712() {
    one(
        &service(
            "  route r\n    get \"/{name}\"\n    when name at_least 3\n      respond 200\n    end\n    respond 404\n  end\n",
        ),
        "MZ0712",
    );
}

#[test]
fn an_interpolated_unknown_name_points_at_the_name_inside_the_string() {
    let src = service("  route r\n    get \"/{name}\"\n    respond 200 text \"hi {nme}\"\n  end\n");
    let d = one(&src, "MZ0707");
    // `    respond 200 text "hi {nme}"`: the name starts at column 27.
    assert_eq!(d.span.start_col, 27);
    let fixed = apply_exact_fixes(&src, &check(&src, "t.mz"));
    assert!(fixed.contains("\"hi {name}\""), "{fixed}");
}

// Contracts at check time --------------------------------------------------------------

#[test]
fn an_example_that_contradicts_an_ensure_is_mz0606() {
    let src = "service t\n  route r\n    get \"/r\"\n    respond 200\n  end\n  contract\n    example get \"/r\" status is 201\n    ensure status in 200 404\n  end\nend service t\n";
    one(src, "MZ0606");
}

#[test]
fn a_check_without_its_is_is_repaired_exactly() {
    let src = "service t\n  route r\n    get \"/r\"\n    respond 200\n  end\n  contract\n    example get \"/r\" status 200\n  end\nend service t\n";
    fixes_clean(src, "MZ0602");
}

#[test]
fn a_predicate_that_does_not_fit_its_facet_is_mz0601() {
    let src = "service t\n  route r\n    get \"/r\"\n    respond 200\n  end\n  contract\n    example get \"/r\" status is \"ok\"\n  end\nend service t\n";
    one(src, "MZ0601");
}

#[test]
fn until_services_run_every_clause_is_not_yet_testable() {
    let (report, tally) = check_contract(B1, "registry.mz");
    assert_eq!(tally.clauses, 2);
    assert_eq!(tally.failed, 2);
    assert!(report.diagnostics.iter().all(|d| d.code == "MZ0607"));
}

// Recovery ----------------------------------------------------------------------------

#[test]
fn an_unclosed_route_is_one_mz0204_and_no_cascade() {
    let src = "service t\n  route a\n    get \"/a\"\n    when x\n      respond 200\n\n  route b\n    get \"/b\"\n    respond 200\n  end\n  contract\n    example get \"/b\" status is 200\n  end\nend service t\n";
    let found = errors(src);
    let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
    // `x` names nothing (MZ0707) is its own mistake; the unclosed blocks are one MZ0204.
    assert_eq!(codes, ["MZ0707", "MZ0204"], "{found:#?}");
}

#[test]
fn end_service_with_a_route_open_is_one_diagnostic() {
    let src = "service t\n  route a\n    get \"/a\"\n    respond 200\nend service t\n";
    one(src, "MZ0204");
}

#[test]
fn a_missing_end_service_has_an_exact_fix() {
    let src = "service t\n  route a\n    get \"/a\"\n    respond 200\n  end\n  contract\n    example get \"/a\" status is 200\n  end\n";
    fixes_clean(src, "MZ0204");
}

#[test]
fn no_two_exact_fixes_overlap_and_mz_fix_never_adds_errors() {
    let bad = [
        route("    respond 204 text \"x\"\n    header \"A\" \"b\"\n"),
        service(
            "  route r\n    GET \"/r/\"\n    query l: int\n    respond 200 problem eror \"x\"\n  end\n",
        ),
        service(
            "  route r\n    get \"v1\"\n    when nme == \"a\"\n      respond 200\n    end\n    respond 404\n  end\n",
        ),
    ];
    for src in &bad {
        let report = check(src, "t.mz");
        let exact: Vec<_> = report
            .diagnostics
            .iter()
            .filter_map(|d| d.fix.as_ref().filter(|f| f.confidence == Confidence::Exact))
            .collect();
        for (i, a) in exact.iter().enumerate() {
            for b in &exact[i + 1..] {
                assert!(!a.span.overlaps(&b.span), "{a:?} overlaps {b:?}\n{src}");
            }
        }
        let fixed = apply_exact_fixes(src, &report);
        assert!(
            check(&fixed, "t.mz").error_count() < report.error_count(),
            "{src}\n---\n{fixed}"
        );
    }
}
