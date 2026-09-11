//! Contract bodies: the model they parse into, and the evaluator that runs them.
//!
//! RFC-0001 §1.6 put behaviour assertions *in the language* because the charter's Phase 0
//! defect metric is "compiles cleanly but is behaviourally wrong" (CHARTER.md §6). Until
//! this module existed the compiler set a `has_contract: bool` and discarded the body, so
//! the one metric that decides whether Phase 1 starts had no toolchain behind it.
//!
//! The design decisions behind the clause grammar, the subject-resolution order, and what
//! this evaluator deliberately does *not* check are written down in
//! [RFC-0006](../../design/RFC-0006-contracts.md). Two of them matter for reading the code:
//!
//! - **Evaluation is static, against the component's own declarations.** A clause is
//!   checked against the enum tables, the view tree and the prop defaults in the same
//!   file — not against a rendered DOM and not against a reference implementation. That
//!   makes `mz contract` a *self*-consistency check; scoring a port against the corpus
//!   ground truth is a second, separate comparison the benchmark harness will do.
//! - **A clause that cannot be evaluated is an error, never a silent pass.** A contract
//!   that quietly does nothing is worse than no contract, because it reads as verification.

use crate::ast::{Component, Element, EnumDecl, Variant};
use crate::diagnostic::{Diagnostic, Span};

/// A parsed `contract` block: the assertions, in source order.
///
/// Order is retained for diagnostics only. A contract is a *set* of assertions — see
/// [`crate::ir::lower`], which sorts and de-duplicates them before hashing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contract {
    /// The assertions, in the order they were written.
    pub clauses: Vec<Clause>,
}

/// One assertion: what is being talked about, and what must be true of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    /// The whole clause line, for diagnostics that quote the source inline.
    pub span: Span,
    /// What the assertion is about.
    pub subject: Subject,
    /// What must hold.
    pub predicate: Predicate,
}

/// What a clause is about.
///
/// The three name-shaped subjects ([`Subject::Cell`] with no column, [`Subject::Named`])
/// are resolved by the fixed order in RFC-0006 §3: enum, then variant, then view
/// attribute, then prop default. The order is fixed rather than heuristic so that adding a
/// prop can never silently change what an existing clause means.
#[derive(Clone, Debug, PartialEq)]
pub enum Subject {
    /// `every <enum> <column>` — the whole column of a variant table.
    EveryVariant {
        /// The enum whose variants are quantified over.
        enum_name: String,
        /// The column read from each variant.
        column: String,
    },
    /// `<enum>.<variant> <column>`, or `<variant>.<column>` when the enum is unambiguous.
    Cell {
        /// The first dotted segment: an enum name, or a variant name.
        qualifier: String,
        /// The second dotted segment: a variant name, or a column name.
        member: String,
        /// The column, when the clause named all three parts.
        column: Option<String>,
    },
    /// A bare name — a view attribute, else a prop's default.
    Named(String),
    /// `<tag> "<text>"` — the view element with that tag carrying that text.
    Element {
        /// The element word, e.g. `button`.
        tag: String,
        /// The text it must carry, without quotes.
        text: String,
    },
    /// `when <variant>` — the branch of the view guarded on that variant.
    WhenVariant(String),
    /// The component itself, for clauses like `uses button`.
    Whole,
}

/// What must be true of a subject.
///
/// [`Predicate::Is`] holds the operand **as written** (a string keeps its quotes), because
/// it is compared against column and attribute values which are also stored as written.
/// The text-shaped predicates hold the unquoted text, because they compare against the
/// inside of a string.
#[derive(Clone, Debug, PartialEq)]
pub enum Predicate {
    /// `is <value>` — exact equality with the value as written.
    Is(String),
    /// `contains "<text>"` — substring of a string value.
    Contains(String),
    /// `not_empty` — a string value with non-whitespace in it.
    NotEmpty,
    /// `at_least <int>` — numeric floor.
    AtLeast(i64),
    /// `in "<a>" "<b>" …` — membership in a closed set.
    OneOf(Vec<String>),
    /// `uses "--token"` — the value references that CSS custom property.
    UsesToken(String),
    /// `uses <component>` — the view composes that component.
    Composes(String),
    /// `shows <tag> "<text>"` — the guarded branch renders that element.
    Shows {
        /// The element word that must appear.
        tag: String,
        /// The text it must carry, without quotes.
        text: String,
    },
    /// `min_height <int>` — the element's height floor, in CSS pixels.
    MinHeight(i64),
}

