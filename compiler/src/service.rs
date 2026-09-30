//! Services: the backend declaration (RFC-0011).
//!
//! A `service` is the second kind of top-level declaration beside `component`: one per file,
//! holding records, enums, `route` blocks, an optional `fallback` and a `contract`. This
//! module holds the tree the parser builds for one, and the checker that proves what
//! RFC-0011 §2–§4 promise — that every route has one method and a well-formed pattern, that
//! every path through a handler responds exactly once, and that every name, header and
//! record literal is one the service declares.
//!
//! Nothing here runs a handler; [`crate::serve`] does, and `crate::lower` emits Rust.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::ast::{Component, EnumDecl, RecordDecl, TypeExpr, TypeKind};
use crate::diagnostic::{Confidence, Diagnostic, Span};
use crate::resolve::{Ty, nearest};

/// A whole `.mz` file holding a service.
#[derive(Debug, PartialEq)]
pub struct Service {
    /// The service's name, `snake_case`.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// Doc lines, in order.
    pub docs: Vec<String>,
    /// Declared capabilities.
    pub uses: Vec<String>,
    /// Service-level `header` lines: they apply to every response (RFC-0011 §3).
    pub headers: Vec<HeaderLine>,
    /// Enum declarations, as in a component.
    pub enums: Vec<EnumDecl>,
    /// Record declarations, as in a component.
    pub records: Vec<RecordDecl>,
    /// Routes, in source order.
    pub routes: Vec<Route>,
    /// The handler for a path no route matches.
    pub fallback: Option<Handler>,
    /// The `contract` block, when present.
    pub contract: Option<ServiceContract>,
}

/// An HTTP method a route can declare (RFC-0011 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// `get`
    Get,
    /// `head`
    Head,
    /// `post`
    Post,
    /// `put`
    Put,
    /// `patch`
    Patch,
    /// `delete`
    Delete,
    /// `options`
    Options,
}

impl Method {
    /// Every method, in the fixed order an `allow` header lists them (RFC-0011 §6).
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Head,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Options,
    ];

    /// The method word as a Mzizi author writes it.
    pub fn word(self) -> &'static str {
        match self {
            Method::Get => "get",
            Method::Head => "head",
            Method::Post => "post",
            Method::Put => "put",
            Method::Patch => "patch",
            Method::Delete => "delete",
            Method::Options => "options",
        }
    }

    /// The method as HTTP spells it.
    pub fn http(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
        }
    }

    /// The method for a word, when it is one.
    pub fn from_word(word: &str) -> Option<Method> {
        Method::ALL.into_iter().find(|m| m.word() == word)
    }

    /// The method for an HTTP token, case-sensitively, as HTTP defines methods.
    pub fn from_http(token: &str) -> Option<Method> {
        Method::ALL.into_iter().find(|m| m.http() == token)
    }
}

/// A `header "<name>" "<value>"` line, in a service or a handler.
#[derive(Clone, Debug, PartialEq)]
pub struct HeaderLine {
    /// The header name as written.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// The value as written, interpolation braces intact.
    pub value: String,
    /// Where the value is.
    pub value_span: Span,
}

/// One segment of a route pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// A literal segment, matched exactly.
    Literal(String),
    /// `{name}` — binds a `text` parameter.
    Param(String),
}

/// One `route` block.
#[derive(Debug, PartialEq)]
pub struct Route {
    /// The route's name.
    pub name: String,
    /// Where the name is.
    pub name_span: Span,
    /// The method, when the method line parsed.
    pub method: Option<(Method, Span)>,
    /// The pattern as written, without quotes, when the method line parsed.
    pub pattern: Option<(String, Span)>,
    /// `query <name>: <type>` lines.
    pub queries: Vec<QueryDecl>,
    /// The handler body.
    pub handler: Handler,
}

/// A `query <name>: <type>` line.
#[derive(Debug, PartialEq)]
pub struct QueryDecl {
    /// Parameter name.
    pub name: String,
    /// Where the name is.
    pub span: Span,
    /// The declared type.
    pub ty: TypeExpr,
}

/// A handler body: a route's, or the fallback's.
#[derive(Debug, PartialEq)]
pub struct Handler {
    /// Where the block opens (the `route` or `fallback` word).
    pub span: Span,
    /// The statements, in order.
    pub body: Vec<Stmt>,
    /// Where the block's `end` is, or the end of file when it never closed.
    pub end_span: Span,
    /// Whether the block reached its own `end`. An unclosed block is already `MZ0204`,
    /// so it is not also reported as a path without a `respond`.
    pub closed: bool,
}

/// A handler statement (RFC-0011 §3).
#[derive(Debug, PartialEq)]
pub enum Stmt {
    /// `when <cond> … [else …] end`.
    When {
        /// The condition, when it parsed.
        cond: Option<Cond>,
        /// Statements run when it holds.
        then: Vec<Stmt>,
        /// Statements run when it does not, if there is an `else`.
        els: Option<Vec<Stmt>>,
        /// The `when` line.
        span: Span,
        /// The closing `end`.
        end_span: Span,
    },
    /// `header "<name>" "<value>"`.
    Header(HeaderLine),
    /// `respond <status> [<body>]`.
    Respond(Respond),
}

impl Stmt {
    /// The first line of the statement.
    pub fn span(&self) -> Span {
        match self {
            Stmt::When { span, .. } => *span,
            Stmt::Header(h) => Span {
                start_line: h.name_span.start_line,
                start_col: 1,
                end_line: h.value_span.end_line,
                end_col: h.value_span.end_col,
            },
            Stmt::Respond(r) => r.span,
        }
    }

