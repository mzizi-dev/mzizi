//! RFC-0008: types, collections, records, and the resolver that can say no.
//!
//! One test (at least) per diagnostic code and per grammar form. Where a diagnostic
//! carries an `exact` fix, the test applies it and re-checks: `exact` means "safe to apply
//! blind" (RFC-0001 §4.3), and the only honest test of that is to apply it blind.

use mzizi_lang_compiler::diagnostic::{Confidence, Diagnostic, Severity};
use mzizi_lang_compiler::ir::{Store, lower};
use mzizi_lang_compiler::outline::outline;
use mzizi_lang_compiler::{check, check_contract, check_with_ast};

/// Wrap body lines in a component with an empty contract, so the only diagnostics are the
/// ones under test (no MZ0501).
fn component(body: &str) -> String {
    format!("component t\n{body}  contract\n  end\nend component t\n")
}

fn diags(src: &str) -> Vec<Diagnostic> {
    check(src, "t.mz").diagnostics
}

fn errors(src: &str) -> Vec<Diagnostic> {
    diags(src)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// Exactly one error, with this code. Returns it.
fn one(src: &str, code: &str) -> Diagnostic {
    let found = errors(src);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {code}, got: {:#?}",
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

/// Apply every fix of the given confidence, last span first so earlier spans stay valid.
fn apply(src: &str, found: &[Diagnostic], want: Confidence) -> String {
    let mut lines: Vec<Vec<char>> = src.lines().map(|l| l.chars().collect()).collect();
    let mut fixes: Vec<_> = found
        .iter()
        .filter_map(|d| d.fix.clone())
        .filter(|f| f.confidence == want)
        .collect();
    fixes.sort_by_key(|f| std::cmp::Reverse((f.span.start_line, f.span.start_col)));
    for fix in fixes {
        // Flatten, splice, split: spans are 1-indexed, end-exclusive.
        let mut flat: Vec<char> = Vec::new();
        let mut offsets = Vec::new();
        for line in &lines {
            offsets.push(flat.len());
            flat.extend(line.iter());
            flat.push('\n');
        }
        let at = |line: u32, col: u32| offsets[(line - 1) as usize] + (col - 1) as usize;
        let start = at(fix.span.start_line, fix.span.start_col);
        let end = at(fix.span.end_line, fix.span.end_col);
        flat.splice(start..end, fix.replace.chars());
        let text: String = flat.into_iter().collect();
        lines = text.lines().map(|l| l.chars().collect()).collect();
    }
    let mut out: String = lines
        .into_iter()
        .map(|l| l.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// The diagnostic carries an `exact` fix, and applying it yields a file with no errors.
fn exact_fix_repairs(src: &str, code: &str) {
    let d = one(src, code);
    let fix = d
        .fix
        .as_ref()
        .unwrap_or_else(|| panic!("{code} must carry a fix"));
    assert_eq!(fix.confidence, Confidence::Exact, "{code}: {}", d.say);
    let repaired = apply(src, &[d], Confidence::Exact);
    clean(&repaired);
}

// ---------------------------------------------------------------------------------------
// G0.1 — the checker can fail on types and names
// ---------------------------------------------------------------------------------------

#[test]
fn prop_x_flarp_now_fails() {
    // The RFC-0008 §9.1 before/after. On main this printed `"errors":0`.
    let d = one(&component("  prop x: flarp\n"), "MZ0701");
    assert!(d.say.contains("`flarp`"), "{}", d.say);
    assert!(d.fix.is_none(), "nothing is near `flarp`: {:?}", d.fix);
}

#[test]
fn a_misspelt_type_gets_the_nearest_type_as_an_exact_fix() {
    exact_fix_repairs(&component("  prop x: txt\n"), "MZ0701");
    exact_fix_repairs(
        &component("  enum size\n    sm  class \"a\"\n  end\n  prop s: siz = sm\n"),
        "MZ0701",
    );
}

#[test]
fn another_languages_type_name_maps_to_the_mzizi_one() {
    exact_fix_repairs(&component("  prop x: string\n"), "MZ0701");
    exact_fix_repairs(&component("  prop n: number = 3\n"), "MZ0701");
    exact_fix_repairs(&component("  prop b: boolean = true\n"), "MZ0701");
}

#[test]
fn a_misspelt_prop_reference_is_caught_with_an_exact_fix() {
    let src =
        component("  prop label: text\n  view\n    row\n      text = lable\n    end\n  end\n");
    exact_fix_repairs(&src, "MZ0707");
}

#[test]
fn a_name_nothing_is_near_carries_no_fix() {
    let d = one(
        &component("  prop label: text\n  view\n    row\n      text = zzzzzz\n    end\n  end\n"),
        "MZ0707",
    );
    assert!(d.fix.is_none());
}

#[test]
fn a_misspelt_enum_column_is_caught() {
    let src = component(
        "  enum tone\n    calm  class \"a\"\n  end\n  prop t: tone = calm\n  view\n    row\n      class = \"x {t.clas}\"\n    end\n  end\n",
    );
    exact_fix_repairs(&src, "MZ0708");
}

#[test]
fn a_misspelt_variant_in_a_when_is_caught() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/connectivity_bar.mz"
    ))
    .unwrap()
    .replace("when state is offline", "when state is offlin");
    exact_fix_repairs(&src, "MZ0708");
    // `ofline` is one edit from `offline` *and* from `online`: a tie, so only a guess.
    let tied = src.replace("when state is offlin", "when state is ofline");
    let d = one(&tied, "MZ0708");
    assert_eq!(d.fix.unwrap().confidence, Confidence::Guess);
}

#[test]
fn rfc_0001s_worked_emit_diagnostic_is_now_real() {
    // RFC-0001 §4.2's example: `emit on_state_change(sync)` → nearest is `syncing`, exact.
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/connectivity_bar.mz"
    ))
    .unwrap()
    .replace(
        "emit on_state_change(syncing)",
        "emit on_state_change(sync)",
    );
    let d = one(&src, "MZ0708");
    assert!(
        d.say.contains("`sync`") && d.say.contains("syncing"),
        "{}",
        d.say
    );
    exact_fix_repairs(&src, "MZ0708");
}

