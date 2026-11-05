//! Rendering a component's view to markup ([RFC-0017](../../design/RFC-0017-render.md)).
//!
//! A component's `view` is already a tree of elements in the AST. This module evaluates that
//! tree against a set of prop values and produces two things: the **element tree as a value**
//! ([`Node`]) and its **serialisation** to a markup string with escaping ([`to_html`]).
//!
//! What this is, and is not (tracker row U1, `LANGUAGE-TRACKER.md`):
//!
//! - It is a library function, exercised by `compiler/tests/render.rs` and by the contract
//!   evaluator's render-backed clauses (RFC-0017 §7). There is no `mz` command for it and no
//!   Workers, browser or WebAssembly target: **nothing here renders in a browser or on a
//!   server, and no component lowers to Rust.**
//! - It renders the **static** part of the view grammar: elements, attributes, `text`,
//!   interpolation, `when` and `else`. Event wires (`tap`, `change`) are not markup and are
//!   left out. `for each`, composing another component, children and slots, pass-through
//!   attributes and raw output are each a later gap (U2 to U7) and report
//!   [`RenderError::Unsupported`] rather than rendering something wrong.
//! - Escaping is total: every text node and every attribute value is escaped when it is
//!   serialised, and there is no way to write unescaped output.

use std::fmt;

use crate::ast::{Attr, Component, Element, EnumDecl, PropDecl, TypeKind};

/// One node of an element tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// An element: a tag, its attributes in source order, and its children.
    Element {
        /// The markup tag, from [`tag_for`].
        tag: &'static str,
        /// `(name, value)` pairs, in the order the view wrote them. Values are unescaped
        /// here; [`to_html`] escapes them.
        attrs: Vec<(String, String)>,
        /// Child nodes: the element's `text` first, then its child elements.
        children: Vec<Node>,
    },
    /// A run of text. Unescaped here; [`to_html`] escapes it.
    Text(String),
}

/// A prop value supplied to [`render`], typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A `bool`.
    Bool(bool),
    /// An `int`.
    Int(i64),
    /// A `text`.
    Text(String),
    /// An enum variant, by name.
    Variant(String),
    /// An `option(T)` that is absent.
    None,
}

/// Why a component could not be rendered. Every case is a refusal: a render that cannot be
/// right does not produce markup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    /// A supplied prop the component does not declare.
    UnknownProp(String),
    /// A prop with no default that was not supplied.
    MissingProp(String),
    /// A supplied value of the wrong type.
    WrongType {
        /// The prop.
        prop: String,
        /// The declared type, as written.
        expected: String,
        /// What was supplied.
        got: String,
    },
    /// A variant the prop's enum does not have.
    UnknownVariant {
        /// The prop.
        prop: String,
        /// The variant supplied.
        variant: String,
    },
    /// An element word with no markup tag in [`tag_for`]'s table. Another component's name
    /// lands here too, until composition is rendered (tracker U2).
    UnknownElement(String),
    /// A form the static renderer does not cover yet; the message names the tracker row.
    Unsupported(String),
    /// A name in the view that does not resolve to a value. The checker reports these; this
    /// is the renderer refusing a tree it was handed unchecked.
    Unresolved(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProp(p) => write!(f, "the component has no prop `{p}`"),
            Self::MissingProp(p) => write!(f, "prop `{p}` has no default and was not supplied"),
            Self::WrongType {
                prop,
                expected,
                got,
            } => write!(f, "prop `{prop}` is {expected}, but {got} was supplied"),
            Self::UnknownVariant { prop, variant } => {
                write!(f, "`{variant}` is not a variant of prop `{prop}`'s enum")
            }
            Self::UnknownElement(w) => write!(f, "element word `{w}` has no markup tag"),
            Self::Unsupported(what) => write!(f, "{what}"),
            Self::Unresolved(n) => write!(f, "`{n}` does not resolve to a value"),
        }
    }
}

impl std::error::Error for RenderError {}

/// The markup tag for a view element word, or `None` for a word the table does not know.
///
/// The view vocabulary is semantic (`strip`, `notice`, `rule`), not markup. This table is the
/// one place the two meet. It is closed: a word that is not here is
/// [`RenderError::UnknownElement`], never a guessed `div`.
pub fn tag_for(word: &str) -> Option<&'static str> {
    Some(match word {
        "strip" | "notice" | "row" | "surface" | "rule" | "field" => "div",
        "mark" | "busy" => "span",
        "caption" => "label",
        "figure" => "figure",
        "picture" => "img",
        "button" => "button",
        _ => return None,
    })
}