/// The words that open a predicate. `is` and `in` are language keywords; the rest are
/// ordinary identifiers, reserved only in predicate position so the keyword vocabulary
/// stays small (RFC-0002 §1).
pub const PREDICATE_WORDS: &[&str] = &[
    "is",
    "in",
    "contains",
    "not_empty",
    "at_least",
    "uses",
    "min_height",
    "shows",
];

impl Clause {
    /// The clause in one canonical spelling, whitespace and all.
    ///
    /// This is what the IR hashes (RFC-0003 §2.1 needs an exact serialization), so it must
    /// depend on the assertion's *meaning* and not on how it was laid out in the file.
    pub fn canonical(&self) -> String {
        let subject = match &self.subject {
            Subject::EveryVariant { enum_name, column } => format!("every {enum_name} {column}"),
            Subject::Cell {
                qualifier,
                member,
                column: Some(column),
            } => format!("{qualifier}.{member} {column}"),
            Subject::Cell {
                qualifier, member, ..
            } => format!("{qualifier}.{member}"),
            Subject::Named(name) => name.clone(),
            Subject::Element { tag, text } => format!("{tag} \"{text}\""),
            Subject::WhenVariant(variant) => format!("when {variant}"),
            Subject::Whole => String::new(),
        };
        let predicate = match &self.predicate {
            Predicate::Is(value) => format!("is {value}"),
            Predicate::Contains(text) => format!("contains \"{text}\""),
            Predicate::NotEmpty => "not_empty".to_string(),
            Predicate::AtLeast(n) => format!("at_least {n}"),
            Predicate::OneOf(set) => {
                let mut out = "in".to_string();
                for item in set {
                    out.push_str(&format!(" \"{item}\""));
                }
                out
            }
            Predicate::UsesToken(token) => format!("uses \"{token}\""),
            Predicate::Composes(name) => format!("uses {name}"),
            Predicate::Shows { tag, text } => format!("shows {tag} \"{text}\""),
            Predicate::MinHeight(n) => format!("min_height {n}"),
        };
        if subject.is_empty() {
            predicate
        } else {
            format!("{subject} {predicate}")
        }
    }
}

/// How many assertions ran, and how many of them did not hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Assertions evaluated.
    pub clauses: usize,
    /// Assertions that failed or could not be evaluated. Non-zero fails `mz contract`.
    pub failed: usize,
}

/// One assertion's result.
enum Verdict {
    /// It holds.
    Pass,
    /// It does not hold — a behavioural defect, the thing CHARTER.md §6 counts.
    Fail(String),
    /// It could not be evaluated. Also a failure: a contract that does nothing reads as
    /// verification while providing none.
    Unevaluable(String),
}

/// Evaluate a component's contract against its own declarations.
///
/// Returns the tally and one diagnostic per clause that did not hold. A component with no
/// `contract` block evaluates zero clauses and produces no diagnostic here — its absence
/// is already RFC-0001 §1.6's warning, raised at parse time.
pub fn evaluate(component: &Component, file: &str) -> (Tally, Vec<Diagnostic>) {
    let mut tally = Tally::default();
    let mut diags = Vec::new();
    let Some(contract) = &component.contract else {
        return (tally, diags);
    };

    for clause in &contract.clauses {
        tally.clauses += 1;
        let written = clause.canonical();
        match judge(component, clause) {
            Verdict::Pass => {}
            Verdict::Fail(why) => {
                tally.failed += 1;
                diags.push(Diagnostic::error(
                    "MZ0603",
                    file,
                    clause.span,
                    format!("`{written}` does not hold — {why}"),
                ));
            }
            Verdict::Unevaluable(why) => {
                tally.failed += 1;
                diags.push(Diagnostic::error(
                    "MZ0605",
                    file,
                    clause.span,
                    format!("`{written}` cannot be evaluated — {why}"),
                ));
            }
        }
    }

    (tally, diags)
}