#[test]
fn a_misspelt_enum_default_is_caught() {
    exact_fix_repairs(
        &component("  enum size\n    small  class \"a\"\n  end\n  prop s: size = smal\n"),
        "MZ0708",
    );
}

#[test]
fn defaults_are_type_checked() {
    one(&component("  prop on: bool = yes\n"), "MZ0706");
    one(&component("  prop n: int = \"3\"\n"), "MZ0706");
    // The quoted bool is mechanical.
    exact_fix_repairs(&component("  prop on: bool = \"true\"\n"), "MZ0706");
}

#[test]
fn a_dotted_access_on_something_without_members_is_caught() {
    one(
        &component(
            "  prop label: text\n  view\n    row\n      text = label.size\n    end\n  end\n",
        ),
        "MZ0709",
    );
}

#[test]
fn an_event_attribute_needs_an_event_or_a_fn() {
    one(
        &component("  prop label: text\n  view\n    control\n      tap = label\n    end\n  end\n"),
        "MZ0711",
    );
    one(
        &component("  view\n    control\n      tap = \"go\"\n    end\n  end\n"),
        "MZ0711",
    );
    clean(&component(
        "  prop on_tap: event(none)\n  view\n    control\n      tap = on_tap\n    end\n  end\n  fn go\n  end\n",
    ));
    clean(&component(
        "  view\n    control\n      tap = go\n    end\n  end\n  fn go\n  end\n",
    ));
}

#[test]
fn an_emit_must_name_an_event_prop_and_fit_its_payload() {
    one(
        &component("  prop label: text\n  fn go\n    emit label\n  end\n"),
        "MZ0715",
    );
    one(
        &component("  prop on_pick: event(text)\n  fn go\n    emit on_pick\n  end\n"),
        "MZ0715",
    );
    one(
        &component("  prop on_pick: event(text)\n  fn go\n    emit on_pick(3)\n  end\n"),
        "MZ0715",
    );
    // A payload on a bare signal is mechanical to remove.
    exact_fix_repairs(
        &component("  prop on_tap: event(none)\n  fn go\n    emit on_tap(3)\n  end\n"),
        "MZ0715",
    );
    exact_fix_repairs(
        &component("  prop on_tap: event(none)\n  fn go\n    emit on_tpa\n  end\n"),
        "MZ0715",
    );
    one(&component("  fn go\n    emit\n  end\n"), "MZ0405");
}

/// Review finding 3: `event(option(T))` was accepted, then every `emit` of it failed —
/// `none` included. The payload is checked against the event's type; for `option(T)`,
/// `none`, an `option(T)` value, and any `T` (wrapped implicitly) all fit.
#[test]
fn an_option_event_takes_none_or_a_value_of_its_inner_type() {
    let with = |emit: &str| {
        component(&format!(
            "  enum pick\n    a  k \"1\"\n    b  k \"2\"\n  end\n  prop chosen: pick = a\n  prop maybe: option(pick)\n  prop label: text\n  prop picked: event(option(pick))\n  fn go\n    {emit}\n  end\n"
        ))
    };
    for ok in [
        "emit picked(none)",
        "emit picked(b)",
        "emit picked(chosen)",
        "emit picked(maybe)",
    ] {
        clean(&with(ok));
    }
    let d = one(&with("emit picked(label)"), "MZ0715");
    assert!(d.say.contains("option(pick)"), "{}", d.say);
    exact_fix_repairs(&with("emit picked(bb)"), "MZ0708");
    one(&with("emit picked"), "MZ0715");
    clean(&component(
        "  prop on_name: event(option(text))\n  fn go\n    emit on_name(\"x\")\n    emit on_name(none)\n  end\n",
    ));
}