    /// The last line of the statement: its `end` for a `when`.
    pub fn last_line(&self) -> u32 {
        match self {
            Stmt::When { end_span, .. } => end_span.start_line,
            other => other.span().start_line,
        }
    }
}

/// A `when` condition.
#[derive(Clone, Debug, PartialEq)]
pub struct Cond {
    /// The parameter tested.
    pub subject: String,
    /// Where it is.
    pub subject_span: Span,
    /// `when not p`.
    pub negated: bool,
    /// What is tested.
    pub test: CondTest,
}

/// The test in a condition.
#[derive(Clone, Debug, PartialEq)]
pub enum CondTest {
    /// `when p` / `when not p` — a bool.
    Truth,
    /// `when p is <literal>`.
    Is(Lit, Span),
    /// `when p is none`.
    IsNone(Span),
    /// `when p in <literal> …`.
    In(Vec<(Lit, Span)>),
    /// `when p at_least <n>`.
    AtLeast(i64, Span),
    /// `when p at_most <n>`.
    AtMost(i64, Span),
}

/// A literal value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lit {
    /// An integer.
    Int(i64),
    /// A string, without quotes, interpolation braces intact.
    Text(String),
    /// `true` / `false`.
    Bool(bool),
    /// `none`.
    None,
}

impl std::fmt::Display for Lit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lit::Int(v) => write!(f, "{v}"),
            Lit::Text(s) => write!(f, "\"{s}\""),
            Lit::Bool(b) => write!(f, "{b}"),
            Lit::None => f.write_str("none"),
        }
    }
}

/// A `respond` statement.
#[derive(Debug, PartialEq)]
pub struct Respond {
    /// The whole line.
    pub span: Span,
    /// The status, when one parsed.
    pub status: Option<(i64, Span)>,
    /// The body.
    pub body: Body,
}

/// A response body form (RFC-0011 §3).
#[derive(Debug, PartialEq)]
pub enum Body {
    /// No body.
    Empty,
    /// `json <record> <field> <value> …`.
    Json(RecordLit),
    /// `text "<string>"`, and where the string is.
    Text(String, Span),
    /// `file "<path>"`, and where the string is.
    File(String, Span),
}

impl Body {
    /// Where the body form is written, from its word to its end.
    pub fn span(&self) -> Option<Span> {
        match self {
            Body::Empty => None,
            Body::Json(r) => Some(r.span),
            Body::Text(_, s) | Body::File(_, s) => Some(*s),
        }
    }
}

/// A record literal: `<record> <field> <value> …`.
#[derive(Debug, PartialEq)]
pub struct RecordLit {
    /// The record's name.
    pub record: String,
    /// Where it is.
    pub record_span: Span,
    /// `(field, where, value)`, in the order written.
    pub fields: Vec<(String, Span, Value)>,
    /// From the body word to the last value.
    pub span: Span,
}

/// A value in a record literal.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A literal.
    Lit(Lit, Span),
    /// A parameter, or an enum variant for an enum-typed field.
    Name(String, Span),
}

impl Value {
    /// Where the value is.
    pub fn span(&self) -> Span {
        match self {
            Value::Lit(_, s) | Value::Name(_, s) => *s,
        }
    }
}

/// A service's `contract` block (RFC-0011 §7).
#[derive(Debug, Default, PartialEq)]
pub struct ServiceContract {
    /// The clauses, in source order.
    pub clauses: Vec<ServiceClause>,
}

/// One clause of a service contract.
#[derive(Clone, Debug, PartialEq)]
pub struct ServiceClause {
    /// The whole line.
    pub span: Span,
    /// What the clause asserts.
    pub kind: ClauseKind,
}

/// The two clause forms a service contract takes.
#[derive(Clone, Debug, PartialEq)]
pub enum ClauseKind {
    /// `example <method> "<target>" <check>` — one request, one fact.
    Example {
        /// The request method.
        method: Method,
        /// The request target, path and optional query, as written.
        target: String,
        /// The fact expected of the response.
        check: Check,
    },
    /// `ensure [when <check> then] <check>` — a fact of every response.
    Ensure {
        /// The guard, if any.
        when: Option<Check>,
        /// The fact.
        then: Check,
    },
}

/// A facet of a response and a predicate on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    /// What is read from the response.
    pub facet: Facet,
    /// What must hold of it.
    pub pred: Pred,
}

/// What a check reads from a response.
#[derive(Clone, Debug, PartialEq)]
pub enum Facet {
    /// The status code.
    Status,
    /// One header, by lower-case name.
    Header(String),
    /// The whole body, as text.
    Body,
    /// A value inside a JSON body, by field path.
    BodyPath(Vec<String>),
}

/// A predicate in a service contract.
#[derive(Clone, Debug, PartialEq)]
pub enum Pred {
    /// `is <value>`.
    Is(Lit),
    /// `in <value> …`.
    In(Vec<Lit>),
    /// `not_empty`.
    NotEmpty,
    /// `contains "<text>"`.
    Contains(String),
    /// `at_least <n>`.
    AtLeast(i64),
    /// `at_most <n>`.
    AtMost(i64),
}

impl Check {
    /// The check in one canonical spelling.
    pub fn canonical(&self) -> String {
        let facet = match &self.facet {
            Facet::Status => "status".to_string(),
            Facet::Header(name) => format!("header \"{name}\""),
            Facet::Body => "body".to_string(),
            Facet::BodyPath(path) => format!("body.{}", path.join(".")),
        };
        let pred = match &self.pred {
            Pred::Is(v) => format!("is {v}"),
            Pred::In(vs) => {
                let mut out = "in".to_string();
                for v in vs {
                    out.push_str(&format!(" {v}"));
                }
                out
            }
            Pred::NotEmpty => "not_empty".to_string(),
            Pred::Contains(t) => format!("contains \"{t}\""),
            Pred::AtLeast(n) => format!("at_least {n}"),
            Pred::AtMost(n) => format!("at_most {n}"),
        };
        format!("{facet} {pred}")
    }
}