fn judge(component: &Component, clause: &Clause) -> Verdict {
    match (&clause.subject, &clause.predicate) {
        // `uses <component>` — composition, checked against the view tree.
        (Subject::Whole, Predicate::Composes(name)) => {
            let view = component.view.as_deref().unwrap_or(&[]);
            if find_tag(view, name).is_some() {
                Verdict::Pass
            } else {
                Verdict::Fail(format!("the view has no `{name}` element"))
            }
        }
        (Subject::Whole, _) => {
            Verdict::Unevaluable("the component as a whole takes only `uses <name>`".to_string())
        }

        // `when <variant> shows <tag> "<text>"` — structural, not rendered. See RFC-0006 §5.
        (Subject::WhenVariant(variant), Predicate::Shows { tag, text }) => {
            let view = component.view.as_deref().unwrap_or(&[]);
            let Some(branch) = find_guard(view, variant) else {
                return Verdict::Unevaluable(format!("no `when` in the view mentions `{variant}`"));
            };
            match find_tag(&branch.children, tag) {
                Some(el) if has_text(el, text) => Verdict::Pass,
                Some(_) => Verdict::Fail(format!(
                    "the `{variant}` branch has a `{tag}`, but not one carrying \"{text}\""
                )),
                None => Verdict::Fail(format!("the `{variant}` branch renders no `{tag}`")),
            }
        }
        (Subject::WhenVariant(_), _) => {
            Verdict::Unevaluable("a `when` subject takes only `shows <tag> \"<text>\"`".to_string())
        }

        // `<tag> "<text>" min_height <n>` — the touch floor on a specific element.
        (Subject::Element { tag, text }, Predicate::MinHeight(floor)) => {
            let view = component.view.as_deref().unwrap_or(&[]);
            let Some(el) = find_tag_with_text(view, tag, text) else {
                return Verdict::Unevaluable(format!(
                    "the view has no `{tag}` carrying \"{text}\""
                ));
            };
            match element_height(el) {
                Some(h) if h >= *floor => Verdict::Pass,
                Some(h) => Verdict::Fail(format!("it is {h}px, below the {floor}px floor")),
                None => Verdict::Unevaluable(format!(
                    "`{tag}` declares no height: add a `height` attribute, or a `min-h-[{floor}px]` class"
                )),
            }
        }
        (Subject::Element { tag, .. }, _) => Verdict::Unevaluable(format!(
            "an element subject like `{tag}` takes only `min_height <n>`"
        )),

        // `every <enum> <column> <predicate>` — the whole column at once.
        (Subject::EveryVariant { enum_name, column }, predicate) => {
            let Some(decl) = find_enum(component, enum_name) else {
                return Verdict::Unevaluable(format!(
                    "no enum named `{enum_name}` in this component"
                ));
            };
            if decl.variants.is_empty() {
                return Verdict::Unevaluable(format!("`{enum_name}` has no variants"));
            }
            for variant in &decl.variants {
                let Some(value) = cell(variant, column) else {
                    return Verdict::Unevaluable(format!(
                        "`{}` has no `{column}` column",
                        variant.name
                    ));
                };
                match check(predicate, value) {
                    Verdict::Pass => {}
                    Verdict::Fail(why) => {
                        return Verdict::Fail(format!("`{}` {why}", variant.name));
                    }
                    other => return other,
                }
            }
            Verdict::Pass
        }

        // A dotted or bare name, resolved by RFC-0006 §3's fixed order.
        (subject, predicate) => match resolve(component, subject) {
            Ok(value) => check(predicate, &value),
            Err(verdict) => verdict,
        },
    }
}