#[test]
fn a_bad_interpolation_is_one_diagnostic() {
    let with = |s: &str| {
        component(&format!(
            "  prop a: text\n  view\n    row\n      class = \"{s}\"\n    end\n  end\n"
        ))
    };
    one(&with("x {}"), "MZ0714");
    one(&with("x {a + a}"), "MZ0714");
    one(&with("x {a"), "MZ0714");
    clean(&with("x {a} y {a}"));
}

#[test]
fn a_camel_case_name_inside_an_interpolation_is_a_naming_error_with_its_fix() {
    exact_fix_repairs(
        &component(
            "  prop my_label: text\n  view\n    row\n      class = \"{myLabel}\"\n    end\n  end\n",
        ),
        "MZ0101",
    );
}

/// Review finding 4: `{item.Version}` snake-cased the whole string to `item._version`
/// and added an overlapping MZ0708. Each segment is snake-cased alone, and the one
/// mistake is one diagnostic.
#[test]
fn a_dotted_interpolation_is_snake_cased_per_segment_as_one_diagnostic() {
    let each = |interp: &str| {
        component(&format!(
            "  record entry\n    field version: text\n    field my_note: text\n  end\n  prop items: list(entry)\n  view\n    for each item in items\n      key = item.version\n      row\n        class = \"{interp}\"\n      end\n    end\n  end\n"
        ))
    };
    for bad in ["{item.Version}", "{item.myNote}"] {
        let src = each(bad);
        let all = diags(&src);
        assert_eq!(all.len(), 1, "{all:#?}");
        let fix = all[0].fix.as_ref().unwrap();
        assert!(!fix.replace.contains("._"), "{}", fix.replace);
        exact_fix_repairs(&src, "MZ0101");
    }
    assert_eq!(
        one(&each("{item.Version}"), "MZ0101").fix.unwrap().replace,
        "item.version"
    );
    // Wrong case *and* no such field: still one diagnostic, and its fix is only a guess.
    let d = one(&each("{item.Versoin}"), "MZ0101");
    assert_eq!(d.fix.unwrap().confidence, Confidence::Guess);
}

#[test]
fn an_enum_column_has_one_type() {
    one(
        &component("  enum size\n    sm  height 48\n    lg  height \"56\"\n  end\n"),
        "MZ0716",
    );
    one(
        &component("  enum size\n    sm  accent nowhere\n  end\n"),
        "MZ0716",
    );
}

#[test]
fn a_column_can_name_another_enums_variant_and_be_followed_through() {
    // `{n.accent.class}` — the changelog's node table, without enum payloads.
    clean(&component(
        "  enum accent\n    gold  class \"g\"\n  end\n  enum node\n    n1  accent gold\n  end\n  prop n: node = n1\n  view\n    row\n      class = \"{n.accent.class}\"\n    end\n  end\n",
    ));
}

#[test]
fn a_bare_variant_shared_by_two_enums_must_be_qualified() {
    let d = one(
        &component(
            "  enum a\n    x  k \"1\"\n  end\n  enum b\n    x  k \"2\"\n  end\n  view\n    row\n      v = x\n    end\n  end\n",
        ),
        "MZ0707",
    );
    assert!(d.say.contains("qualify"), "{}", d.say);
    clean(&component(
        "  enum a\n    x  k \"1\"\n  end\n  enum b\n    x  k \"2\"\n  end\n  view\n    row\n      v = a.x\n      w = \"{b.x.k}\"\n    end\n  end\n",
    ));
}

#[test]
fn another_components_variant_is_an_unchecked_warning_not_an_error() {
    let src =
        component("  view\n    button\n      variant = button_variant.ghost\n    end\n  end\n");
    clean(&src);
    let all = diags(&src);
    assert!(
        all.iter()
            .any(|d| d.code == "MZ0502" && d.say.contains("nothing to fix")),
        "{all:#?}"
    );
}

/// Review finding 2: `variant = tone.loud` with a local `prop tones` was a hard MZ0707. A
/// dotted head declared nowhere in this file is another component's enum until modules
/// say otherwise, so it is always the MZ0502 warning; a close local name rides along as a
/// `guess` fix and never makes it an error.
#[test]
fn an_external_head_near_a_local_name_is_a_warning_with_a_guess_not_an_error() {
    for src in [
        component(
            "  prop tones: text\n  view\n    button\n      variant = tone.loud\n    end\n  end\n",
        ),
        component("  prop label: text\n  view\n    row\n      text = lable.x\n    end\n  end\n"),
    ] {
        clean(&src);
        let warn: Vec<_> = diags(&src)
            .into_iter()
            .filter(|d| d.code == "MZ0502")
            .collect();
        assert_eq!(warn.len(), 1, "{warn:#?}");
        assert_eq!(warn[0].severity, Severity::Warning);
        assert_eq!(warn[0].fix.as_ref().unwrap().confidence, Confidence::Guess);
    }
}

