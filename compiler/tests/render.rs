//! The static renderer (RFC-0017): the element tree as a value, its serialisation with
//! escaping, typed props, and the primitives it renders.
//!
//! The expected markup is written out here, in full, by hand from the primitive's `.mz`
//! file, not copied from the renderer's output: each string is what the file says should be
//! on the page.

use std::path::PathBuf;

use mzizi_lang_compiler::ast::Component;
use mzizi_lang_compiler::check_with_ast;
use mzizi_lang_compiler::render::{Node, RenderError, Value, escape, render, to_html, tree};

fn load(path: &str) -> Component {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(path);
    let src =
        std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("cannot read {full:?}: {e}"));
    let (component, report) = check_with_ast(&src, path);
    assert_eq!(report.error_count(), 0, "{path} must check clean");
    component.unwrap_or_else(|| panic!("{path} is not a component"))
}

fn var(name: &str) -> Value {
    Value::Variant(name.to_string())
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

#[test]
fn escape_covers_the_five_characters_and_leaves_the_rest() {
    assert_eq!(
        escape("a&b<c>d\"e'f ünï"),
        "a&#38;b&#60;c&#62;d&#34;e&#39;f ünï"
    );
    assert_eq!(escape(""), "");
}

#[test]
fn serialising_a_tree_escapes_text_and_attribute_values() {
    let nodes = vec![Node::Element {
        tag: "span",
        attrs: vec![("class".to_string(), "a\"b".to_string())],
        children: vec![Node::Text("<x>".to_string())],
    }];
    assert_eq!(
        to_html(&nodes),
        "<span class=\"a&#34;b\">&#60;x&#62;</span>"
    );
}

#[test]
fn separator_renders_its_defaults() {
    let c = load("primitives/separator.mz");
    assert_eq!(
        render(&c, &[]).unwrap(),
        "<div data-slot=\"separator\" data-portal=\"https://mzizi.dev/components/separator\" \
         data-axis=\"horizontal\" data-decorative=\"true\" class=\"shrink-0 bg-border h-px w-full\"></div>"
    );
}

#[test]
fn separator_takes_a_typed_variant_and_bool() {
    let c = load("primitives/separator.mz");
    let html = render(
        &c,
        &[
            ("axis", var("vertical")),
            ("decorative", Value::Bool(false)),
        ],
    )
    .unwrap();
    assert_eq!(
        html,
        "<div data-slot=\"separator\" data-portal=\"https://mzizi.dev/components/separator\" \
         data-axis=\"vertical\" data-decorative=\"false\" class=\"shrink-0 bg-border w-px self-stretch\"></div>"
    );
}

#[test]
fn badge_renders_its_label_as_escaped_text() {
    let c = load("primitives/badge.mz");
    let html = render(&c, &[("label", text("A & <B>"))]).unwrap();
    assert!(html.starts_with("<span data-slot=\"badge\" "), "{html}");
    assert!(html.contains(" data-variant=\"default\" "), "{html}");
    assert!(
        html.contains("bg-primary text-primary-foreground\">A &#38; &#60;B&#62;</span>"),
        "{html}"
    );
}

#[test]
fn an_attribute_value_is_escaped_after_interpolation() {
    // The variant's own column holds a quote-free string, so the hostile text arrives
    // through a prop: spinner's `announce` lands in a `label` attribute.
    let c = load("primitives/spinner.mz");
    let html = render(&c, &[("announce", text("\"><script>"))]).unwrap();
    assert!(
        html.contains("data-label=\"&#34;&#62;&#60;script&#62;\""),
        "{html}"
    );
    assert!(!html.contains("<script>"));
}

#[test]
fn a_when_picks_its_branch_and_an_absent_option_falls_to_else() {
    let c = load("primitives/avatar.mz");
    let fallback = render(
        &c,
        &[
            ("initials", text("NK")),
            ("alt", text("Nyasha")),
            ("image", Value::None),
        ],
    )
    .unwrap();
    assert!(
        fallback.contains("data-slot=\"avatar-fallback\""),
        "{fallback}"
    );
    assert!(fallback.contains(">NK</div>"), "{fallback}");
    assert!(!fallback.contains("<img"), "{fallback}");

    let image = render(
        &c,
        &[
            ("initials", text("NK")),
            ("alt", text("Nyasha")),
            ("image", text("/n.png")),
        ],
    )
    .unwrap();
    assert!(
        image.contains("<img data-slot=\"avatar-image\" src=\"/n.png\" alt=\"Nyasha\""),
        "{image}"
    );
    assert!(image.ends_with("></figure>"), "{image}");
    assert!(!image.contains("avatar-fallback"), "{image}");
}

#[test]
fn the_tree_is_a_value_you_can_inspect() {
    let c = load("primitives/separator.mz");
    let nodes = tree(&c, &[]).unwrap();
    assert_eq!(nodes.len(), 1);
    match &nodes[0] {
        Node::Element {
            tag,
            attrs,
            children,
        } => {
            assert_eq!(*tag, "div");
            assert!(children.is_empty());
            assert_eq!(attrs[0], ("data-slot".to_string(), "separator".to_string()));
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn rendering_is_deterministic() {
    let c = load("primitives/alert.mz");
    let props = [
        ("title", text("Saved")),
        ("message", text("All changes stored.")),
    ];
    assert_eq!(render(&c, &props).unwrap(), render(&c, &props).unwrap());
}

#[test]
fn alert_role_comes_from_the_variant_table() {
    let c = load("primitives/alert.mz");
    let props = |v: &str| {
        vec![
            ("variant", var(v)),
            ("title", text("t")),
            ("message", text("m")),
        ]
    };
    let loud = render(&c, &props("destructive")).unwrap();
    let quiet = render(&c, &props("info")).unwrap();
    assert!(loud.contains(" role=\"alert\" "), "{loud}");
    assert!(quiet.contains(" role=\"status\" "), "{quiet}");
}

#[test]
fn a_wrong_or_missing_prop_is_refused() {
    let c = load("primitives/separator.mz");
    assert_eq!(
        render(&c, &[("axis", var("diagonal"))]),
        Err(RenderError::UnknownVariant {
            prop: "axis".into(),
            variant: "diagonal".into()
        })
    );
    assert!(matches!(
        render(&c, &[("axis", Value::Bool(true))]),
        Err(RenderError::WrongType { .. })
    ));
    assert!(matches!(
        render(&c, &[("size", Value::Int(1))]),
        Err(RenderError::UnknownProp(_))
    ));
    let badge = load("primitives/badge.mz");
    assert_eq!(
        render(&badge, &[]),
        Err(RenderError::MissingProp("label".into()))
    );
}

#[test]
fn what_is_not_rendered_yet_says_so() {
    let input = load("primitives/input.mz");
    let err = render(
        &input,
        &[
            ("value", text("v")),
            ("placeholder", text("p")),
            ("on_change", text("x")),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, RenderError::Unsupported(_)), "{err:?}");
    let bar = load("primitives/confirm_bar.mz");
    assert!(render(&bar, &[]).is_err());
}

#[test]
fn event_wires_are_left_out_of_the_markup() {
    let c = load("primitives/input.mz");
    let html = render(&c, &[("value", text("v")), ("placeholder", text("p"))]).unwrap();
    assert!(!html.contains("change"), "{html}");
    assert!(!html.contains("on-change"), "{html}");
}

#[test]
fn progress_offsets_the_indicator_by_integer_arithmetic_in_the_view() {
    let c = load("primitives/progress.mz");
    let at = |v: i64| render(&c, &[("value", Value::Int(v))]).unwrap();
    assert!(
        at(40).contains("style=\"transform: translateX(-60%);\""),
        "{}",
        at(40)
    );
    assert!(at(40).contains("aria-valuenow=\"40\""), "{}", at(40));
    assert!(at(0).contains("translateX(-100%)"));
    assert!(at(100).contains("translateX(-0%)"));
    // Out of range is clamped by min and max, not rendered as a negative offset.
    assert!(at(150).contains("translateX(-0%)"));
    assert!(at(-5).contains("translateX(-100%)"));
    // The default is 0.
    assert!(render(&c, &[]).unwrap().contains("aria-valuenow=\"0\""));
}

#[test]
fn arithmetic_that_cannot_be_computed_is_refused_not_rendered() {
    let src = "component t\n  prop n: int = 0\n  view\n    row\n      class = \"{10 / n}\"\n    end\n  end\n  contract\n  end\nend component t\n";
    let (c, report) = check_with_ast(src, "t.mz");
    assert_eq!(report.error_count(), 0);
    let c = c.unwrap();
    assert!(matches!(render(&c, &[]), Err(RenderError::Arithmetic(_))));
    assert_eq!(
        render(&c, &[("n", Value::Int(5))]).unwrap(),
        "<div class=\"2\"></div>"
    );
}