/// Resolve a name-shaped subject to the value it denotes, as written.
fn resolve(component: &Component, subject: &Subject) -> Result<String, Verdict> {
    match subject {
        Subject::Cell {
            qualifier,
            member,
            column: Some(column),
        } => {
            let decl = find_enum(component, qualifier).ok_or_else(|| {
                Verdict::Unevaluable(format!("no enum named `{qualifier}` in this component"))
            })?;
            let variant = decl
                .variants
                .iter()
                .find(|v| &v.name == member)
                .ok_or_else(|| Verdict::Fail(format!("`{qualifier}` has no variant `{member}`")))?;
            cell(variant, column)
                .map(str::to_string)
                .ok_or_else(|| Verdict::Fail(format!("`{member}` has no `{column}` column")))
        }

        // `<a>.<b>` with no third part. `a` is a variant and `b` its column — but only if
        // exactly one enum says so, because two enums with a same-named variant would make
        // the clause mean two different things depending on declaration order.
        Subject::Cell {
            qualifier, member, ..
        } => {
            let matches: Vec<&EnumDecl> = component
                .enums
                .iter()
                .filter(|e| {
                    e.variants
                        .iter()
                        .any(|v| &v.name == qualifier && cell(v, member).is_some())
                })
                .collect();
            match matches.as_slice() {
                [one] => {
                    let variant = one.variants.iter().find(|v| &v.name == qualifier).unwrap();
                    Ok(cell(variant, member).unwrap().to_string())
                }
                [] if find_enum(component, qualifier).is_some() => {
                    Err(Verdict::Unevaluable(format!(
                        "`{qualifier}` is an enum, so name the column too: `{qualifier}.{member} <column> …`"
                    )))
                }
                [] => Err(Verdict::Unevaluable(format!(
                    "no variant `{qualifier}` with a `{member}` column in this component"
                ))),
                many => Err(Verdict::Unevaluable(format!(
                    "`{qualifier}` is a variant of {} enums — qualify it as `<enum>.{qualifier} {member} …`",
                    many.len()
                ))),
            }
        }

        Subject::Named(name) => {
            let view = component.view.as_deref().unwrap_or(&[]);
            if let Some(value) = outermost_attr(view, name) {
                return Ok(value.to_string());
            }
            match component.props.iter().find(|p| &p.name == name) {
                Some(prop) => prop.default.clone().ok_or_else(|| {
                    Verdict::Unevaluable(format!(
                        "`{name}` is a prop with no default, so its value is the caller's, not this component's"
                    ))
                }),
                None => Err(Verdict::Unevaluable(format!(
                    "`{name}` is not an enum, a variant, a view attribute, or a prop of this component"
                ))),
            }
        }

        _ => Err(Verdict::Unevaluable(
            "this subject shape has no value".to_string(),
        )),
    }
}