/// Tags that have no closing tag and no children.
fn is_void(tag: &str) -> bool {
    matches!(tag, "img" | "input" | "br" | "hr")
}

/// The markup attribute name for a view attribute, or `None` when the attribute is not
/// markup (an event wire).
pub fn markup_attr(name: &str) -> Option<String> {
    Some(match name {
        "tap" | "change" => return None,
        "slot" => "data-slot".to_string(),
        "portal" => "data-portal".to_string(),
        "source" => "src".to_string(),
        "class" | "id" | "role" | "href" | "src" | "alt" | "for" | "type" | "name" | "value"
        | "placeholder" | "title" | "lang" | "dir" | "tabindex" | "width" | "height" | "target"
        | "rel" => name.to_string(),
        n if n.starts_with("aria_") => n.replace('_', "-"),
        n => format!("data-{}", n.replace('_', "-")),
    })
}

/// Escape `value` for text and double-quoted attribute values: `&`, `<`, `>`, `"` and `'`
/// become decimal references; everything else, including non-ASCII, is written unchanged.
/// The same five, in the same form, as the registry's `html` module.
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&#38;"),
            '<' => out.push_str("&#60;"),
            '>' => out.push_str("&#62;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Serialise a node list to markup. Attributes are written in order, double-quoted, and
/// every text node and attribute value is escaped.
pub fn to_html(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        write_node(&mut out, node);
    }
    out
}