#[test]
fn duplicates_and_built_in_names_are_caught() {
    one(&component("  prop a: text\n  prop a: text\n"), "MZ0704");
    one(
        &component("  record r\n    field a: text\n    field a: int\n  end\n"),
        "MZ0704",
    );
    one(
        &component("  record text\n    field a: int\n  end\n"),
        "MZ0704",
    );
    one(
        &component("  enum r\n    x  k \"1\"\n  end\n  record r\n    field a: int\n  end\n"),
        "MZ0704",
    );
}

#[test]
fn an_unknown_type_is_reported_once_not_at_every_use() {
    // RFC-0001 §4.1: one diagnostic per real error. Every use of `entry` sees the error
    // type and stays quiet.
    let src = component(
        "  prop entry: entyr\n  view\n    row\n      text = entry.title\n      class = \"{entry.a} {entry.b}\"\n    end\n  end\n",
    );
    one(&src, "MZ0701");
}

#[test]
fn a_broken_prop_line_does_not_make_its_uses_unknown_names() {
    let src = component("  prop label\n  view\n    row\n      text = label\n    end\n  end\n");
    one(&src, "MZ0305");
}

// ---------------------------------------------------------------------------------------
// Types: D1–D3
// ---------------------------------------------------------------------------------------

#[test]
fn the_closed_type_set_parses_and_resolves() {
    clean(&component(
        "  record r\n    field a: int\n  end\n  enum e\n    x  k \"1\"\n  end\n  prop a: bool\n  prop b: int\n  prop c: text\n  prop d: e\n  prop f: r\n  prop g: list(r)\n  prop h: option(text)\n  prop i: event(e)\n  prop j: event(none)\n  prop k: list(list(int))\n  prop l: event(list(r))\n",
    ));
}

#[test]
fn a_symbolic_type_constructor_is_one_diagnostic_with_the_mzizi_spelling() {
    for (bad, good) in [
        ("list<text>", "list(text)"),
        ("text[]", "list(text)"),
        ("[text]", "list(text)"),
        ("Option<text>", "option(text)"),
        ("Vec<int>", "list(int)"),
        ("list<option<text>>", "list(option(text))"),
    ] {
        let src = component(&format!("  prop x: {bad}\n"));
        let d = one(&src, "MZ0105");
        assert_eq!(d.fix.as_ref().unwrap().replace, good, "{bad}");
        exact_fix_repairs(&src, "MZ0105");
    }
}

/// Review findings 5 and 6: `list<entry[]>` fixed to `list(entry())` and then failed
/// MZ0309; `Option<Entry>` gave an MZ0105 and an MZ0101 whose exact fixes overlapped. The
/// whole type expression is now one diagnostic with one exact fix, nesting and case
/// included, and nothing inside its span is reported again.
#[test]
fn a_nested_or_camel_case_symbolic_type_is_one_diagnostic_and_one_fix() {
    for (bad, good) in [
        ("list<entry[]>", "list(list(entry))"),
        ("Option<Entry>", "option(entry)"),
        ("list<Entry>", "list(entry)"),
        ("[Entry]", "list(entry)"),
        ("Entry[]", "list(entry)"),
        ("entry[][]", "list(list(entry))"),
        ("[[entry]]", "list(list(entry))"),
        ("Vec<Option<Entry>>", "list(option(entry))"),
        ("list<option(entry)>", "list(option(entry))"),
    ] {
        let src = component(&format!(
            "  record entry\n    field a: text\n  end\n  prop x: {bad}\n"
        ));
        let all = diags(&src);
        assert_eq!(all.len(), 1, "{bad}: {all:#?}");
        let d = one(&src, "MZ0105");
        assert_eq!(d.fix.as_ref().unwrap().replace, good, "{bad}");
        exact_fix_repairs(&src, "MZ0105");
    }
}