/// Apply a predicate to a value written the way the source writes it.
fn check(predicate: &Predicate, value: &str) -> Verdict {
    match predicate {
        Predicate::Is(expected) => {
            if value == expected {
                Verdict::Pass
            } else {
                Verdict::Fail(format!("it is {value}, not {expected}"))
            }
        }
        Predicate::Contains(needle) => match text_of(value) {
            Some(text) if text.contains(needle.as_str()) => Verdict::Pass,
            Some(_) => Verdict::Fail(format!("{value} does not contain \"{needle}\"")),
            None => Verdict::Unevaluable(format!("{value} is not a string")),
        },
        Predicate::NotEmpty => match text_of(value) {
            Some(text) if !text.trim().is_empty() => Verdict::Pass,
            Some(_) => Verdict::Fail("it is empty".to_string()),
            None => Verdict::Unevaluable(format!("{value} is not a string")),
        },
        Predicate::AtLeast(floor) => match value.parse::<i64>() {
            Ok(n) if n >= *floor => Verdict::Pass,
            Ok(n) => Verdict::Fail(format!("is {n}, below {floor}")),
            Err(_) => Verdict::Unevaluable(format!("{value} is not a number")),
        },
        Predicate::OneOf(set) => match text_of(value) {
            Some(text) if set.iter().any(|s| s == text) => Verdict::Pass,
            Some(_) => Verdict::Fail(format!(
                "is {value}, which is not one of the allowed values"
            )),
            None => Verdict::Unevaluable(format!("{value} is not a string")),
        },
        Predicate::UsesToken(token) => match text_of(value) {
            // `var(--radius-lg` rather than a bare substring: a class that merely mentions
            // the token's name in some other position is not a reference to it.
            Some(text) if text.contains(&format!("var({token}")) => Verdict::Pass,
            Some(_) => Verdict::Fail(format!("it never reads `var({token}…)`")),
            None => Verdict::Unevaluable(format!("{value} is not a string")),
        },
        Predicate::Composes(name) => Verdict::Unevaluable(format!(
            "`uses {name}` is a clause about the whole component, so it takes no subject"
        )),
        Predicate::Shows { .. } => {
            Verdict::Unevaluable("`shows` needs a `when <variant>` subject".to_string())
        }
        Predicate::MinHeight(_) => {
            Verdict::Unevaluable("`min_height` needs an element subject".to_string())
        }
    }
}

fn find_enum<'a>(component: &'a Component, name: &str) -> Option<&'a EnumDecl> {
    component.enums.iter().find(|e| e.name == name)
}

fn cell<'a>(variant: &'a Variant, column: &str) -> Option<&'a str> {
    variant
        .columns
        .iter()
        .find(|(c, _)| c == column)
        .map(|(_, v)| v.as_str())
}

/// The inside of a string literal, or `None` when the value is not one.
fn text_of(value: &str) -> Option<&str> {
    value.strip_prefix('"').and_then(|v| v.strip_suffix('"'))
}

/// The outermost element carrying `name`, breadth-first.
///
/// Outermost rather than "any", because `slot is "card"` is a claim about the component's
/// own identity, not about whatever one of its descendants happens to declare. `card` has
/// three slots; only the one on the root surface is the card's.
fn outermost_attr<'a>(view: &'a [Element], name: &str) -> Option<&'a str> {
    let mut level: Vec<&'a Element> = view.iter().collect();
    while !level.is_empty() {
        for el in &level {
            if let Some((_, value)) = el.attrs.iter().find(|(k, _)| k == name) {
                return Some(value.as_str());
            }
        }
        level = level.iter().flat_map(|el| el.children.iter()).collect();
    }
    None
}

fn find_tag<'a>(view: &'a [Element], tag: &str) -> Option<&'a Element> {
    for el in view {
        if el.tag == tag {
            return Some(el);
        }
        if let Some(found) = find_tag(&el.children, tag) {
            return Some(found);
        }
    }
    None
}

fn find_tag_with_text<'a>(view: &'a [Element], tag: &str, text: &str) -> Option<&'a Element> {
    for el in view {
        if el.tag == tag && has_text(el, text) {
            return Some(el);
        }
        if let Some(found) = find_tag_with_text(&el.children, tag, text) {
            return Some(found);
        }
    }
    None
}

/// Whether an element carries `text` as any attribute value — `text = "Retry"` in the
/// corpus, but also `label = "Retry"`, because which attribute holds a control's words is
/// an element's business and not the contract's.
fn has_text(el: &Element, text: &str) -> bool {
    el.attrs
        .iter()
        .any(|(_, value)| text_of(value) == Some(text))
}

/// A `when` element anywhere in the view whose condition mentions `variant`.
fn find_guard<'a>(view: &'a [Element], variant: &str) -> Option<&'a Element> {
    for el in view {
        if el.tag == "when" && el.attrs.iter().any(|(_, value)| value == variant) {
            return Some(el);
        }
        if let Some(found) = find_guard(&el.children, variant) {
            return Some(found);
        }
    }
    None
}

