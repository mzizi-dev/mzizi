//! Running a service in process: RFC-0011 §6's algorithm, the handler statements, and the
//! contract evaluator of §7.
//!
//! The handler language has no loop, no call and no state, so running a handler is exact
//! and always terminates. That is what lets `mz contract` test a service with no lowering
//! and no socket. The lowering (`crate::lower`) emits the same algorithm as Rust, and its
//! generated tests are the same `example` clauses, so the two are checked against one list
//! (RFC-0011 HD-5).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::contract::Tally;
use crate::diagnostic::{Diagnostic, Span, json_string};
use crate::service::{
    Body, Check, ClauseKind, Cond, CondTest, Facet, Handler, Lit, Method, Pred, Route, Segment,
    Service, Stmt, Value, query_type, segments,
};

/// One request: a method token and a target, a path with an optional `?query`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Request {
    /// The method as HTTP spells it, e.g. `GET`.
    pub method: String,
    /// The request target as sent, e.g. `/v1/ui?limit=3`.
    pub target: String,
}

/// One response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// Headers, lower-case names, in the order they were set.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

impl Response {
    /// A header's value, by lower-case name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn empty(status: u16) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

/// A value a handler holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val {
    /// A text.
    Text(String),
    /// An int.
    Int(i64),
    /// A bool.
    Bool(bool),
    /// An absent option.
    None,
}

impl Val {
    fn render(&self) -> String {
        match self {
            Val::Text(t) => t.clone(),
            Val::Int(v) => v.to_string(),
            Val::Bool(b) => b.to_string(),
            Val::None => String::new(),
        }
    }
}

/// A service ready to answer requests, with its fixtures read.
pub struct Runtime<'a> {
    service: &'a Service,
    fixtures: BTreeMap<String, Vec<u8>>,
}

impl<'a> Runtime<'a> {
    /// Prepare `service`, reading its fixtures relative to `file`'s directory. `mz check`
    /// has already proved they exist; one that cannot be read answers as an empty body.
    pub fn new(service: &'a Service, file: &str) -> Runtime<'a> {
        let dir = Path::new(file).parent().unwrap_or(Path::new(""));
        let mut fixtures = BTreeMap::new();
        let mut walk = |stmts: &[Stmt]| collect_files(stmts, &mut fixtures, dir);
        for r in &service.routes {
            walk(&r.handler.body);
        }
        if let Some(f) = &service.fallback {
            walk(&f.body);
        }
        Runtime { service, fixtures }
    }

    /// Answer one request: RFC-0011 §6, steps 1–4.
    pub fn handle(&self, req: &Request) -> Response {
        let mut res = self.route(req);
        for h in &self.service.headers {
            if res.header(&h.name).is_none() {
                res.headers
                    .push((h.name.clone(), interpolate(&h.value, &BTreeMap::new())));
            }
        }
        // A value holding a control character is not a header value (RFC 9110 §5.5). It
        // is dropped, exactly as the lowered server drops what `HeaderValue` refuses.
        res.headers
            .retain(|(_, v)| !v.chars().any(|c| (c < ' ' && c != '\t') || c == '\u{7f}'));
        res
    }

    fn route(&self, req: &Request) -> Response {
        let (path, query) = match req.target.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (req.target.as_str(), None),
        };
        // 1. Canonical path.
        if let Some(location) = canonical_redirect(path, query) {
            let mut res = Response::empty(308);
            res.headers.push(("location".to_string(), location));
            return res;
        }
        // 2. Match.
        let raw: Vec<&str> = path.split('/').skip(1).filter(|s| !s.is_empty()).collect();
        let mut best: Option<(Vec<bool>, Vec<&Route>)> = None;
        for route in &self.service.routes {
            let Some(segs) = route.pattern.as_ref().and_then(|(p, _)| segments(p)) else {
                continue;
            };
            let Some(key) = match_key(&segs, &raw) else {
                continue;
            };
            match &mut best {
                Some((k, group)) if *k == key => group.push(route),
                Some((k, _)) if key < *k => best = Some((key, vec![route])),
                Some(_) => {}
                None => best = Some((key, vec![route])),
            }
        }
        let Some((_, group)) = best else {
            return match &self.service.fallback {
                Some(f) => self.run(f, &BTreeMap::new()),
                None => Response::empty(404),
            };
        };
        // 3. Method.
        let method = Method::from_http(&req.method);
        let find = |m: Method| {
            group
                .iter()
                .copied()
                .find(|r| r.method.map(|x| x.0) == Some(m))
        };
        if let Some(route) = method.and_then(find) {
            return self.run_route(route, &raw, query);
        }
        if method == Some(Method::Head)
            && let Some(route) = find(Method::Get)
        {
            let mut res = self.run_route(route, &raw, query);
            res.body.clear();
            return res;
        }
        let allow = allow(group.iter().filter_map(|r| r.method.map(|m| m.0)));
        let status = if method == Some(Method::Options) {
            204
        } else {
            405
        };
        let mut res = Response::empty(status);
        res.headers.push(("allow".to_string(), allow));
        res
    }