/// Every `exact` fix in a file's diagnostic set can be applied blind, together, only if no
/// two of them touch the same text. Checked over the whole corpus under a set of
/// mutations that each break many lines at once, plus the reviewers' cases.
#[test]
fn no_two_exact_fixes_ever_overlap() {
    let mut fixtures: Vec<String> = Vec::new();
    for dir in ["examples", "primitives"] {
        let path = format!("{}/../{dir}", env!("CARGO_MANIFEST_DIR"));
        let mut files: Vec<_> = std::fs::read_dir(&path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "mz"))
            .collect();
        files.sort();
        for f in files {
            let src = std::fs::read_to_string(f).unwrap();
            let mutations: [&dyn Fn(&str) -> String; 8] = [
                &|s| s.replace(": text", ": Option<Text>"),
                &|s| s.replace(": text", ": list<String>"),
                &|s| s.replace(": text", ": Text[]"),
                &|s| s.replace("list(", "list<").replace("))", ")>"),
                &|s| s.replace("{", "{X"),
                &|s| s.replace(".", ".Z"),
                &|s| s.replace("prop ", "prop my").replace("field ", "field my"),
                &|s| s.replace("end\n", "\n"),
            ];
            fixtures.push(src.clone());
            fixtures.extend(mutations.iter().map(|m| m(&src)));
        }
    }
    for body in [
        "  prop x: list<string>\n",
        "  prop x: Vec<Option<String>>\n",
        "  record entry\n    field a: text\n  end\n  prop x: Option<Entry>\n  prop y: list<entry[]>\n",
        "  record entry\n    field version: text\n  end\n  prop items: list(entry)\n  view\n    for each item in items\n      key = item.version\n      row\n        class = \"{item.Version} {item.Versoin} {Item.version}\"\n      end\n    end\n  end\n",
    ] {
        fixtures.push(component(body));
    }
    for src in &fixtures {
        let exact: Vec<_> = diags(src)
            .into_iter()
            .filter_map(|d| d.fix.map(|f| (d.code, f)))
            .filter(|(_, f)| f.confidence == Confidence::Exact)
            .collect();
        for (i, (ca, a)) in exact.iter().enumerate() {
            for (cb, b) in &exact[i + 1..] {
                assert!(
                    !a.span.overlaps(&b.span),
                    "{ca} {:?} and {cb} {:?} overlap in:\n{src}",
                    a,
                    b
                );
            }
        }
    }
}

#[test]
fn a_question_mark_says_how_optional_is_spelt() {
    let found = errors(&component("  prop x?: text\n"));
    assert!(
        found
            .iter()
            .any(|d| d.code == "MZ0104" && d.say.contains("option(")),
        "{found:#?}"
    );
}

#[test]
fn a_type_built_wrongly_is_caught() {
    one(&component("  prop x: list\n"), "MZ0702");
    one(&component("  prop x: option\n"), "MZ0702");
    one(&component("  prop x: text(int)\n"), "MZ0702");
    one(&component("  prop x: none\n"), "MZ0702");
    one(&component("  prop x: list(event(none))\n"), "MZ0702");
    one(&component("  prop x: event(event(none))\n"), "MZ0702");
    one(
        &component("  record r\n    field cb: event(none)\n  end\n"),
        "MZ0702",
    );
    exact_fix_repairs(&component("  prop on_tap: event\n"), "MZ0702");
}

#[test]
fn there_is_one_way_to_say_nothing() {
    // D3: a list is never optional, an option never nested, an event never optional.
    for (bad, good) in [
        ("option(list(text))", "list(text)"),
        ("option(option(int))", "option(int)"),
        ("option(event(none))", "event(none)"),
    ] {
        let src = component(&format!("  prop x: {bad}\n"));
        assert_eq!(one(&src, "MZ0703").fix.unwrap().replace, good);
        exact_fix_repairs(&src, "MZ0703");
    }
}

#[test]
fn a_malformed_type_is_a_parse_error() {
    one(&component("  prop x: list()\n"), "MZ0309");
    exact_fix_repairs(&component("  prop x: list(text\n"), "MZ0309");
}

#[test]
fn leftovers_on_a_declaration_line_are_no_longer_ignored() {
    one(&component("  prop x: text garbage\n"), "MZ0310");
}

#[test]
fn list_and_option_defaults_are_implicit() {
    // One form each (RFC-0008 §1.2).
    exact_fix_repairs(&component("  prop xs: list(text) = none\n"), "MZ0706");
    exact_fix_repairs(&component("  prop x: option(text) = none\n"), "MZ0706");
    let src = component("  prop x: option(text) = \"hi\"\n");
    assert_eq!(
        one(&src, "MZ0706").fix.unwrap().replace,
        "text = \"hi\"",
        "an option with a default is not optional"
    );
    exact_fix_repairs(&src, "MZ0706");
    one(
        &component("  record r\n    field a: int\n  end\n  prop x: r = a\n"),
        "MZ0706",
    );
}

// ---------------------------------------------------------------------------------------
// G0.3 — records
// ---------------------------------------------------------------------------------------

#[test]
fn a_record_parses_resolves_and_reaches_its_fields() {
    let src = component(
        "  record entry\n    field version: text\n    field tags: list(text)\n  end\n  prop e: entry\n  view\n    row\n      text = e.version\n    end\n  end\n",
    );
    clean(&src);
    let (c, _) = check_with_ast(&src, "t.mz");
    let c = c.unwrap();
    assert_eq!(c.records.len(), 1);
    assert_eq!(c.records[0].fields[1].ty, "list(text)");
}

#[test]
fn a_record_closes_with_end_or_end_record() {
    clean(&component("  record r\n    field a: int\n  end record\n"));
}