/// An element's height in CSS pixels, if it declares one.
///
/// Two sources, in order: an explicit `height` or `min_height` attribute, then a
/// `min-h-[Npx]` or `h-[Npx]` class. Tailwind's *scale* classes (`h-12`) are deliberately
/// not read — that mapping is a framework version's business, and guessing it would let
/// this check silently disagree with what actually renders. An element whose only height
/// is `h-12` reports unevaluable, which is the honest answer.
fn element_height(el: &Element) -> Option<i64> {
    for key in ["height", "min_height"] {
        if let Some((_, value)) = el.attrs.iter().find(|(k, _)| k == key)
            && let Ok(n) = value.parse::<i64>()
        {
            return Some(n);
        }
    }
    let class = el
        .attrs
        .iter()
        .find(|(k, _)| k == "class")
        .and_then(|(_, v)| text_of(v))?;
    for prefix in ["min-h-[", "h-["] {
        if let Some(px) = bracketed_pixels(class, prefix) {
            return Some(px);
        }
    }
    None
}

fn bracketed_pixels(class: &str, prefix: &str) -> Option<i64> {
    let start = class.find(prefix)? + prefix.len();
    let rest = &class[start..];
    let end = rest.find(']')?;
    rest[..end].strip_suffix("px")?.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_with_ast;

    fn parsed(src: &str) -> Component {
        let (component, report) = check_with_ast(src, "t.mz");
        assert_eq!(
            report.error_count(),
            0,
            "fixture must parse: {:#?}",
            report.diagnostics
        );
        component.expect("fixture produces a component")
    }

    fn run(src: &str) -> (Tally, Vec<String>) {
        let component = parsed(src);
        let (tally, diags) = evaluate(&component, "t.mz");
        (tally, diags.into_iter().map(|d| d.say).collect())
    }

    const TABLE: &str = "\
component a
  enum a_size
    default  class \"h-14\"  height 56
    sm       class \"h-12\"  height 48
  end
  view
    control
      slot = \"a\"
      class = \"rounded-[var(--radius-lg,14px)]\"
    end
  end
  contract
";

    fn with(clauses: &str) -> String {
        format!("{TABLE}{clauses}  end\nend component a\n")
    }

    #[test]
    fn a_column_floor_holds_across_every_variant() {
        let (tally, says) = run(&with("    every a_size height at_least 48\n"));
        assert_eq!(
            tally,
            Tally {
                clauses: 1,
                failed: 0
            },
            "{says:?}"
        );
    }

    #[test]
    fn a_violated_floor_names_the_variant_that_broke_it() {
        // The corpus defect this whole feature exists for: a size below the touch floor.
        let src = with("    every a_size height at_least 56\n");
        let (tally, says) = run(&src);
        assert_eq!(tally.failed, 1);
        assert!(says[0].contains("`sm`"), "must name the variant: {says:?}");
        assert!(says[0].contains("48"), "must quote the value: {says:?}");
    }

    #[test]
    fn a_cell_is_addressable_by_enum_variant_and_column() {
        let (tally, says) = run(&with("    a_size.sm height is 48\n"));
        assert_eq!(tally.failed, 0, "{says:?}");
    }

    #[test]
    fn a_dotted_variant_column_resolves_without_naming_the_enum() {
        let (tally, says) = run(&with("    sm.height is 48\n"));
        assert_eq!(tally.failed, 0, "{says:?}");
    }

    #[test]
    fn a_view_attribute_resolves_from_the_outermost_element_that_has_it() {
        let (tally, says) = run(&with("    slot is \"a\"\n"));
        assert_eq!(tally.failed, 0, "{says:?}");
    }

    #[test]
    fn a_design_token_reference_is_checked_as_a_var_read_not_a_substring() {
        let (tally, says) = run(&with("    class uses \"--radius-lg\"\n"));
        assert_eq!(tally.failed, 0, "{says:?}");
        // Naming the token without reading it must not pass.
        let (tally, _) = run(
            "component b\n  view\n    control\n      class = \"--radius-lg\"\n    end\n  end\n  contract\n    class uses \"--radius-lg\"\n  end\nend component b\n",
        );
        assert_eq!(tally.failed, 1);
    }

    #[test]
    fn an_unresolvable_subject_fails_rather_than_passing_quietly() {
        // The whole point: a clause that cannot be evaluated must not read as verification.
        let (tally, says) = run(&with("    card_radius uses \"--radius-lg\"\n"));
        assert_eq!(tally.failed, 1);
        assert!(
            says[0].contains("cannot be evaluated"),
            "must say so plainly: {says:?}"
        );
    }

    #[test]
    fn a_membership_set_rejects_a_value_outside_it() {
        let src = "\
component a
  enum a_variant
    one  announce \"status\"
    two  announce \"shout\"
  end
  contract
    every a_variant announce in \"status\" \"alert\"
  end
end component a
";
        let (tally, says) = run(src);
        assert_eq!(tally.failed, 1);
        assert!(says[0].contains("`two`"), "{says:?}");
    }

    #[test]
    fn composition_is_checked_against_the_view_tree() {
        let src = "\
component a
  view
    row
      button
        label = \"go\"
      end
    end
  end
  contract
    uses button
    uses alert
  end
end component a
";
        let (tally, says) = run(src);
        assert_eq!(tally.clauses, 2);
        assert_eq!(tally.failed, 1, "{says:?}");
        assert!(says[0].contains("`alert`"), "{says:?}");
    }

    #[test]
    fn a_guarded_branch_is_checked_structurally() {
        let src = "\
component a
  enum s
    offline  label \"off\"
  end
  prop state: s
  view
    strip
      when state is offline
        button
          text = \"Retry\"
          class = \"min-h-[48px]\"
        end
      end
    end
  end
  contract
    when offline shows button \"Retry\"
    button \"Retry\" min_height 48
  end
end component a
";
        let (tally, says) = run(src);
        assert_eq!(
            tally,
            Tally {
                clauses: 2,
                failed: 0
            },
            "{says:?}"
        );
    }

    #[test]
    fn a_touch_floor_violation_in_a_class_string_is_caught() {
        let src = "\
component a
  view
    button
      text = \"Retry\"
      class = \"min-h-[40px]\"
    end
  end
  contract
    button \"Retry\" min_height 48
  end
end component a
";
        let (tally, says) = run(src);
        assert_eq!(tally.failed, 1);
        assert!(says[0].contains("40px"), "{says:?}");
    }

    #[test]
    fn a_height_the_compiler_cannot_read_is_unevaluable_not_a_pass() {
        // `h-12` is 48px in one Tailwind version and could be something else in another.
        // Guessing would let the check silently disagree with what renders.
        let src = "\
component a
  view
    button
      text = \"Retry\"
      class = \"h-12\"
    end
  end
  contract
    button \"Retry\" min_height 48
  end
end component a
";
        let (tally, says) = run(src);
        assert_eq!(tally.failed, 1);
        assert!(says[0].contains("cannot be evaluated"), "{says:?}");
    }

    #[test]
    fn a_component_with_no_contract_evaluates_nothing_and_fails_nothing() {
        let component = parsed("component a\nend component a\n");
        let (tally, diags) = evaluate(&component, "a.mz");
        assert_eq!(tally, Tally::default());
        assert!(diags.is_empty());
    }

    #[test]
    fn the_canonical_form_is_independent_of_source_spacing() {
        let tight = parsed(&with("    every a_size height at_least 48\n"));
        let loose = parsed(&with("    every    a_size   height    at_least   48\n"));
        assert_eq!(
            tight.contract.unwrap().clauses[0].canonical(),
            loose.contract.unwrap().clauses[0].canonical()
        );
    }
}