    fn run_route(&self, route: &Route, raw: &[&str], query: Option<&str>) -> Response {
        let mut env = BTreeMap::new();
        if let Some(segs) = route.pattern.as_ref().and_then(|(p, _)| segments(p)) {
            for (seg, value) in segs.iter().zip(raw) {
                if let Segment::Param(name) = seg {
                    env.insert(name.clone(), Val::Text(percent_decode(value, false)));
                }
            }
        }
        let pairs = query.map(parse_query).unwrap_or_default();
        for q in &route.queries {
            let value = pairs
                .iter()
                .find(|(k, _)| *k == q.name)
                .map(|(_, v)| v.as_str());
            let val = match (query_type(&q.ty), value) {
                (Some(ty), Some(v)) => decode_query(&ty, v),
                _ => Val::None,
            };
            env.insert(q.name.clone(), val);
        }
        self.run(&route.handler, &env)
    }

    fn run(&self, handler: &Handler, env: &BTreeMap<String, Val>) -> Response {
        let mut headers = Vec::new();
        match self.block(&handler.body, env, &mut headers) {
            Some(res) => res,
            // `mz check` proves every path responds (MZ0804); an unchecked tree that does
            // not is a server error, never a silent 200.
            None => Response::empty(500),
        }
    }

    fn block(
        &self,
        stmts: &[Stmt],
        env: &BTreeMap<String, Val>,
        headers: &mut Vec<(String, String)>,
    ) -> Option<Response> {
        for stmt in stmts {
            match stmt {
                Stmt::Header(h) => {
                    headers.push((h.name.to_ascii_lowercase(), interpolate(&h.value, env)))
                }
                Stmt::When {
                    cond, then, els, ..
                } => {
                    let holds = cond.as_ref().is_some_and(|c| holds(c, env));
                    let branch = if holds { Some(then) } else { els.as_ref() };
                    if let Some(branch) = branch
                        && let Some(res) = self.block(branch, env, headers)
                    {
                        return Some(res);
                    }
                }
                Stmt::Respond(r) => {
                    let status = r.status.map_or(500, |(s, _)| s) as u16;
                    let (body, content_type) = match &r.body {
                        Body::Empty => (Vec::new(), None),
                        Body::Text(t, _) => (
                            interpolate(t, env).into_bytes(),
                            Some("text/plain; charset=utf-8"),
                        ),
                        Body::File(path, _) => (
                            self.fixtures.get(path).cloned().unwrap_or_default(),
                            Some(content_type_for(path)),
                        ),
                        Body::Json(lit) => (
                            self.record_json(&lit.record, &lit.fields, env).into_bytes(),
                            Some("application/json"),
                        ),
                    };
                    let mut all = headers.clone();
                    if let Some(ct) = content_type
                        && !all.iter().any(|(n, _)| n == "content-type")
                    {
                        all.push(("content-type".to_string(), ct.to_string()));
                    }
                    return Some(Response {
                        status,
                        headers: all,
                        body,
                    });
                }
            }
        }
        None
    }