#[test]
fn a_misspelt_record_field_is_caught() {
    exact_fix_repairs(
        &component(
            "  record entry\n    field version: text\n  end\n  prop e: entry\n  view\n    row\n      text = e.versoin\n    end\n  end\n",
        ),
        "MZ0708",
    );
}

#[test]
fn a_record_is_not_a_value() {
    one(
        &component(
            "  record entry\n    field version: text\n  end\n  prop e: entry\n  view\n    row\n      text = e\n    end\n  end\n",
        ),
        "MZ0711",
    );
}

#[test]
fn the_typescript_field_prior_costs_no_round_trip() {
    exact_fix_repairs(
        &component("  record r\n    version: text\n  end\n"),
        "MZ0308",
    );
    one(&component("  record r\n    view\n  end\n"), "MZ0308");
    one(&component("  record\n    field a: int\n  end\n"), "MZ0307");
    one(
        &component("  record r\n    field a: int = 3\n  end\n"),
        "MZ0310",
    );
}

#[test]
fn a_record_that_contains_itself_is_caught() {
    one(
        &component("  record node\n    field kids: list(node)\n  end\n"),
        "MZ0705",
    );
}

#[test]
fn field_is_still_an_element_word() {
    // `primitives/input.mz`'s view is a `field`; reserving the word would have broken it.
    clean(&component(
        "  view\n    field\n      slot = \"input\"\n    end\n  end\n",
    ));
}

// ---------------------------------------------------------------------------------------
// G0.2 + G0.5 — lists and `for each`
// ---------------------------------------------------------------------------------------

const ENTRIES: &str = "  record entry\n    field version: text\n    field title: text\n    field tags: list(text)\n    field added: option(text)\n  end\n  prop entries: list(entry)\n";

fn with_view(view: &str) -> String {
    component(&format!("{ENTRIES}  view\n{view}  end\n"))
}

#[test]
fn nested_for_each_over_a_field_resolves() {
    clean(&with_view(
        "    for each e in entries\n      key = e.version\n      article\n        text = e.title\n        for each tag in e.tags\n          key = tag\n          chip\n            text = \"#{tag}\"\n          end\n        end\n      end\n    end\n",
    ));
}

#[test]
fn a_binding_exists_only_inside_its_block() {
    one(
        &with_view(
            "    for each e in entries\n      key = e.version\n      row\n      end\n    end\n    row\n      text = e.title\n    end\n",
        ),
        "MZ0707",
    );
    // Using the record's type name where a value belongs is its own mistake.
    let d = one(
        &with_view(
            "    row
      text = entry.title
    end
",
        ),
        "MZ0707",
    );
    assert!(d.say.contains("record type"), "{}", d.say);
}

#[test]
fn a_list_is_not_a_value() {
    one(
        &with_view("    row\n      text = entries\n    end\n"),
        "MZ0711",
    );
    one(
        &with_view("    row\n      class = \"{entries}\"\n    end\n"),
        "MZ0711",
    );
}