fn write_node(out: &mut String, node: &Node) {
    match node {
        Node::Text(t) => out.push_str(&escape(t)),
        Node::Element {
            tag,
            attrs,
            children,
        } => {
            out.push('<');
            out.push_str(tag);
            for (name, value) in attrs {
                out.push(' ');
                out.push_str(name);
                out.push_str("=\"");
                out.push_str(&escape(value));
                out.push('"');
            }
            out.push('>');
            if is_void(tag) {
                return;
            }
            for child in children {
                write_node(out, child);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
    }
}

/// Render a component's view with the given prop values to markup.
///
/// Props not supplied take their declared default. See [`tree`] for the element tree.
pub fn render(component: &Component, props: &[(&str, Value)]) -> Result<String, RenderError> {
    Ok(to_html(&tree(component, props)?))
}

/// Evaluate a component's view with the given prop values to an element tree.
pub fn tree(component: &Component, props: &[(&str, Value)]) -> Result<Vec<Node>, RenderError> {
    let env = bind(component, props)?;
    let mut out = Vec::new();
    for el in component.view.as_deref().unwrap_or(&[]) {
        element(component, &env, el, &mut out)?;
    }
    Ok(out)
}

/// The prop environment: each renderable prop's name and value.
struct Env(Vec<(String, Value)>);

impl Env {
    fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

fn type_label(ty: &crate::ast::TypeExpr) -> String {
    ty.to_string()
}

fn describe(v: &Value) -> String {
    match v {
        Value::Bool(b) => format!("the bool `{b}`"),
        Value::Int(i) => format!("the int `{i}`"),
        Value::Text(t) => format!("the text \"{t}\""),
        Value::Variant(n) => format!("the variant `{n}`"),
        Value::None => "none".to_string(),
    }
}

fn find_enum<'a>(c: &'a Component, name: &str) -> Option<&'a EnumDecl> {
    c.enums.iter().find(|e| e.name == name)
}

/// Check one supplied value against a declared type.
fn check_value(
    c: &Component,
    prop: &PropDecl,
    ty: &crate::ast::TypeExpr,
    v: &Value,
) -> Result<(), RenderError> {
    let wrong = || RenderError::WrongType {
        prop: prop.name.clone(),
        expected: type_label(&prop.ty),
        got: describe(v),
    };
    match (&ty.kind, v) {
        (TypeKind::Apply(ctor, _), Value::None) if ctor == "option" => Ok(()),
        (TypeKind::Apply(ctor, inner), _) if ctor == "option" => check_value(c, prop, inner, v),
        (TypeKind::Name(n), Value::Bool(_)) if n == "bool" => Ok(()),
        (TypeKind::Name(n), Value::Int(_)) if n == "int" => Ok(()),
        (TypeKind::Name(n), Value::Text(_)) if n == "text" => Ok(()),
        (TypeKind::Name(n), Value::Variant(variant)) => match find_enum(c, n) {
            Some(decl) if decl.variants.iter().any(|x| &x.name == variant) => Ok(()),
            Some(_) => Err(RenderError::UnknownVariant {
                prop: prop.name.clone(),
                variant: variant.clone(),
            }),
            None => Err(wrong()),
        },
        (TypeKind::Name(n), _) if matches!(n.as_str(), "bool" | "int" | "text") => Err(wrong()),
        (TypeKind::Name(n), _) if find_enum(c, n).is_some() => Err(wrong()),
        _ => Err(RenderError::Unsupported(format!(
            "prop `{}` has type {}, which the static renderer cannot take a value for",
            prop.name,
            type_label(&prop.ty)
        ))),
    }
}

/// Parse a default as written into a value of the declared type.
fn parse_default(
    c: &Component,
    prop: &PropDecl,
    ty: &crate::ast::TypeExpr,
    written: &str,
) -> Result<Value, RenderError> {
    let unsupported = || {
        RenderError::Unsupported(format!(
            "prop `{}` has a default the static renderer cannot read: `{written}`",
            prop.name
        ))
    };
    let v = match &ty.kind {
        TypeKind::Apply(ctor, inner) if ctor == "option" => {
            if written == "none" {
                return Ok(Value::None);
            }
            return parse_default(c, prop, inner, written);
        }
        TypeKind::Name(n) if n == "bool" => match written {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err(unsupported()),
        },
        TypeKind::Name(n) if n == "int" => Value::Int(written.parse().map_err(|_| unsupported())?),
        TypeKind::Name(n) if n == "text" => {
            let inner = written
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(unsupported)?;
            Value::Text(inner.to_string())
        }
        TypeKind::Name(n) if find_enum(c, n).is_some() => Value::Variant(written.to_string()),
        _ => return Err(unsupported()),
    };
    check_value(c, prop, ty, &v)?;
    Ok(v)
}

fn bind(c: &Component, supplied: &[(&str, Value)]) -> Result<Env, RenderError> {
    for (name, _) in supplied {
        if !c.props.iter().any(|p| p.name == *name) {
            return Err(RenderError::UnknownProp((*name).to_string()));
        }
    }
    let mut env = Vec::new();
    for prop in &c.props {
        // An event prop is not a value: it has no entry, and nothing may be supplied for it.
        if matches!(&prop.ty.kind, TypeKind::Apply(ctor, _) if ctor == "event") {
            if supplied.iter().any(|(n, _)| *n == prop.name) {
                return Err(RenderError::Unsupported(format!(
                    "prop `{}` is an event; handlers are not rendered (tracker U4)",
                    prop.name
                )));
            }
            continue;
        }
        let value = match supplied.iter().find(|(n, _)| *n == prop.name) {
            Some((_, v)) => {
                check_value(c, prop, &prop.ty, v)?;
                v.clone()
            }
            None => match &prop.default {
                Some(written) => parse_default(c, prop, &prop.ty, written)?,
                None if matches!(&prop.ty.kind, TypeKind::Apply(ctor, _) if ctor == "option") => {
                    Value::None
                }
                None => return Err(RenderError::MissingProp(prop.name.clone())),
            },
        };
        env.push((prop.name.clone(), value));
    }
    Ok(Env(env))
}

/// A resolved path's value as text, or `None` for an absent option.
fn path_text(c: &Component, env: &Env, path: &str) -> Result<Option<String>, RenderError> {
    let mut segments = path.split('.');
    let head = segments.next().unwrap_or_default();
    let value = env
        .get(head)
        .ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
    let column = segments.next();
    if segments.next().is_some() {
        return Err(RenderError::Unsupported(format!(
            "`{path}` goes more than one level deep; records are not rendered yet"
        )));
    }
    Ok(match (value, column) {
        (Value::None, _) => None,
        (Value::Bool(b), None) => Some(b.to_string()),
        (Value::Int(i), None) => Some(i.to_string()),
        (Value::Text(t), None) => Some(t.clone()),
        (Value::Variant(v), None) => Some(v.clone()),
        (Value::Variant(variant), Some(col)) => {
            let ty = c
                .props
                .iter()
                .find(|p| p.name == head)
                .map(|p| &p.ty)
                .ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
            let enum_name = enum_of(ty).ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
            let decl =
                find_enum(c, enum_name).ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
            let v = decl
                .variants
                .iter()
                .find(|x| &x.name == variant)
                .ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
            let (_, written) = v
                .columns
                .iter()
                .find(|(n, _)| n == col)
                .ok_or_else(|| RenderError::Unresolved(path.to_string()))?;
            Some(unquote(written).to_string())
        }
        _ => return Err(RenderError::Unresolved(path.to_string())),
    })
}

fn enum_of(ty: &crate::ast::TypeExpr) -> Option<&str> {
    match &ty.kind {
        TypeKind::Name(n) => Some(n),
        TypeKind::Apply(ctor, inner) if ctor == "option" => enum_of(inner),
        _ => None,
    }
}

fn unquote(written: &str) -> &str {
    written
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(written)
}

/// An attribute's value, interpolated, or `None` when it is an absent option.
fn attr_value(c: &Component, env: &Env, attr: &Attr) -> Result<Option<String>, RenderError> {
    let v = attr.value.as_str();
    if let Some(inner) = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        let mut out = String::new();
        let mut rest = inner;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let close = rest[open..]
                .find('}')
                .map(|p| p + open)
                .ok_or_else(|| RenderError::Unresolved(v.to_string()))?;
            let body = &rest[open + 1..close];
            let text = path_text(c, env, body)?
                .ok_or_else(|| RenderError::Unresolved(format!("{body} (absent)")))?;
            out.push_str(&text);
            rest = &rest[close + 1..];
        }
        out.push_str(rest);
        return Ok(Some(out));
    }
    if v.parse::<i64>().is_ok() || matches!(v, "true" | "false") {
        return Ok(Some(v.to_string()));
    }
    if v == "none" || v == "nothing" {
        return Ok(None);
    }
    path_text(c, env, v)
}