    /// A record literal as JSON, fields in declaration order (RFC-0011 §4.1).
    fn record_json(
        &self,
        record: &str,
        fields: &[(String, Span, Value)],
        env: &BTreeMap<String, Val>,
    ) -> String {
        let Some(decl) = self.service.records.iter().find(|r| r.name == record) else {
            return "{}".to_string();
        };
        let mut out = String::from("{");
        for (i, f) in decl.fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&json_string(&f.name));
            out.push(':');
            let value = fields.iter().find(|(n, _, _)| *n == f.name).map(|x| &x.2);
            let val = match value {
                Some(Value::Lit(l, _)) => lit_val(l, env),
                Some(Value::Name(n, _)) => match env.get(n) {
                    Some(v) => v.clone(),
                    // A bare name that is not a parameter is an enum variant.
                    None => Val::Text(n.clone()),
                },
                None => Val::None,
            };
            out.push_str(&match val {
                Val::Text(t) => json_string(&t),
                Val::Int(v) => v.to_string(),
                Val::Bool(b) => b.to_string(),
                Val::None => "null".to_string(),
            });
        }
        out.push('}');
        out
    }
}

fn collect_files(stmts: &[Stmt], out: &mut BTreeMap<String, Vec<u8>>, dir: &Path) {
    for s in stmts {
        match s {
            Stmt::Respond(r) => {
                if let Body::File(path, _) = &r.body {
                    let bytes = std::fs::read(dir.join(path)).unwrap_or_default();
                    out.insert(path.clone(), bytes);
                }
            }
            Stmt::When { then, els, .. } => {
                collect_files(then, out, dir);
                if let Some(e) = els {
                    collect_files(e, out, dir);
                }
            }
            Stmt::Header(_) => {}
        }
    }
}

/// The content type a fixture's extension implies (RFC-0011 §3).
pub fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Step 1: the `location` of a `308`, when `path` is not canonical.
pub fn canonical_redirect(path: &str, query: Option<&str>) -> Option<String> {
    if path == "/" || !path.starts_with('/') {
        return None;
    }
    let parts: Vec<&str> = path.split('/').skip(1).collect();
    if !parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let kept: Vec<&str> = parts.into_iter().filter(|p| !p.is_empty()).collect();
    let mut location = format!("/{}", kept.join("/"));
    if let Some(q) = query {
        location.push('?');
        location.push_str(q);
    }
    Some(location)
}

/// Whether `segs` matches `raw`, and its specificity: one `false` per literal and `true`
/// per parameter, so the smaller key is the better match (a literal beats a parameter).
fn match_key(segs: &[Segment], raw: &[&str]) -> Option<Vec<bool>> {
    if segs.len() != raw.len() {
        return None;
    }
    let mut key = Vec::with_capacity(segs.len());
    for (seg, value) in segs.iter().zip(raw) {
        match seg {
            Segment::Literal(l) if l == value => key.push(false),
            Segment::Literal(_) => return None,
            Segment::Param(_) => key.push(true),
        }
    }
    Some(key)
}

/// The `allow` header for a set of declared methods (RFC-0011 §6, step 3).
pub fn allow(declared: impl Iterator<Item = Method>) -> String {
    let mut set: BTreeSet<Method> = declared.collect();
    if set.contains(&Method::Get) {
        set.insert(Method::Head);
    }
    set.insert(Method::Options);
    Method::ALL
        .into_iter()
        .filter(|m| set.contains(m))
        .map(Method::http)
        .collect::<Vec<_>>()
        .join(", ")
}

/// `application/x-www-form-urlencoded` pairs, decoded (RFC-0011 §4.2).
pub fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (percent_decode(k, true), percent_decode(v, true)),
            None => (percent_decode(p, true), String::new()),
        })
        .collect()
}

/// Percent-decode; with `plus`, `+` is a space. A malformed escape is kept as written, and
/// bytes that are not UTF-8 are replaced.
pub fn percent_decode(s: &str, plus: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(if plus && b == b'+' { b' ' } else { b });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The query decoding rule of RFC-0011 §4.2, for one present value.
pub fn decode_query(ty: &crate::resolve::Ty, v: &str) -> Val {
    use crate::resolve::Ty;
    match ty {
        Ty::Text => Val::Text(v.to_string()),
        Ty::Bool => match v {
            "true" => Val::Bool(true),
            "false" => Val::Bool(false),
            _ => Val::None,
        },
        Ty::Int => {
            let digits = v.strip_prefix('-').unwrap_or(v);
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                v.parse::<i64>().map_or(Val::None, Val::Int)
            } else {
                Val::None
            }
        }
        _ => Val::None,
    }
}