#[test]
fn for_each_iterates_a_list_only() {
    one(
        &component(
            "  prop label: text\n  view\n    for each c in label\n      key = c\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0711",
    );
}

#[test]
fn a_for_each_key_is_written_once_one_way() {
    let body = |key: &str| {
        with_view(&format!(
            "    for each e in entries\n{key}      row\n        text = e.title\n      end\n    end\n"
        ))
    };
    // Missing: for a record there is no single obvious key, so no fix.
    let d = one(&body(""), "MZ0713");
    assert!(d.fix.is_none());
    // A string key is the Dioxus artefact; the path is the one form.
    exact_fix_repairs(&body("      key = \"{e.version}\"\n"), "MZ0713");
    // A key that does not use the binding is the same for every item.
    one(&body("      key = \"x\"\n"), "MZ0713");
    one(&body("      key = entries\n"), "MZ0713");
    // One key.
    one(
        &body("      key = e.version\n      key = e.title\n"),
        "MZ0713",
    );
    // A key is a scalar.
    one(&body("      key = e.tags\n"), "MZ0711");
    // Nothing else sits on a `for each`.
    one(
        &body("      key = e.version\n      class = \"x\"\n"),
        "MZ0713",
    );
}

#[test]
fn a_missing_key_on_a_scalar_list_suggests_the_item() {
    let src = with_view(
        "    for each e in entries\n      key = e.version\n      for each t in e.tags\n        chip\n          text = t\n        end\n      end\n    end\n",
    );
    let d = one(&src, "MZ0713");
    let fix = d.fix.expect("a scalar list's item is the natural key");
    assert_eq!(fix.confidence, Confidence::Guess);
    clean(&apply(&src, &[d_with(fix)], Confidence::Guess));

    fn d_with(fix: mzizi_lang_compiler::diagnostic::Fix) -> Diagnostic {
        Diagnostic::error("MZ0713", "t.mz", fix.span, "").with_fix(
            fix.span,
            fix.replace,
            fix.confidence,
        )
    }
}

#[test]
fn a_binding_may_not_shadow() {
    one(
        &with_view(
            "    for each entries in entries\n      key = entries.version\n      row\n      end\n    end\n",
        ),
        "MZ0713",
    );
}

#[test]
fn a_list_emptiness_test_is_is_none() {
    clean(&with_view(
        "    for each e in entries\n      key = e.version\n      when e.tags is none\n        nothing\n      else\n        row\n          label = \"Tags\"\n        end\n      end\n    end\n",
    ));
    one(
        &with_view(
            "    for each e in entries\n      key = e.version\n      when e.tags\n        row\n        end\n      end\n    end\n",
        ),
        "MZ0712",
    );
}

// ---------------------------------------------------------------------------------------
// G0.4 — options and narrowing
// ---------------------------------------------------------------------------------------

#[test]
fn an_option_narrows_inside_the_else_of_is_none() {
    clean(&with_view(
        "    for each e in entries\n      key = e.version\n      when e.added is none\n        nothing\n      else\n        row\n          text = e.added\n          class = \"a {e.added}\"\n        end\n      end\n    end\n",
    ));
}

#[test]
fn an_un_narrowed_option_is_a_diagnostic_not_a_blank() {
    // TY-6: in JSX `{entry.added}` on an absent value renders nothing, silently.
    let bare = with_view(
        "    for each e in entries\n      key = e.version\n      row\n        text = e.added\n      end\n    end\n",
    );
    one(&bare, "MZ0710");
    let interpolated = with_view(
        "    for each e in entries\n      key = e.version\n      row\n        class = \"{e.added}\"\n      end\n    end\n",
    );
    one(&interpolated, "MZ0710");
}

#[test]
fn narrowing_holds_in_the_else_branch_only() {
    one(
        &with_view(
            "    for each e in entries\n      key = e.version\n      when e.added is none\n        row\n          text = e.added\n        end\n      end\n    end\n",
        ),
        "MZ0710",
    );
}

#[test]
fn narrowing_is_keyed_on_the_path_as_written() {
    // Narrowing `e.added` narrows nothing else.
    let src = component(
        "  prop a: option(text)\n  prop b: option(text)\n  view\n    when a is none\n      nothing\n    else\n      row\n        text = b\n      end\n    end\n  end\n",
    );
    one(&src, "MZ0710");
}

#[test]
fn field_access_through_an_option_needs_narrowing() {
    let src = component(
        "  record r\n    field a: text\n  end\n  prop x: option(r)\n  view\n    row\n      text = x.a\n    end\n  end\n",
    );
    one(&src, "MZ0710");
    clean(&component(
        "  record r\n    field a: text\n  end\n  prop x: option(r)\n  view\n    when x is none\n      nothing\n    else\n      row\n        text = x.a\n      end\n    end\n  end\n",
    ));
}

#[test]
fn is_some_is_rewritten_to_the_one_form() {
    // What `primitives/avatar.mz` wrote before RFC-0008.
    let src = component(
        "  prop image: option(text)\n  view\n    when image is some\n      picture\n        source = image\n      end\n    end\n  end\n",
    );
    exact_fix_repairs(&src, "MZ0712");
}

#[test]
fn an_option_is_not_a_bool() {
    one(
        &component(
            "  prop x: option(text)\n  view\n    when x\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
    one(
        &component(
            "  prop x: option(text)\n  view\n    when not x\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
}

#[test]
fn a_bool_is_tested_one_way() {
    exact_fix_repairs(
        &component(
            "  prop on: bool\n  view\n    when on is true\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
    exact_fix_repairs(
        &component(
            "  prop on: bool\n  view\n    when on is false\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
}

#[test]
fn an_enum_is_never_none_and_text_is_never_absent() {
    one(
        &component(
            "  enum s\n    a  k \"1\"\n  end\n  prop x: s\n  view\n    when x is none\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
    let d = one(
        &component(
            "  prop x: text\n  view\n    when x is none\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
    assert!(d.say.contains("option(text)"), "{}", d.say);
}

#[test]
fn a_when_holds_elements_only() {
    one(
        &component(
            "  prop on: bool\n  view\n    when on\n      class = \"x\"\n      row\n      end\n    end\n  end\n",
        ),
        "MZ0712",
    );
}

// ---------------------------------------------------------------------------------------
// `else`
// ---------------------------------------------------------------------------------------

#[test]
fn else_serves_every_when() {
    clean(&component(
        "  prop on: bool\n  view\n    when on\n      row\n      end\n    else\n      nothing\n    end\n  end\n",
    ));
}

#[test]
fn else_belongs_to_when_once() {
    one(
        &component("  view\n    row\n    else\n    end\n  end\n"),
        "MZ0404",
    );
    one(
        &component(
            "  prop on: bool\n  view\n    when on\n      nothing\n    else\n      nothing\n    else\n      nothing\n    end\n  end\n",
        ),
        "MZ0404",
    );
}

#[test]
fn else_is_its_own_branch_for_contracts() {
    // `when on shows …` is about the branch the condition selects, not the `else`.
    let src = "component t\n  prop on: bool\n  view\n    when on\n      nothing\n    else\n      button\n        text = \"Go\"\n      end\n    end\n  end\n  contract\n    when on shows button \"Go\"\n  end\nend component t\n";
    let (report, tally) = check_contract(src, "t.mz");
    assert_eq!(tally.failed, 1, "{:#?}", report.diagnostics);
}

// ---------------------------------------------------------------------------------------
// IR, hash and outline
// ---------------------------------------------------------------------------------------

fn root_hash(src: &str) -> String {
    let (c, report) = check_with_ast(src, "t.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    let mut store = Store::new();
    lower(&c.unwrap(), &mut store).short()
}

#[test]
fn records_else_branches_and_emits_all_reach_the_hash() {
    let base = with_view(
        "    for each e in entries\n      key = e.version\n      when e.added is none\n        nothing\n      else\n        row\n          text = e.added\n        end\n      end\n    end\n",
    );
    let field_changed = base.replace("field title: text", "field heading: text");
    let else_changed = base.replace("          text = e.added\n", "          label = e.added\n");
    assert_ne!(root_hash(&base), root_hash(&field_changed));
    assert_ne!(root_hash(&base), root_hash(&else_changed));

    let emit = |v: &str| {
        component(&format!(
            "  enum s\n    a  k \"1\"\n    b  k \"2\"\n  end\n  prop on: event(s)\n  fn go\n    emit on({v})\n  end\n"
        ))
    };
    assert_ne!(
        root_hash(&emit("a")),
        root_hash(&emit("b")),
        "a fn that emits something else is a different component"
    );
}

#[test]
fn the_outline_carries_records_and_new_types_and_reparses() {
    let src = with_view("    row\n    end\n");
    let (c, _) = check_with_ast(&src, "t.mz");
    let text = outline(&c.unwrap());
    assert!(
        text.contains("record entry\n    field version: text"),
        "{text}"
    );
    assert!(text.contains("field added: option(text)"), "{text}");
    assert!(text.contains("prop entries: list(entry)"), "{text}");
    // Canonical order: the record precedes the prop that uses it.
    assert!(text.find("record entry").unwrap() < text.find("prop entries").unwrap());
    let report = check(&text, "outline.mz");
    assert_eq!(report.error_count(), 0, "{:#?}\n{text}", report.diagnostics);
}

// ---------------------------------------------------------------------------------------
// The protocol
// ---------------------------------------------------------------------------------------

#[test]
fn every_new_diagnostic_stays_inside_the_say_budget() {
    // RFC-0001 §4.2: `say` targets <= 200 characters.
    let long = "a_really_quite_long_name_that_an_agent_might_write_for_a_prop";
    let fixtures = [
        component("  prop x: flarp\n"),
        component(&format!(
            "  prop {long}: text\n  view\n    row\n      text = {long}.nope\n    end\n  end\n"
        )),
        component(&format!(
            "  record r\n    field {long}: int\n  end\n  prop x: option(r)\n  view\n    row\n      text = x.{long}\n    end\n  end\n"
        )),
        with_view("    for each e in entries\n      row\n      end\n    end\n"),
        component(
            "  view\n    button\n      variant = some_other_components_variant_enum.default_variant\n    end\n  end\n",
        ),
    ];
    for src in fixtures {
        for d in diags(&src) {
            assert!(
                d.say.chars().count() <= 200,
                "{} is {} chars: {}",
                d.code,
                d.say.chars().count(),
                d.say
            );
        }
    }
}

// ---------------------------------------------------------------------------------------
// The changelog port (RFC-0008 §9.3)
// ---------------------------------------------------------------------------------------

#[test]
fn the_changelog_port_checks_clean_and_keeps_its_contract() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/changelog_renderer.mz"
    ))
    .unwrap();
    let (report, tally) = check_contract(&src, "changelog_renderer.mz");
    assert_eq!(report.error_count(), 0, "{:#?}", report.diagnostics);
    assert!(tally.clauses >= 12 && tally.failed == 0, "{tally:?}");

    // It exercises what it was written to: a list of records, nested iteration over a
    // field of the outer binding, and the `is none … else` form.
    let (c, _) = check_with_ast(&src, "changelog_renderer.mz");
    let c = c.unwrap();
    assert_eq!(c.props[0].ty, "list(changelog_entry)");
    fn depth(els: &[mzizi_lang_compiler::ast::Element]) -> usize {
        els.iter()
            .map(|e| {
                let own = usize::from(e.tag == "for");
                let below =
                    depth(&e.children).max(e.else_children.as_deref().map(depth).unwrap_or(0));
                own + below
            })
            .max()
            .unwrap_or(0)
    }
    assert_eq!(depth(c.view.as_deref().unwrap()), 2, "nested for each");
}
