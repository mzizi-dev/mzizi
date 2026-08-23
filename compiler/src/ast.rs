//! The AST. Deliberately shallow: the prototype's job is to prove the grammar and the
//! diagnostic loop, not to lower anything yet (lowering waits on the IR — RFC-0002 §2.1).

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
    /// Prop declarations.
    pub props: Vec<PropDecl>,
    /// The view tree, if a `view` block was present.
    pub view: Option<Vec<Element>>,
    /// Function names declared (bodies are not modelled in the prototype).
    pub fns: Vec<String>,
    /// Whether a `contract` block was present — its absence is a warning (RFC-0001 §1.6).
    pub has_contract: bool,
}

/// An enum whose variants carry data columns (RFC-0001 §1.3).
#[derive(Debug, PartialEq)]
pub struct EnumDecl {
    /// Enum name.
    pub name: String,
    /// Variants in declaration order.
    pub variants: Vec<Variant>,
}

/// One enum variant and its column values.
#[derive(Debug, PartialEq)]
pub struct Variant {
    /// Variant name.
    pub name: String,
    /// Where the variant name is, for the missing-column diagnostic.
    pub span: Span,
    /// `(column, value)` pairs in declaration order.
    pub columns: Vec<(String, String)>,
}

/// A prop declaration: `prop name: type [= default]`.
#[derive(Debug, PartialEq)]
pub struct PropDecl {
    /// Prop name.
    pub name: String,
    /// Declared type, as written.
    pub ty: String,
    /// Whether a default was supplied.
    pub has_default: bool,
    /// The default value as written, when there is one. Part of the interface: a caller
    /// needs to know what happens when the prop is omitted.
    pub default: Option<String>,
}

/// A view element: a word, its attributes, and its children.
#[derive(Debug, PartialEq)]
pub struct Element {
    /// Element word (`strip`, `button`, `text`, `when`, …).
    pub tag: String,
    /// Where the tag is.
    pub span: Span,
    /// `(name, value-as-written)` attribute pairs.
    pub attrs: Vec<(String, String)>,
    /// Nested children.
    pub children: Vec<Element>,
}
