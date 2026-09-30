//! The AST. Deliberately shallow: the prototype's job is to prove the grammar and the
//! diagnostic loop, not to lower anything yet (lowering waits on the IR — RFC-0002 §2.1).

use crate::contract::Contract;
use crate::diagnostic::Span;

/// A whole `.mz` file: exactly one component (RFC-0001 §7.4 — one component, one file).
#[derive(Debug, PartialEq)]
pub struct Component {
    /// The component's name, `snake_case`.
    pub name: String,
    /// Where the name appears, for diagnostics that point at identity.
    pub name_span: Span,
    /// Doc lines (`##`), in order.
    pub docs: Vec<String>,
    /// Declared capabilities (`use net`), in order.
    pub uses: Vec<String>,
    /// Enum declarations.
    pub enums: Vec<EnumDecl>,
    /// Record declarations (RFC-0008 §2).
    pub records: Vec<RecordDecl>,
    /// Prop declarations.
    pub props: Vec<PropDecl>,
    /// The view tree, if a `view` block was present.
    pub view: Option<Vec<Element>>,
    /// Function names declared. Bodies are not modelled beyond `emit` (RFC-0008 §5).
    pub fns: Vec<String>,
    /// Every `emit` statement in every `fn` body, in source order.
    pub emits: Vec<Emit>,
    /// Names declared by lines that failed to parse. The resolver treats them as known
    /// with an unknown type, so one broken `prop` line is one diagnostic rather than one
    /// per use of the prop (RFC-0001 §4.1).
    pub broken: Vec<String>,
    /// The `contract` block, when one was present — its absence is a warning
    /// (RFC-0001 §1.6).
    ///
    /// The body used to be parsed and thrown away behind a `has_contract: bool`, which
    /// made the charter's defect metric unmeasurable: a presence flag cannot tell you
    /// whether the behaviour a component promises is the behaviour it has. Retaining the
    /// assertions is what `mz contract` evaluates (RFC-0006).
    pub contract: Option<Contract>,
}

impl Component {
    /// Whether a `contract` block was present at all.
    pub fn has_contract(&self) -> bool {
        self.contract.is_some()
    }
}

/// An enum whose variants carry data columns (RFC-0001 §1.3).
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    /// Enum name.
    pub name: String,
    /// Variants in declaration order.
    pub variants: Vec<Variant>,
}

/// One enum variant and its column values.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// Variant name.
    pub name: String,
    /// Where the variant name is, for the missing-column diagnostic.
    pub span: Span,
    /// `(column, value)` pairs in declaration order.
    pub columns: Vec<(String, String)>,
}

/// A record declaration: named fields, closed with a bare `end` (RFC-0008 §2).
#[derive(Clone, Debug, PartialEq)]
pub struct RecordDecl {
    /// Record name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// Fields in declaration order — order is meaning, because it is the layout a
    /// serialiser and an FFI binding emit.
    pub fields: Vec<FieldDecl>,
    /// Field names declared by `field` lines whose type failed to parse. As with
    /// [`Component::broken`], the resolver treats them as known with an unknown type, so
    /// one broken line is one diagnostic, not one more per use of the field.
    pub broken: Vec<String>,
}

/// One `field <name>: <type>` line.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldDecl {
    /// Field name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// Declared type.
    pub ty: TypeExpr,
}

/// A type as written: a name, or a constructor applied to one argument (RFC-0008 §1).
///
/// Names are resolved by the checker, not the parser, because a name may be a built-in
/// (`text`), an enum or a record declared later in the file.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeExpr {
    /// What was written.
    pub kind: TypeKind,
    /// Where it was written, the whole expression.
    pub span: Span,
}

/// The shape of a [`TypeExpr`].
#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    /// `bool`, `int`, `text`, or a declared enum or record — or an unknown name.
    Name(String),
    /// `none`, legal only as the payload of `event(none)`.
    Nothing,
    /// `list(T)`, `option(T)`, `event(T)` — or any other word followed by `(...)`, which
    /// the checker rejects with a precise diagnostic.
    Apply(String, Box<TypeExpr>),
}

impl std::fmt::Display for TypeExpr {
    /// The canonical spelling, independent of source whitespace. This is what the IR
    /// hashes and what `mz outline` prints.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            TypeKind::Name(name) => f.write_str(name),
            TypeKind::Nothing => f.write_str("none"),
            TypeKind::Apply(ctor, arg) => write!(f, "{ctor}({arg})"),
        }
    }
}

impl PartialEq<&str> for TypeExpr {
    /// Compare against a canonical spelling, structurally, without rendering.
    fn eq(&self, other: &&str) -> bool {
        match &self.kind {
            TypeKind::Name(name) => name == other,
            TypeKind::Nothing => *other == "none",
            TypeKind::Apply(ctor, arg) => other
                .strip_prefix(ctor.as_str())
                .and_then(|rest| rest.strip_prefix('('))
                .and_then(|rest| rest.strip_suffix(')'))
                .is_some_and(|inner| **arg == inner),
        }
    }
}

/// An `emit` statement inside a `fn` body: `emit <event>` or `emit <event>(<value>)`.
#[derive(Debug, PartialEq)]
pub struct Emit {
    /// The `fn` it appears in.
    pub in_fn: String,
    /// The event prop named.
    pub target: String,
    /// Where the event name is.
    pub target_span: Span,
    /// The payload as written, and where, when one was given.
    pub arg: Option<(String, Span)>,
}

/// A prop declaration: `prop name: type [= default]`.
#[derive(Debug, PartialEq)]
pub struct PropDecl {
    /// Prop name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// Declared type.
    pub ty: TypeExpr,
    /// Whether a default was supplied.
    pub has_default: bool,
    /// The default value as written, when there is one. Part of the interface: a caller
    /// needs to know what happens when the prop is omitted.
    pub default: Option<String>,
    /// From the `=` through the end of the default, for fixes that delete or rewrite it.
    pub default_span: Option<Span>,
}

/// A view element: a word, its attributes, and its children.
#[derive(Debug, PartialEq)]
pub struct Element {
    /// Element word (`strip`, `button`, `text`, `when`, …).
    pub tag: String,
    /// Where the tag is.
    pub span: Span,
    /// Attributes, and the condition words of a `when` / `for` line, in source order.
    pub attrs: Vec<Attr>,
    /// Nested children.
    pub children: Vec<Element>,
    /// The `else` branch of a `when`, when it has one (RFC-0008 §4).
    pub else_children: Option<Vec<Element>>,
}

/// One attribute: `name = value`, or one word of a condition tail (`is offline`), whose
/// name is the keyword before it or empty for the leading operand.
#[derive(Clone, Debug, PartialEq)]
pub struct Attr {
    /// Attribute name, condition keyword, or empty.
    pub name: String,
    /// The value as written: a string keeps its quotes, a dotted path its dots.
    pub value: String,
    /// Where the value is.
    pub span: Span,
    /// Each dotted segment's span, for a path; empty for a literal.
    pub segments: Vec<Span>,
}