fn lit_val(l: &Lit, env: &BTreeMap<String, Val>) -> Val {
    match l {
        Lit::Int(v) => Val::Int(*v),
        Lit::Text(t) => Val::Text(interpolate(t, env)),
        Lit::Bool(b) => Val::Bool(*b),
        Lit::None => Val::None,
    }
}

fn holds(c: &Cond, env: &BTreeMap<String, Val>) -> bool {
    let v = env.get(&c.subject).cloned().unwrap_or(Val::None);
    let eq = |l: &Lit| match (l, &v) {
        (Lit::Int(a), Val::Int(b)) => a == b,
        (Lit::Text(a), Val::Text(b)) => a == b,
        (Lit::Bool(a), Val::Bool(b)) => a == b,
        (Lit::None, Val::None) => true,
        _ => false,
    };
    let result = match &c.test {
        CondTest::Truth => v == Val::Bool(true),
        CondTest::Is(l, _) => eq(l),
        CondTest::IsNone(_) => v == Val::None,
        CondTest::In(set) => set.iter().any(|(l, _)| eq(l)),
        CondTest::AtLeast(n, _) => matches!(v, Val::Int(x) if x >= *n),
        CondTest::AtMost(n, _) => matches!(v, Val::Int(x) if x <= *n),
    };
    result != c.negated
}

/// Replace every `{name}` with the named value.
pub fn interpolate(text: &str, env: &BTreeMap<String, Val>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = after[..close].trim();
        out.push_str(&env.get(name).map(Val::render).unwrap_or_default());
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------------------
// JSON, read back for `body.<field>` checks
// ---------------------------------------------------------------------------------------

/// A parsed JSON value. Numbers keep their text, so `1.0` and `1` stay distinct.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// `null`
    Null,
    /// `true` / `false`
    Bool(bool),
    /// A number, as written.
    Number(String),
    /// A string.
    Str(String),
    /// An array.
    Array(Vec<Json>),
    /// An object, members in order.
    Object(Vec<(String, Json)>),
}