/// Whether a `when` condition holds.
fn holds(env: &Env, el: &Element) -> Result<bool, RenderError> {
    let cond: Vec<&Attr> = el
        .attrs
        .iter()
        .filter(|a| a.name.is_empty() || matches!(a.name.as_str(), "not" | "is"))
        .collect();
    let shape: Vec<&str> = cond.iter().map(|a| a.name.as_str()).collect();
    let value_of = |a: &Attr| -> Result<Value, RenderError> {
        let head = a.value.split('.').next().unwrap_or_default();
        env.get(head)
            .cloned()
            .ok_or_else(|| RenderError::Unresolved(a.value.clone()))
    };
    match shape.as_slice() {
        [""] | ["not"] => {
            let negated = shape[0] == "not";
            match value_of(cond[0])? {
                Value::Bool(b) => Ok(b != negated),
                _ => Err(RenderError::Unresolved(format!(
                    "{} is not a bool",
                    cond[0].value
                ))),
            }
        }
        ["", "is"] => {
            let (subject, want) = (cond[0], cond[1].value.as_str());
            let have = value_of(subject)?;
            Ok(match (&have, want) {
                (Value::None, "none") => true,
                (_, "none") => false,
                (Value::Variant(v), w) => v == w,
                (Value::Bool(b), w) => b.to_string() == w,
                (Value::Int(i), w) => i.to_string() == w,
                (Value::Text(t), w) => format!("\"{t}\"") == w,
                (Value::None, _) => false,
            })
        }
        _ => Err(RenderError::Unsupported(
            "a `when` condition this renderer does not read".to_string(),
        )),
    }
}

fn element(c: &Component, env: &Env, el: &Element, out: &mut Vec<Node>) -> Result<(), RenderError> {
    match el.tag.as_str() {
        "nothing" => Ok(()),
        "when" => {
            let branch = if holds(env, el)? {
                Some(el.children.as_slice())
            } else {
                el.else_children.as_deref()
            };
            for child in branch.unwrap_or(&[]) {
                element(c, env, child, out)?;
            }
            Ok(())
        }
        "for" => Err(RenderError::Unsupported(
            "`for each` is not rendered yet (tracker U8)".to_string(),
        )),
        word => {
            let tag = tag_for(word).ok_or_else(|| RenderError::UnknownElement(word.to_string()))?;
            let mut attrs = Vec::new();
            let mut children = Vec::new();
            for attr in &el.attrs {
                if attr.name.is_empty() {
                    continue;
                }
                if attr.name == "text" {
                    if let Some(t) = attr_value(c, env, attr)? {
                        children.push(Node::Text(t));
                    }
                    continue;
                }
                let Some(name) = markup_attr(&attr.name) else {
                    continue;
                };
                if let Some(value) = attr_value(c, env, attr)? {
                    attrs.push((name, value));
                }
            }
            if is_void(tag) && !el.children.is_empty() {
                return Err(RenderError::Unsupported(format!(
                    "`{word}` is a `{tag}`, which cannot have children"
                )));
            }
            for child in &el.children {
                element(c, env, child, &mut children)?;
            }
            out.push(Node::Element {
                tag,
                attrs,
                children,
            });
            Ok(())
        }
    }
}