impl ServiceClause {
    /// The clause in one canonical spelling, for diagnostics that quote it.
    pub fn canonical(&self) -> String {
        match &self.kind {
            ClauseKind::Example {
                method,
                target,
                check,
            } => format!(
                "example {} \"{target}\" {}",
                method.word(),
                check.canonical()
            ),
            ClauseKind::Ensure { when: None, then } => format!("ensure {}", then.canonical()),
            ClauseKind::Ensure {
                when: Some(w),
                then,
            } => format!("ensure when {} then {}", w.canonical(), then.canonical()),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------------------

/// Whether `c` may appear in a literal pattern segment: RFC 3986's unreserved characters,
/// lower-case only (RFC-0011 §2).
fn literal_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.' | '_' | '~')
}

/// Split a well-formed pattern into segments. `None` when it is malformed; [`check`]
/// reports why. `/` is the empty list.
pub fn segments(pattern: &str) -> Option<Vec<Segment>> {
    let rest = pattern.strip_prefix('/')?;
    if rest.is_empty() {
        return Some(Vec::new());
    }
    rest.split('/')
        .map(|seg| {
            if let Some(name) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                let ok = !name.is_empty()
                    && name.starts_with(|c: char| c.is_ascii_lowercase())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                ok.then(|| Segment::Param(name.to_string()))
            } else if !seg.is_empty() && seg.chars().all(literal_char) {
                Some(Segment::Literal(seg.to_string()))
            } else {
                None
            }
        })
        .collect()
}

/// The pattern with every parameter renamed to `{}`, so two routes that differ only in
/// parameter names compare equal.
fn shape(segs: &[Segment]) -> String {
    let mut out = String::new();
    for s in segs {
        out.push('/');
        match s {
            Segment::Literal(l) => out.push_str(l),
            Segment::Param(_) => out.push_str("{}"),
        }
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

/// Why a pattern is malformed, and the repaired pattern when the repair is certain.
fn pattern_problem(pattern: &str) -> (String, Option<String>) {
    if pattern.contains('?') {
        let path = pattern.split('?').next().unwrap_or("");
        return (
            format!(
                "`\"{pattern}\"` holds a query — declare `query <name>: option(<type>)` lines instead"
            ),
            (!path.is_empty()).then(|| path.to_string()),
        );
    }
    if !pattern.starts_with('/') {
        let fixed = format!("/{pattern}");
        let certain = segments(&fixed).is_some();
        return (
            format!("`\"{pattern}\"` must start with `/`"),
            certain.then_some(fixed),
        );
    }
    if pattern.len() > 1 && pattern.ends_with('/') {
        let fixed = pattern.trim_end_matches('/');
        let fixed = if fixed.is_empty() { "/" } else { fixed };
        let certain = segments(fixed).is_some();
        return (
            format!(
                "`\"{pattern}\"` ends with `/` — patterns have no trailing slash; the runtime redirects `…/` to the path without it"
            ),
            certain.then(|| fixed.to_string()),
        );
    }
    if pattern.contains("//") {
        let mut fixed = String::new();
        for seg in pattern.split('/').filter(|s| !s.is_empty()) {
            fixed.push('/');
            fixed.push_str(seg);
        }
        let certain = segments(&fixed).is_some();
        return (
            format!("`\"{pattern}\"` has an empty segment (`//`)"),
            certain.then_some(fixed),
        );
    }
    let lowered = pattern.to_ascii_lowercase();
    if lowered != pattern && segments(&lowered).is_some() {
        return (
            format!("`\"{pattern}\"` has upper-case letters — pattern literals are lower-case"),
            Some(lowered),
        );
    }
    (
        format!(
            "`\"{pattern}\"` is not a pattern — a segment is lower-case letters, digits and `-._~`, or a whole `{{name}}`"
        ),
        None,
    )
}

// ---------------------------------------------------------------------------------------
// The checker
// ---------------------------------------------------------------------------------------

/// Headers the server sets, which a handler cannot (RFC-0011 §3).
const SERVER_HEADERS: &[&str] = &["content-length", "transfer-encoding", "connection"];

/// The type of a parameter or field, as far as a handler can use it.
#[derive(Clone, Debug, PartialEq)]
pub enum PTy {
    /// `bool`, `int` or `text`.
    Scalar(Ty),
    /// `option` of a scalar.
    Option(Ty),
    /// An enum, by name.
    Enum(String),
    /// Anything a record literal cannot build (a record, a list).
    Other(String),
    /// Already reported.
    Unknown,
}

impl PTy {
    fn from_ty(ty: &Ty) -> PTy {
        match ty {
            Ty::Bool | Ty::Int | Ty::Text => PTy::Scalar(ty.clone()),
            Ty::Option(inner) if matches!(**inner, Ty::Bool | Ty::Int | Ty::Text) => {
                PTy::Option((**inner).clone())
            }
            Ty::Enum(name) => PTy::Enum(name.clone()),
            Ty::Unknown => PTy::Unknown,
            other => PTy::Other(other.to_string()),
        }
    }

    fn describe(&self) -> String {
        match self {
            PTy::Scalar(t) => t.to_string(),
            PTy::Option(t) => format!("option({t})"),
            PTy::Enum(n) => n.clone(),
            PTy::Other(s) => s.clone(),
            PTy::Unknown => "unknown".to_string(),
        }
    }
}

/// A synthetic component holding the service's enums and records, so the RFC-0008
/// resolver checks them exactly as it does a component's — one set of type rules.
pub fn type_scope(service: &Service) -> Component {
    Component {
        name: service.name.clone(),
        name_span: service.name_span,
        docs: Vec::new(),
        uses: service.uses.clone(),
        enums: service.enums.clone(),
        records: service.records.clone(),
        props: Vec::new(),
        view: None,
        fns: Vec::new(),
        emits: Vec::new(),
        broken: Vec::new(),
        contract: None,
    }
}

/// The resolved type of each query parameter of a route: `option(<scalar>)` or unknown.
pub fn query_type(ty: &TypeExpr) -> Option<Ty> {
    match &ty.kind {
        TypeKind::Apply(ctor, arg) if ctor == "option" => match &arg.kind {
            TypeKind::Name(n) => match n.as_str() {
                "bool" => Some(Ty::Bool),
                "int" => Some(Ty::Int),
                "text" => Some(Ty::Text),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Check a service. `file` is the path `mz` was given, used to find fixtures.
pub fn check(service: &Service, file: &str) -> Vec<Diagnostic> {
    let scope_component = type_scope(service);
    let mut diags = crate::resolve::resolve(&scope_component, file);
    let records = crate::resolve::record_types(&scope_component);
    let mut c = Checker {
        s: service,
        file: file.to_string(),
        diags: Vec::new(),
        records,
    };
    c.service_headers();
    c.routes();
    if let Some(fallback) = &service.fallback {
        c.handler(fallback, &BTreeMap::new(), "fallback");
    }
    c.contract();
    diags.append(&mut c.diags);
    diags
}

struct Checker<'a> {
    s: &'a Service,
    file: String,
    diags: Vec<Diagnostic>,
    records: BTreeMap<String, Vec<(String, Ty)>>,
}

/// The names a handler can read, and what each one is.
type Params = BTreeMap<String, PTy>;

impl Checker<'_> {
    fn err(&mut self, code: &'static str, span: Span, say: String) {
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say));
    }

    fn err_fix(
        &mut self,
        code: &'static str,
        span: Span,
        say: String,
        fix: (Span, String, Confidence),
    ) {
        self.diags
            .push(Diagnostic::error(code, &self.file, span, say).with_fix(fix.0, fix.1, fix.2));
    }

    /// A header name must be lower-case and one the handler may set.
    fn header_name(&mut self, h: &HeaderLine) -> bool {
        let lowered = h.name.to_ascii_lowercase();
        if h.name.is_empty()
            || !lowered
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
        {
            self.err(
                "MZ0807",
                h.name_span,
                format!("`\"{}\"` is not a header name", h.name),
            );
            return false;
        }
        if lowered != h.name {
            self.err_fix(
                "MZ0807",
                h.name_span,
                format!(
                    "header names are written lower-case — `\"{}\"` is `\"{lowered}\"`",
                    h.name
                ),
                (h.name_span, format!("\"{lowered}\""), Confidence::Exact),
            );
        }
        if SERVER_HEADERS.contains(&lowered.as_str()) {
            self.err(
                "MZ0807",
                h.name_span,
                format!("`{lowered}` is set by the server — delete this line"),
            );
            return false;
        }
        true
    }

    fn service_headers(&mut self) {
        let mut seen = BTreeSet::new();
        for h in &self.s.headers {
            if self.header_name(h) && !seen.insert(h.name.to_ascii_lowercase()) {
                self.err(
                    "MZ0807",
                    h.name_span,
                    format!(
                        "`header \"{}\"` is set twice at service level — delete one",
                        h.name
                    ),
                );
            }
            self.interpolations(&h.value, h.value_span, &BTreeMap::new());
        }
    }

    fn routes(&mut self) {
        let mut names = BTreeSet::new();
        let mut shapes: BTreeMap<(Method, String), (String, u32)> = BTreeMap::new();
        for route in &self.s.routes {
            if !names.insert(route.name.clone()) {
                self.err(
                    "MZ0812",
                    route.name_span,
                    format!("`route {}` is declared twice — rename one", route.name),
                );
            }
            let mut params: Params = BTreeMap::new();
            if let Some((pattern, span)) = &route.pattern {
                match segments(pattern) {
                    Some(segs) => {
                        for seg in &segs {
                            if let Segment::Param(p) = seg
                                && params.insert(p.clone(), PTy::Scalar(Ty::Text)).is_some()
                            {
                                self.err(
                                    "MZ0802",
                                    *span,
                                    format!("`\"{pattern}\"` binds `{{{p}}}` twice — rename one"),
                                );
                            }
                        }
                        if let Some((method, _)) = route.method {
                            let key = (method, shape(&segs));
                            if let Some((other, line)) = shapes.get(&key) {
                                self.err(
                                    "MZ0803",
                                    *span,
                                    format!(
                                        "`route {}` answers `{} \"{pattern}\"`, which `route {other}` on line {line} already answers",
                                        route.name,
                                        method.word()
                                    ),
                                );
                            } else {
                                shapes.insert(key, (route.name.clone(), span.start_line));
                            }
                        }
                    }
                    None => {
                        // The pattern is one mistake: its `{name}`s still bind, so their
                        // uses are not a second diagnostic each (RFC-0001 §4.1).
                        for part in pattern.split('/') {
                            if let Some(name) =
                                part.strip_prefix('{').and_then(|p| p.strip_suffix('}'))
                                && !name.is_empty()
                            {
                                params.insert(name.to_string(), PTy::Scalar(Ty::Text));
                            }
                        }
                        let (say, fix) = pattern_problem(pattern);
                        match fix {
                            Some(fixed) => self.err_fix(
                                "MZ0802",
                                *span,
                                say,
                                (*span, format!("\"{fixed}\""), Confidence::Exact),
                            ),
                            None => self.err("MZ0802", *span, say),
                        }
                    }
                }
            }
            for q in &route.queries {
                let ty = self.query(q);
                if params.contains_key(&q.name) {
                    self.err(
                        "MZ0812",
                        q.span,
                        format!(
                            "`query {}` has the same name as a path parameter of `route {}` — rename one",
                            q.name, route.name
                        ),
                    );
                    continue;
                }
                if params.insert(q.name.clone(), ty).is_some() {
                    self.err(
                        "MZ0812",
                        q.span,
                        format!("`query {}` is declared twice — delete one", q.name),
                    );
                }
            }
            self.handler(&route.handler, &params, &format!("route {}", route.name));
        }
    }

    /// A query parameter's type: `option(<scalar>)`, or an error (RFC-0011 §2).
    fn query(&mut self, q: &QueryDecl) -> PTy {
        if let Some(ty) = query_type(&q.ty) {
            return PTy::Option(ty);
        }
        let (inner, wrapped) = match &q.ty.kind {
            TypeKind::Apply(ctor, arg) if ctor == "option" => (&**arg, true),
            _ => (&q.ty, false),
        };
        let declared = |n: &str| {
            self.s.records.iter().any(|r| r.name == n)
                || self.s.enums.iter().any(|e| e.name == n)
                || matches!(n, "list" | "option" | "event")
        };
        match &inner.kind {
            TypeKind::Name(n) if matches!(n.as_str(), "bool" | "int" | "text") && !wrapped => {
                self.err_fix(
                    "MZ0809",
                    q.ty.span,
                    format!(
                        "`query {}: {n}` — a query parameter can always be absent, so it is `option({n})`",
                        q.name
                    ),
                    (q.ty.span, format!("option({n})"), Confidence::Exact),
                );
            }
            TypeKind::Name(n) if !declared(n) => {
                let say = format!(
                    "`{n}` is not a type — a query parameter is `option(bool)`, `option(int)` or `option(text)`"
                );
                match crate::resolve::alias_or_nearest(n, ["bool", "int", "text"]) {
                    Some((name, conf)) => self.err_fix(
                        "MZ0701",
                        q.ty.span,
                        say,
                        (q.ty.span, format!("option({name})"), conf),
                    ),
                    None => self.err("MZ0701", q.ty.span, say),
                }
            }
            _ => self.err(
                "MZ0809",
                q.ty.span,
                format!(
                    "`query {}: {}` — a query parameter is `option(bool)`, `option(int)` or `option(text)`",
                    q.name, q.ty
                ),
            ),
        }
        PTy::Unknown
    }

    /// Check one handler: its statements, and that every path responds exactly once.
    fn handler(&mut self, h: &Handler, params: &Params, what: &str) {
        let responds = self.block(&h.body, params, &BTreeSet::new(), &BTreeSet::new());
        if !responds && h.closed {
            let reason = match h.body.last() {
                None => "it has no statements".to_string(),
                Some(Stmt::When { span, .. }) => format!(
                    "when the `when` on line {} does not respond on both branches",
                    span.start_line
                ),
                Some(_) => "the last statement is not a `respond`".to_string(),
            };
            self.err(
                "MZ0804",
                h.end_span,
                format!(
                    "a path through `{what}` reaches `end` without a `respond` — {reason}; every path must respond"
                ),
            );
        }
    }

    /// Check a statement list. Returns whether every path through it responds.
    fn block(
        &mut self,
        stmts: &[Stmt],
        params: &Params,
        narrowed: &BTreeSet<String>,
        headers: &BTreeSet<String>,
    ) -> bool {
        let mut headers = headers.clone();
        let mut done = false;
        for stmt in stmts {
            if done {
                let span = stmt.span();
                let fix = Span {
                    start_line: span.start_line,
                    start_col: 1,
                    end_line: stmt.last_line() + 1,
                    end_col: 1,
                };
                self.err_fix(
                    "MZ0805",
                    span,
                    format!(
                        "line {} can never run — the `respond` above it already ended this path; delete it",
                        span.start_line
                    ),
                    (fix, String::new(), Confidence::Exact),
                );
                return true;
            }
            match stmt {
                Stmt::Header(h) => {
                    if self.header_name(h) {
                        let lowered = h.name.to_ascii_lowercase();
                        if !headers.insert(lowered.clone()) {
                            self.err(
                                "MZ0807",
                                h.name_span,
                                format!(
                                    "`header \"{lowered}\"` is already set on this path — delete one"
                                ),
                            );
                        }
                    }
                    self.interpolations_narrowed(&h.value, h.value_span, params, narrowed);
                }
                Stmt::Respond(r) => {
                    self.respond(r, params, narrowed);
                    done = true;
                }
                Stmt::When {
                    cond, then, els, ..
                } => {
                    let narrow = cond.as_ref().and_then(|c| self.cond(c, params, narrowed));
                    let then_done = self.block(then, params, narrowed, &headers);
                    let els_done = match els {
                        Some(e) => {
                            let mut n = narrowed.clone();
                            if let Some(p) = narrow {
                                n.insert(p);
                            }
                            self.block(e, params, &n, &headers)
                        }
                        None => false,
                    };
                    done = then_done && els_done;
                }
            }
        }
        done
    }

    /// Check a condition. Returns the parameter an `is none` narrows in its `else`.
    fn cond(&mut self, c: &Cond, params: &Params, narrowed: &BTreeSet<String>) -> Option<String> {
        let ty = self.lookup(&c.subject, c.subject_span, params)?;
        let ty = match ty {
            PTy::Option(t) if narrowed.contains(&c.subject) => PTy::Scalar(t),
            other => other,
        };
        if ty == PTy::Unknown {
            return None;
        }
        let bad = |this: &mut Self, say: String| {
            this.err("MZ0712", c.subject_span, say);
        };
        let s = &c.subject;
        match (&c.test, &ty) {
            (CondTest::IsNone(_), PTy::Option(_)) => {
                if c.negated {
                    bad(self, format!("`when not {s} is none` — write `when {s} is none` … `else`, which narrows `{s}` in the `else`"));
                    return None;
                }
                return Some(s.clone());
            }
            (CondTest::IsNone(_), other) => bad(
                self,
                format!("`{s}` is {}, which is never none", other.describe()),
            ),
            (_, PTy::Option(_)) => self.err(
                "MZ0710",
                c.subject_span,
                format!(
                    "`{s}` is an option and may be absent — test `when {s} is none` first, and use it in the `else`"
                ),
            ),
            (CondTest::Truth, PTy::Scalar(Ty::Bool)) => {}
            (CondTest::Truth, other) => bad(
                self,
                format!(
                    "`when {s}` tests a bool, and `{s}` is {} — compare it with `is` or `in`",
                    other.describe()
                ),
            ),
            (CondTest::Is(Lit::Bool(b), span), PTy::Scalar(Ty::Bool)) => {
                let fixed = if *b == c.negated {
                    format!("not {s}")
                } else {
                    s.clone()
                };
                let whole = Span {
                    start_line: c.subject_span.start_line,
                    start_col: c.subject_span.start_col,
                    end_line: span.end_line,
                    end_col: span.end_col,
                };
                self.err_fix(
                    "MZ0712",
                    whole,
                    format!("`{s} is {b}` — a bool is tested as `when {fixed}`, one form per intent"),
                    (whole, fixed, Confidence::Exact),
                );
            }
            (CondTest::Is(lit, _), PTy::Scalar(t)) => self.lit_fits(lit, t, c.subject_span, s),
            (CondTest::In(lits), PTy::Scalar(t)) if *t != Ty::Bool => {
                for (lit, _) in lits {
                    self.lit_fits(lit, t, c.subject_span, s);
                }
            }
            (CondTest::AtLeast(..) | CondTest::AtMost(..), PTy::Scalar(Ty::Int)) => {}
            (CondTest::AtLeast(..) | CondTest::AtMost(..), other) => bad(
                self,
                format!(
                    "`at_least` and `at_most` compare an int, and `{s}` is {}",
                    other.describe()
                ),
            ),
            (_, other) => bad(
                self,
                format!("`{s}` is {}, which this condition cannot test", other.describe()),
            ),
        }
        None
    }

    fn lit_fits(&mut self, lit: &Lit, ty: &Ty, span: Span, s: &str) {
        let ok = matches!(
            (lit, ty),
            (Lit::Int(_), Ty::Int) | (Lit::Text(_), Ty::Text) | (Lit::Bool(_), Ty::Bool)
        );
        if !ok {
            self.err(
                "MZ0712",
                span,
                format!("`{s}` is {ty}, and `{lit}` is not a {ty}"),
            );
        }
    }

    /// Resolve a name in a handler: a path or query parameter.
    fn lookup(&mut self, name: &str, span: Span, params: &Params) -> Option<PTy> {
        if let Some(t) = params.get(name) {
            return Some(t.clone());
        }
        let fix = nearest(name, params.keys().map(String::as_str));
        let in_scope = if params.is_empty() {
            "nothing is in scope here".to_string()
        } else {
            format!(
                "in scope: {}",
                params
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let say = format!("`{name}` names nothing — {in_scope}");
        match fix {
            Some((n, conf)) => self.err_fix("MZ0707", span, say, (span, n, conf)),
            None => self.err("MZ0707", span, say),
        }
        None
    }

    fn respond(&mut self, r: &Respond, params: &Params, narrowed: &BTreeSet<String>) {
        let Some((status, status_span)) = r.status else {
            return;
        };
        if !(100..=599).contains(&status) {
            self.err(
                "MZ0806",
                status_span,
                format!("`respond {status}` — a status is 100 to 599"),
            );
            return;
        }
        let bodyless = status < 200 || status == 204 || status == 304;
        if bodyless && let Some(span) = r.body.span() {
            let fix = Span {
                start_line: status_span.end_line,
                start_col: status_span.end_col,
                end_line: span.end_line,
                end_col: span.end_col,
            };
            self.err_fix(
                "MZ0806",
                span,
                format!("a `{status}` response has no body — delete the body"),
                (fix, String::new(), Confidence::Exact),
            );
            return;
        }
        match &r.body {
            Body::Empty => {}
            Body::Text(t, span) => self.interpolations_narrowed(t, *span, params, narrowed),
            Body::File(path, span) => self.fixture(path, *span),
            Body::Json(lit) => self.record_lit(lit, params, narrowed),
        }
    }

    fn fixture(&mut self, path: &str, span: Span) {
        let dir = Path::new(&self.file).parent().unwrap_or(Path::new(""));
        let full = dir.join(path);
        if path.is_empty() || Path::new(path).is_absolute() || path.split('/').any(|s| s == "..") {
            self.err(
                "MZ0810",
                span,
                format!("`file \"{path}\"` must be a relative path inside the service's directory"),
            );
        } else if !full.is_file() {
            self.err(
                "MZ0810",
                span,
                format!(
                    "`file \"{path}\"` does not exist next to this file (looked for `{}`)",
                    full.display()
                ),
            );
        }
    }

    fn record_lit(&mut self, lit: &RecordLit, params: &Params, narrowed: &BTreeSet<String>) {
        let Some(fields) = self.records.get(&lit.record).cloned() else {
            let names: Vec<&str> = self.s.records.iter().map(|r| r.name.as_str()).collect();
            let fix = nearest(&lit.record, names.iter().copied());
            let say = if names.is_empty() {
                format!(
                    "`json {}` names no record — this service declares none",
                    lit.record
                )
            } else {
                format!("`json {}` names no record of this service", lit.record)
            };
            match fix {
                Some((n, c)) => {
                    self.err_fix("MZ0701", lit.record_span, say, (lit.record_span, n, c))
                }
                None => self.err("MZ0701", lit.record_span, say),
            }
            return;
        };
        let mut given = BTreeSet::new();
        // A misspelt field is one mistake: its nearest name is not also reported missing.
        let mut meant = BTreeSet::new();
        for (name, span, value) in &lit.fields {
            let Some((_, ty)) = fields.iter().find(|(f, _)| f == name) else {
                let fix = nearest(name, fields.iter().map(|(f, _)| f.as_str()));
                if let Some((m, _)) = &fix {
                    meant.insert(m.clone());
                }
                let say = format!("`record {}` has no field `{name}`", lit.record);
                match fix {
                    Some((n, c)) => self.err_fix("MZ0708", *span, say, (*span, n, c)),
                    None => self.err("MZ0708", *span, say),
                }
                continue;
            };
            if !given.insert(name.clone()) {
                self.err(
                    "MZ0808",
                    *span,
                    format!("field `{name}` is given twice — delete one"),
                );
                continue;
            }
            self.field_value(
                &lit.record,
                name,
                &PTy::from_ty(ty),
                value,
                params,
                narrowed,
            );
        }
        let missing: Vec<&str> = fields
            .iter()
            .map(|(f, _)| f.as_str())
            .filter(|f| !given.contains(*f) && !meant.contains(*f))
            .collect();
        if !missing.is_empty() {
            self.err(
                "MZ0808",
                lit.record_span,
                format!(
                    "`json {}` leaves out {} — every field is given, `none` for an absent option",
                    lit.record,
                    missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
    }

    fn field_value(
        &mut self,
        record: &str,
        field: &str,
        want: &PTy,
        value: &Value,
        params: &Params,
        narrowed: &BTreeSet<String>,
    ) {
        let span = value.span();
        let mismatch = |this: &mut Self, got: String| {
            this.err(
                "MZ0808",
                span,
                format!(
                    "`{record}.{field}` is {}, and {got} is not",
                    want.describe()
                ),
            );
        };
        match (want, value) {
            (PTy::Unknown, _) => {}
            (PTy::Other(t), _) => self.err(
                "MZ0808",
                span,
                format!(
                    "`{record}.{field}` is {t}, which a record literal cannot build yet — a response record's fields are scalars or options of scalars (RFC-0011 §3)"
                ),
            ),
            (PTy::Enum(e), Value::Name(n, _)) => {
                let variants: Vec<String> = self
                    .s
                    .enums
                    .iter()
                    .find(|x| &x.name == e)
                    .map(|x| x.variants.iter().map(|v| v.name.clone()).collect())
                    .unwrap_or_default();
                if !variants.iter().any(|v| v == n) {
                    let fix = nearest(n, variants.iter().map(String::as_str));
                    let say = format!("`{n}` is not a variant of `{e}`");
                    match fix {
                        Some((v, c)) => self.err_fix("MZ0708", span, say, (span, v, c)),
                        None => self.err("MZ0708", span, say),
                    }
                }
            }
            (PTy::Enum(_), Value::Lit(l, _)) => mismatch(self, format!("`{l}`")),
            (want, Value::Lit(l, _)) => {
                let ok = match (want, l) {
                    (PTy::Option(_), Lit::None) => true,
                    (PTy::Scalar(t) | PTy::Option(t), l) => matches!(
                        (t, l),
                        (Ty::Int, Lit::Int(_)) | (Ty::Text, Lit::Text(_)) | (Ty::Bool, Lit::Bool(_))
                    ),
                    _ => false,
                };
                if !ok {
                    mismatch(self, format!("`{l}`"));
                } else if let Lit::Text(t) = l {
                    self.interpolations_narrowed(t, span, params, narrowed);
                }
            }
            (want, Value::Name(n, nspan)) => {
                let Some(got) = self.lookup(n, *nspan, params) else {
                    return;
                };
                let got = match got {
                    PTy::Option(t) if narrowed.contains(n) => PTy::Scalar(t),
                    other => other,
                };
                let ok = match (want, &got) {
                    (_, PTy::Unknown) => true,
                    (PTy::Scalar(a), PTy::Scalar(b)) => a == b,
                    (PTy::Option(a), PTy::Option(b) | PTy::Scalar(b)) => a == b,
                    _ => false,
                };
                if !ok {
                    if let (PTy::Scalar(a), PTy::Option(b)) = (want, &got)
                        && a == b
                    {
                        self.err(
                            "MZ0710",
                            *nspan,
                            format!(
                                "`{n}` is an option and may be absent, and `{record}.{field}` is {a} — test `when {n} is none` first, and use it in the `else`"
                            ),
                        );
                    } else {
                        mismatch(self, format!("`{n}` ({})", got.describe()));
                    }
                }
            }
        }
    }

    fn interpolations(&mut self, text: &str, span: Span, params: &Params) {
        self.interpolations_narrowed(text, span, params, &BTreeSet::new());
    }

    /// Every `{name}` in a string names a scalar in scope (RFC-0008 §5's rule for `{...}`).
    fn interpolations_narrowed(
        &mut self,
        text: &str,
        span: Span,
        params: &Params,
        narrowed: &BTreeSet<String>,
    ) {
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                self.err(
                    "MZ0714",
                    span,
                    format!("`{{` in `\"{text}\"` is never closed with `}}`"),
                );
                return;
            };
            let name = after[..close].trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                self.err(
                    "MZ0714",
                    span,
                    format!("`{{{name}}}` in `\"{text}\"` must name one parameter"),
                );
            } else if let Some(ty) = {
                // `span` is the string token, quote included; the name sits inside it.
                let offset = text[..text.len() - rest.len() + open + 1].chars().count()
                    + after[..close]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .count();
                let at = Span::single(
                    span.start_line,
                    span.start_col + 1 + offset as u32,
                    name.chars().count() as u32,
                );
                self.lookup(name, at, params)
            } {
                match ty {
                    PTy::Option(_) if !narrowed.contains(name) => self.err(
                        "MZ0710",
                        span,
                        format!(
                            "`{{{name}}}` is an option and may be absent — test `when {name} is none` first, and interpolate it in the `else`"
                        ),
                    ),
                    PTy::Other(t) => self.err(
                        "MZ0711",
                        span,
                        format!("`{{{name}}}` is {t}, which cannot be written into text"),
                    ),
                    _ => {}
                }
            }
            rest = &after[close + 1..];
        }
    }

    /// Contract checks `mz check` makes (RFC-0011 §7): header names, targets, contradictions.
    fn contract(&mut self) {
        let Some(contract) = &self.s.contract else {
            self.diags.push(Diagnostic::warning(
                "MZ0613",
                &self.file,
                self.s.name_span,
                format!(
                    "`service {}` has no `contract` block — its behaviour is unverified (RFC-0010 §8)",
                    self.s.name
                ),
            ));
            return;
        };
        if contract.clauses.is_empty() {
            self.diags.push(Diagnostic::warning(
                "MZ0613",
                &self.file,
                self.s.name_span,
                format!(
                    "`service {}` has an empty `contract` block — nothing is checked (RFC-0010 §8)",
                    self.s.name
                ),
            ));
        }
        let mut allowed: Option<(Vec<i64>, u32)> = None;
        for clause in &contract.clauses {
            if let ClauseKind::Ensure {
                when: None,
                then:
                    Check {
                        facet: Facet::Status,
                        pred: Pred::In(set),
                    },
            } = &clause.kind
            {
                let ints = set
                    .iter()
                    .filter_map(|l| match l {
                        Lit::Int(v) => Some(*v),
                        _ => None,
                    })
                    .collect();
                allowed = Some((ints, clause.span.start_line));
            }
        }
        for clause in &contract.clauses {
            let checks: Vec<&Check> = match &clause.kind {
                ClauseKind::Example { target, check, .. } => {
                    if !target.starts_with('/') {
                        self.err(
                            "MZ0601",
                            clause.span,
                            format!("`example … \"{target}\"` — a request target starts with `/`"),
                        );
                    }
                    if let (
                        Some((set, line)),
                        Check {
                            facet: Facet::Status,
                            pred: Pred::Is(Lit::Int(v)),
                        },
                    ) = (&allowed, check)
                        && !set.contains(v)
                    {
                        self.err(
                            "MZ0606",
                            clause.span,
                            format!(
                                "`{}` expects status {v}, which the `ensure status in …` on line {line} rules out — one of them is wrong",
                                clause.canonical()
                            ),
                        );
                    }
                    vec![check]
                }
                ClauseKind::Ensure { when, then } => when.iter().chain([then]).collect(),
            };
            for check in checks {
                if let Facet::Header(name) = &check.facet
                    && name.to_ascii_lowercase() != *name
                {
                    self.err(
                        "MZ0601",
                        clause.span,
                        format!(
                            "header names are lower-case — write `header \"{}\"`",
                            name.to_ascii_lowercase()
                        ),
                    );
                }
                let fits = match (&check.facet, &check.pred) {
                    (Facet::Status, Pred::Is(Lit::Int(_)) | Pred::AtLeast(_) | Pred::AtMost(_)) => {
                        true
                    }
                    (Facet::Status, Pred::In(vs)) => vs.iter().all(|v| matches!(v, Lit::Int(_))),
                    (Facet::Status, _) => false,
                    (Facet::Header(_) | Facet::Body, Pred::Is(Lit::Text(_) | Lit::None)) => true,
                    (Facet::Header(_) | Facet::Body, Pred::In(vs)) => {
                        vs.iter().all(|v| matches!(v, Lit::Text(_)))
                    }
                    (Facet::Header(_) | Facet::Body, Pred::NotEmpty | Pred::Contains(_)) => true,
                    (Facet::Header(_) | Facet::Body, _) => false,
                    (Facet::BodyPath(_), _) => true,
                };
                if !fits {
                    self.err(
                        "MZ0601",
                        clause.span,
                        format!(
                            "`{}` — that predicate does not fit that facet: `status` is a number, a header and `body` are text",
                            check.canonical()
                        ),
                    );
                }
            }
        }
    }
}