/// Parse a JSON text. `None` when it is not one.
pub fn parse_json(text: &str) -> Option<Json> {
    let mut p = JsonParser {
        s: text.as_bytes(),
        i: 0,
    };
    let v = p.value()?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl JsonParser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        match *self.s.get(self.i)? {
            b'n' => self.eat("null").then_some(Json::Null),
            b't' => self.eat("true").then_some(Json::Bool(true)),
            b'f' => self.eat("false").then_some(Json::Bool(false)),
            b'"' => self.string().map(Json::Str),
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.eat("]") {
                    return Some(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.ws();
                    if self.eat("]") {
                        return Some(Json::Array(items));
                    }
                    if !self.eat(",") {
                        return None;
                    }
                }
            }
            b'{' => {
                self.i += 1;
                let mut members = Vec::new();
                self.ws();
                if self.eat("}") {
                    return Some(Json::Object(members));
                }
                loop {
                    self.ws();
                    let key = self.string()?;
                    self.ws();
                    if !self.eat(":") {
                        return None;
                    }
                    let v = self.value()?;
                    members.push((key, v));
                    self.ws();
                    if self.eat("}") {
                        return Some(Json::Object(members));
                    }
                    if !self.eat(",") {
                        return None;
                    }
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len()
                    && matches!(
                        self.s[self.i],
                        b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
                    )
                {
                    self.i += 1;
                }
                let text = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                text.parse::<f64>().ok()?;
                Some(Json::Number(text.to_string()))
            }
            _ => None,
        }
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat("\"") {
            return None;
        }
        let mut out = Vec::new();
        loop {
            let b = *self.s.get(self.i)?;
            self.i += 1;
            match b {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let mut code = u32::from_str_radix(hex, 16).ok()?;
                            if (0xD800..0xDC00).contains(&code) && self.eat("\\u") {
                                let lo =
                                    std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                                self.i += 4;
                                let lo = u32::from_str_radix(lo, 16).ok()?;
                                code = 0x10000 + ((code - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            let c = char::from_u32(code).unwrap_or('\u{fffd}');
                            let mut buf = [0; 4];
                            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return None,
                    }
                }
                _ => out.push(b),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// The contract evaluator (RFC-0011 §7)
// ---------------------------------------------------------------------------------------

/// What a facet read from a response.
enum Read {
    Int(i64),
    Text(String),
    Absent,
    /// A JSON body without the field a `body.<field>` check reads. It fails every
    /// predicate, `is none` included: `none` there means JSON `null` (RFC-0011 §7).
    Missing,
    Json(Json),
    /// It could not be read; the reason.
    Unreadable(String),
}

fn read(res: &Response, facet: &Facet) -> Read {
    match facet {
        Facet::Status => Read::Int(res.status as i64),
        Facet::Header(name) => match res.header(name) {
            Some(v) => Read::Text(v.to_string()),
            None => Read::Absent,
        },
        Facet::Body => Read::Text(String::from_utf8_lossy(&res.body).into_owned()),
        Facet::BodyPath(path) => {
            let text = String::from_utf8_lossy(&res.body);
            let Some(mut v) = parse_json(&text) else {
                return Read::Unreadable("the body is not JSON".to_string());
            };
            for seg in path {
                let next = match &v {
                    Json::Object(members) => {
                        members.iter().find(|(k, _)| k == seg).map(|x| x.1.clone())
                    }
                    _ => None,
                };
                match next {
                    Some(n) => v = n,
                    None => return Read::Missing,
                }
            }
            Read::Json(v)
        }
    }
}

fn describe_read(r: &Read) -> String {
    match r {
        Read::Int(v) => v.to_string(),
        Read::Text(t) => format!("\"{}\"", clip(t)),
        Read::Absent => "absent".to_string(),
        Read::Missing => "not in the body".to_string(),
        Read::Json(j) => clip(&json_text(j)),
        Read::Unreadable(why) => why.clone(),
    }
}

fn json_text(j: &Json) -> String {
    match j {
        Json::Null => "null".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.clone(),
        Json::Str(s) => json_string(s),
        Json::Array(items) => format!(
            "[{}]",
            items.iter().map(json_text).collect::<Vec<_>>().join(",")
        ),
        Json::Object(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("{}:{}", json_string(k), json_text(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn clip(s: &str) -> String {
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(60).collect::<String>())
    } else {
        s.to_string()
    }
}

/// `Ok(true)` holds, `Ok(false)` does not, `Err` cannot be evaluated.
fn judge(res: &Response, check: &Check) -> Result<bool, String> {
    let r = read(res, &check.facet);
    if let Read::Unreadable(why) = &r {
        return Err(why.clone());
    }
    let num = |r: &Read| match r {
        Read::Int(v) => Some(*v as f64),
        Read::Json(Json::Number(n)) => n.parse::<f64>().ok(),
        _ => None,
    };
    let text = |r: &Read| match r {
        Read::Text(t) => Some(t.clone()),
        Read::Json(Json::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let is = |l: &Lit| match l {
        Lit::Int(v) => num(&r) == Some(*v as f64),
        Lit::Text(t) => text(&r).as_deref() == Some(t.as_str()),
        Lit::Bool(b) => matches!(&r, Read::Json(Json::Bool(x)) if x == b),
        Lit::None => matches!(&r, Read::Absent | Read::Json(Json::Null)),
    };
    Ok(match &check.pred {
        Pred::Is(l) => is(l),
        Pred::In(set) => set.iter().any(is),
        Pred::NotEmpty => match &r {
            Read::Text(t) => !t.trim().is_empty(),
            Read::Json(Json::Str(s)) => !s.trim().is_empty(),
            Read::Json(Json::Array(a)) => !a.is_empty(),
            Read::Json(Json::Object(o)) => !o.is_empty(),
            _ => false,
        },
        Pred::Contains(t) => text(&r).is_some_and(|x| x.contains(t.as_str())),
        Pred::AtLeast(n) => num(&r).is_some_and(|x| x >= *n as f64),
        Pred::AtMost(n) => num(&r).is_some_and(|x| x <= *n as f64),
    })
}

/// The request set every `ensure` is checked over (RFC-0011 §7): deterministic,
/// enumerated, sorted.
pub fn request_set(service: &Service) -> Vec<Request> {
    let mut out: BTreeSet<Request> = BTreeSet::new();
    let mut add = |method: &str, target: String| {
        out.insert(Request {
            method: method.to_string(),
            target,
        });
    };
    let mut paths = Vec::new();
    for route in &service.routes {
        let Some(segs) = route.pattern.as_ref().and_then(|(p, _)| segments(p)) else {
            continue;
        };
        let mut compared: BTreeMap<String, BTreeSet<Lit>> = BTreeMap::new();
        literals(&route.handler.body, &mut compared);
        let params: Vec<&String> = segs
            .iter()
            .filter_map(|s| match s {
                Segment::Param(p) => Some(p),
                _ => None,
            })
            .collect();
        let values = |p: &str| -> Vec<String> {
            let mut v: Vec<String> = compared
                .get(p)
                .into_iter()
                .flatten()
                .filter_map(|l| match l {
                    Lit::Text(t) if !t.is_empty() && !t.contains('/') => Some(t.clone()),
                    Lit::Int(i) => Some(i.to_string()),
                    _ => None,
                })
                .collect();
            let mut other = "zz".to_string();
            while v.contains(&other) {
                other.push('z');
            }
            v.push(other);
            v
        };
        let build = |choice: &BTreeMap<&str, String>| {
            let mut path = String::new();
            for s in &segs {
                path.push('/');
                match s {
                    Segment::Literal(l) => path.push_str(l),
                    Segment::Param(p) => path.push_str(&choice[p.as_str()]),
                }
            }
            if path.is_empty() {
                path.push('/');
            }
            path
        };
        let base: BTreeMap<&str, String> = params
            .iter()
            .map(|p| (p.as_str(), values(p)[0].clone()))
            .collect();
        let mut route_paths = vec![build(&base)];
        for p in &params {
            for v in values(p) {
                let mut c = base.clone();
                c.insert(p.as_str(), v);
                route_paths.push(build(&c));
            }
        }
        let method = route.method.map_or("GET", |m| m.0.http());
        for path in &route_paths {
            for q in &route.queries {
                let mut vs: Vec<String> = vec!["abc".into(), "-1".into(), "0".into()];
                for l in compared.get(&q.name).into_iter().flatten() {
                    match l {
                        Lit::Text(t) => vs.push(t.clone()),
                        Lit::Int(i) => {
                            vs.push(i.to_string());
                            vs.push((i - 1).to_string());
                        }
                        Lit::Bool(b) => vs.push(b.to_string()),
                        Lit::None => {}
                    }
                }
                for v in vs {
                    add(method, format!("{path}?{}={}", q.name, encode(&v)));
                }
            }
        }
        paths.extend(route_paths);
    }
    paths.push("/".to_string());
    let mut unknown = "/mz-unknown".to_string();
    while paths.contains(&unknown) {
        unknown.push('z');
    }
    paths.push(unknown);
    for path in &paths {
        for m in Method::ALL {
            add(m.http(), path.clone());
        }
        if path != "/" {
            add("GET", format!("{path}/"));
            add("GET", format!("/{path}"));
        }
    }
    out.into_iter().collect()
}

fn encode(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Every literal a handler compares each parameter with, and `at_least`/`at_most` bounds.
fn literals(stmts: &[Stmt], out: &mut BTreeMap<String, BTreeSet<Lit>>) {
    for s in stmts {
        if let Stmt::When {
            cond, then, els, ..
        } = s
        {
            if let Some(c) = cond {
                let set = out.entry(c.subject.clone()).or_default();
                match &c.test {
                    CondTest::Is(l, _) => {
                        set.insert(l.clone());
                    }
                    CondTest::In(ls) => set.extend(ls.iter().map(|(l, _)| l.clone())),
                    CondTest::AtLeast(n, _) | CondTest::AtMost(n, _) => {
                        set.insert(Lit::Int(*n));
                        set.insert(Lit::Int(n + 1));
                    }
                    CondTest::Truth => {
                        set.insert(Lit::Bool(true));
                        set.insert(Lit::Bool(false));
                    }
                    CondTest::IsNone(_) => {}
                }
            }
            literals(then, out);
            if let Some(e) = els {
                literals(e, out);
            }
        }
    }
}

/// Evaluate a service's contract by running it (RFC-0011 §7).
pub fn evaluate(service: &Service, file: &str) -> (Tally, Vec<Diagnostic>) {
    let rt = Runtime::new(service, file);
    let mut tally = Tally::default();
    let mut diags = Vec::new();
    let Some(contract) = &service.contract else {
        tally.tested = Some(0);
        return (tally, diags);
    };
    let has_ensure = contract
        .clauses
        .iter()
        .any(|c| matches!(c.kind, ClauseKind::Ensure { .. }));
    let generated: Vec<(Request, Response)> = if has_ensure {
        request_set(service)
            .into_iter()
            .map(|r| {
                let res = rt.handle(&r);
                (r, res)
            })
            .collect()
    } else {
        Vec::new()
    };
    tally.tested = Some(generated.len());
    for clause in &contract.clauses {
        tally.clauses += 1;
        let written = clause.canonical();
        let outcome: Result<Option<String>, String> = match &clause.kind {
            ClauseKind::Example {
                method,
                target,
                check,
            } => {
                let res = rt.handle(&Request {
                    method: method.http().to_string(),
                    target: target.clone(),
                });
                judge(&res, check).map(|ok| {
                    (!ok).then(|| {
                        format!(
                            "{} is {}",
                            facet_name(&check.facet),
                            describe_read(&read(&res, &check.facet))
                        )
                    })
                })
            }
            ClauseKind::Ensure { when, then } => {
                let mut result = Ok(None);
                let reads_body = |c: &Check| matches!(c.facet, Facet::Body | Facet::BodyPath(_));
                let body_clause = reads_body(then) || when.as_ref().is_some_and(reads_body);
                for (req, res) in &generated {
                    // A `HEAD` response has no body by definition (RFC 9110 §9.3.2), so a
                    // clause about the body says nothing about one.
                    if body_clause && req.method == "HEAD" {
                        continue;
                    }
                    let applies = match when {
                        Some(w) => judge(res, w).unwrap_or(false),
                        None => true,
                    };
                    if !applies {
                        continue;
                    }
                    match judge(res, then) {
                        Ok(true) => {}
                        Ok(false) => {
                            result = Ok(Some(format!(
                                "{} \"{}\" gives {} {} ({} requests)",
                                Method::from_http(&req.method).map_or("get", Method::word),
                                req.target,
                                facet_name(&then.facet),
                                describe_read(&read(res, &then.facet)),
                                generated.len()
                            )));
                            break;
                        }
                        Err(why) => {
                            result = Err(format!(
                                "{why}, for {} \"{}\"",
                                Method::from_http(&req.method).map_or("get", Method::word),
                                req.target
                            ));
                            break;
                        }
                    }
                }
                result
            }
        };
        let code = match clause.kind {
            ClauseKind::Example { .. } => "MZ0612",
            ClauseKind::Ensure { .. } => "MZ0611",
        };
        match outcome {
            Ok(None) => {}
            Ok(Some(why)) => {
                tally.failed += 1;
                diags.push(Diagnostic::error(
                    code,
                    file,
                    clause.span,
                    format!("`{written}` does not hold — {why}"),
                ));
            }
            Err(why) => {
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

fn facet_name(f: &Facet) -> String {
    match f {
        Facet::Status => "status".to_string(),
        Facet::Header(n) => format!("header \"{n}\""),
        Facet::Body => "body".to_string(),
        Facet::BodyPath(p) => format!("body.{}", p.join(".")),
    }
}
