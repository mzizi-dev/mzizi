//! Lowering a `program` to Rust (RFC-0013 §14), and building and running it (`mz run`, §13).
//!
//! The generated package has **no dependencies**: a `Cargo.toml` and a `src/main.rs` that
//! uses only `std`. So `cargo build --offline` always works, and running a program needs no
//! network, unlike a service.
//!
//! What the lowering keeps true (§14.1, §14.3):
//!
//! - every Mzizi value is an owned Rust value; a read of a `text` binding is a `.clone()`, so
//!   a callee never changes its caller's bindings;
//! - integer `+ - * / %`, unary `-`, `pow` and `abs`, and `to_int` on a float, go through
//!   helpers that **trap** — one line on standard error naming the `.mz` file, line, column
//!   and the expression, then exit status 101 — rather than wrap or panic. A trap is an
//!   explicit check, not `overflow-checks`, so debug and release builds stop at the same
//!   place. Float arithmetic is Rust's `f64`, IEEE 754 exactly, and never traps (§4.2);
//! - a float prints through RFC-0013 §3.8's text form, whose code is `numbers/float_text.rs`
//!   emitted verbatim, the same function the compiler's own tests run;
//! - `print` writes with `writeln!` on a locked standard output and exits 141 when the write
//!   fails (a closed pipe), so it never panics;
//! - the generated code holds no `unwrap`, `expect`, `panic!`, indexing or `unsafe`;
//! - `main` runs on a thread with a 64 MiB stack.
//!
//! Nothing here claims the lowered code is fast.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::expr::{BinOp, Expr, ExprKind, TextPart, Ty, UnOp, canonical, fold};
use crate::numbers::FLOAT_TEXT_RUNTIME;
use crate::program::{FnDecl, Program, Stmt, StmtKind};

/// The generated package, as `(relative path, contents)` pairs.
#[derive(Debug, PartialEq)]
pub struct Package {
    /// The program's name, for the package and binary name.
    pub name: String,
    /// `Cargo.toml` and `src/main.rs`.
    pub files: Vec<(String, String)>,
}

impl Package {
    /// The name of the binary `cargo build` produces.
    pub fn binary(&self) -> String {
        format!("mz-{}", self.name.replace('_', "-"))
    }
}

/// The runtime every program's `main.rs` starts with, verbatim.
const RUNTIME: &str = r#"// The runtime's helpers are emitted whole, a program may declare a `fn` it never calls
// or a `var` it never changes (MZ0924 is a warning), and `return` is how Mzizi returns.
// None of these is a defect, so an inherited `-D warnings` must not fail the build.
#![allow(unused, clippy::needless_return)]

use std::io::Write as _;

/// Where a trap happened, in the `.mz` source.
struct MzAt {
    file: &'static str,
    line: u32,
    col: u32,
    text: &'static str,
}

/// RFC-0013 §4.3: a trap writes one line and exits 101. It is not a panic.
fn mz_trap(at: &MzAt, what: &str) -> ! {
    let mut err = ::std::io::stderr().lock();
    let _ = writeln!(
        err,
        "mz: trap MZ0991 at {}:{}:{}: {} in `{}`",
        at.file, at.line, at.col, what, at.text
    );
    ::std::process::exit(101)
}

fn mz_add(a: i64, b: i64, at: &MzAt) -> i64 {
    match a.checked_add(b) {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

fn mz_sub(a: i64, b: i64, at: &MzAt) -> i64 {
    match a.checked_sub(b) {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

fn mz_mul(a: i64, b: i64, at: &MzAt) -> i64 {
    match a.checked_mul(b) {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

/// Truncates toward zero, as RFC-0013 §4.1 specifies.
fn mz_div(a: i64, b: i64, at: &MzAt) -> i64 {
    if b == 0 {
        mz_trap(at, "integer division by zero")
    }
    match a.checked_div(b) {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

/// Takes the sign of the dividend, as RFC-0013 §4.1 specifies. Only a zero divisor traps:
/// `int` minimum `% -1` is 0, which fits, so it is `wrapping_rem`'s exact answer.
fn mz_rem(a: i64, b: i64, at: &MzAt) -> i64 {
    if b == 0 {
        mz_trap(at, "integer division by zero")
    }
    a.wrapping_rem(b)
}

fn mz_neg(a: i64, at: &MzAt) -> i64 {
    match a.checked_neg() {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

fn mz_abs(a: i64, at: &MzAt) -> i64 {
    match a.checked_abs() {
        Some(v) => v,
        None => mz_trap(at, "integer overflow"),
    }
}

/// `x.pow(n)` on `int` (RFC-0013 §4.4, §14.2): `checked_pow` takes a `u32`, so a negative
/// `n` traps, and above `u32::MAX` only `0`, `1` and `-1` have an answer that fits.
fn mz_pow(x: i64, n: i64, at: &MzAt) -> i64 {
    if n < 0 {
        mz_trap(at, "negative exponent")
    }
    match u32::try_from(n) {
        Ok(n) => match x.checked_pow(n) {
            Some(v) => v,
            None => mz_trap(at, "integer overflow"),
        },
        Err(_) => match x {
            0 | 1 => x,
            -1 => {
                if n % 2 == 0 {
                    1
                } else {
                    -1
                }
            }
            _ => mz_trap(at, "integer overflow"),
        },
    }
}

/// `f.to_int()` (RFC-0013 §4.4): truncates toward zero; `nan`, the infinities and any value
/// outside `int` trap.
fn mz_to_int(x: f64, at: &MzAt) -> i64 {
    let t = x.trunc();
    if t.is_nan() || t < -9223372036854775808.0 || t >= 9223372036854775808.0 {
        mz_trap(at, "float to int out of range")
    }
    t as i64
}

/// A value's text form (RFC-0013 §3.8).
trait MzText {
    fn mz_text(&self) -> String;
}

impl MzText for i64 {
    fn mz_text(&self) -> String {
        self.to_string()
    }
}

impl MzText for f64 {
    fn mz_text(&self) -> String {
        mz_float_text(*self)
    }
}

impl MzText for bool {
    fn mz_text(&self) -> String {
        self.to_string()
    }
}

impl MzText for String {
    fn mz_text(&self) -> String {
        self.clone()
    }
}

/// `print`: one line on standard output. A closed pipe exits 141, the shell's status for
/// it, and never panics as the `println` macro would.
fn mz_print<T: MzText>(v: &T) {
    let mut out = ::std::io::stdout().lock();
    if writeln!(out, "{}", v.mz_text()).is_err() {
        ::std::process::exit(141);
    }
}

/// `main` runs on a thread with a 64 MiB stack, so deep recursion has room (§4.3).
fn main() {
    match ::std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(mz_main)
    {
        Ok(running) => {
            if running.join().is_err() {
                ::std::process::exit(101);
            }
        }
        Err(_) => mz_main(),
    }
}
"#;

/// Lower a checked program. Call only on a program with no errors: the lowering never
/// guesses at a tree the checker rejected.
pub fn lower(p: &Program, source: &str) -> Package {
    let mut fns = BTreeMap::new();
    for f in &p.fns {
        fns.insert(f.name.as_str(), f);
    }
    let mut lw = Lower {
        fns: &fns,
        types: BTreeMap::new(),
        sites: Vec::new(),
    };
    let mut body = String::new();
    for f in &p.fns {
        lw.function(f, &mut body);
    }
    let mut main = String::new();
    let _ = writeln!(
        main,
        "// Generated by `mz` from `{source}` (RFC-0013 §14). Do not edit; regenerate it with\n// `mz build {source} --out <dir>`.\n"
    );
    main.push_str(RUNTIME);
    main.push('\n');
    main.push_str(FLOAT_TEXT_RUNTIME);
    if !lw.sites.is_empty() {
        main.push('\n');
    }
    for (i, (line, col, text)) in lw.sites.iter().enumerate() {
        let _ = writeln!(
            main,
            "const MZ_AT_{n}: MzAt = MzAt {{ file: {file:?}, line: {line}, col: {col}, text: {text:?} }};",
            n = i + 1,
            file = source,
        );
    }
    main.push_str(&body);
    let cargo = format!(
        r#"# Generated by `mz` from `{source}`. Do not edit (RFC-0013 §14).
[package]
name = "mz-{name}"
version = "0.0.0"
edition = "2024"
publish = false

# Its own workspace root, so it never joins a surrounding workspace.
[workspace]

# None, by design: a program's package builds offline (RFC-0013 §13.1).
[dependencies]

# Traps are explicit checks, so both profiles behave alike; this is a second net.
[profile.release]
overflow-checks = true
"#,
        name = p.name.replace('_', "-"),
    );
    Package {
        name: p.name.clone(),
        files: vec![
            ("Cargo.toml".to_string(), cargo),
            ("src/main.rs".to_string(), main),
        ],
    }
}

/// Rust's keywords (strict, reserved and weak, edition 2024), which a Mzizi name is
/// emitted around as a raw identifier (RFC-0013 §14.2).
const RUST_KEYWORDS: &[&str] = &[
    "as",
    "break",
    "const",
    "continue",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "static",
    "struct",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "async",
    "await",
    "dyn",
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "macro",
    "override",
    "priv",
    "typeof",
    "unsized",
    "virtual",
    "yield",
    "try",
    "gen",
    "macro_rules",
    "union",
    "raw",
    "safe",
];

/// A Mzizi name as a Rust identifier.
fn ident(name: &str) -> String {
    match name {
        "crate" | "self" | "super" | "Self" => format!("mz_kw_{name}"),
        n if RUST_KEYWORDS.contains(&n) => format!("r#{n}"),
        n => n.to_string(),
    }
}

/// A Mzizi function's Rust name: `main` is the generated `mz_main`.
fn fn_ident(name: &str) -> String {
    if name == "main" {
        "mz_main".to_string()
    } else {
        ident(name)
    }
}

fn rust_type(t: Ty) -> &'static str {
    match t {
        Ty::Int => "i64",
        Ty::Float => "f64",
        Ty::Bool => "bool",
        Ty::Text => "String",
        Ty::Nothing | Ty::Error => "()",
    }
}

struct Lower<'a> {
    fns: &'a BTreeMap<&'a str, &'a FnDecl>,
    /// The type of every binding in the function being lowered. A name is bound at most
    /// once in a function (RFC-0013 §5.3), so one map per function is exact.
    types: BTreeMap<String, Ty>,
    /// Trap sites: line, column, and the expression's canonical text.
    sites: Vec<(u32, u32, String)>,
}

impl Lower<'_> {
    fn function(&mut self, f: &FnDecl, out: &mut String) {
        self.types.clear();
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| {
                self.types.insert(p.name.clone(), p.ty.ty);
                format!("{}: {}", ident(&p.name), rust_type(p.ty.ty))
            })
            .collect();
        let ret = f
            .ret
            .map(|r| format!(" -> {}", rust_type(r.ty)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "\n// {}\nfn {}({}){ret} {{",
            f.signature(),
            fn_ident(&f.name),
            params.join(", ")
        );
        self.block(&f.body, 1, out);
        out.push_str("}\n");
    }

    fn block(&mut self, stmts: &[Stmt], depth: usize, out: &mut String) {
        for s in stmts {
            self.stmt(s, depth, out);
        }
    }

    fn stmt(&mut self, s: &Stmt, depth: usize, out: &mut String) {
        let pad = "    ".repeat(depth);
        match &s.kind {
            StmtKind::Bind {
                mutable,
                name,
                ty,
                value,
                ..
            } => {
                let t = ty.map_or_else(|| self.ty(value), |t| t.ty);
                self.types.insert(name.clone(), t);
                let _ = writeln!(
                    out,
                    "{pad}let {}{}: {} = {};",
                    if *mutable { "mut " } else { "" },
                    ident(name),
                    rust_type(t),
                    self.expr(value)
                );
            }
            StmtKind::Assign { name, value, .. } => {
                let _ = writeln!(out, "{pad}{} = {};", ident(name), self.expr(value));
            }
            StmtKind::Return(None) => {
                let _ = writeln!(out, "{pad}return;");
            }
            StmtKind::Return(Some(v)) => {
                let _ = writeln!(out, "{pad}return {};", self.expr(v));
            }
            StmtKind::When {
                cond,
                then,
                otherwise,
            } => {
                let _ = writeln!(out, "{pad}if {} {{", self.expr(cond));
                self.block(then, depth + 1, out);
                if let Some(o) = otherwise {
                    let _ = writeln!(out, "{pad}}} else {{");
                    self.block(o, depth + 1, out);
                }
                let _ = writeln!(out, "{pad}}}");
            }
            StmtKind::Expr(e) => {
                let _ = writeln!(out, "{pad}{};", self.expr(e));
            }
        }
    }

    /// An expression's type. The program was checked, so every name is bound.
    fn ty(&self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Text(_) => Ty::Text,
            ExprKind::Name(n) => self.types.get(n).copied().unwrap_or(Ty::Error),
            ExprKind::Call { name, .. } => self
                .fns
                .get(name.as_str())
                .map_or(Ty::Nothing, |f| f.ret.map_or(Ty::Nothing, |r| r.ty)),
            ExprKind::Method { recv, name, .. } => {
                crate::numbers::method(self.ty(recv), name).map_or(Ty::Error, |(_, ret)| ret)
            }
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => self.ty(operand),
            ExprKind::Unary { op: UnOp::Not, .. } => Ty::Bool,
            ExprKind::Binary { op, lhs, .. } if op.is_arithmetic() => self.ty(lhs),
            ExprKind::Binary { .. } => Ty::Bool,
            ExprKind::Error => Ty::Error,
        }
    }

    /// A trap site for `e`, and its constant's name.
    fn site(&mut self, e: &Expr) -> String {
        self.sites
            .push((e.span.start_line, e.span.start_col, canonical(e)));
        format!("&MZ_AT_{}", self.sites.len())
    }

    /// An expression as Rust, safe to use as an operand of a Rust binary operator.
    fn atom(&mut self, e: &Expr) -> String {
        let s = self.expr(e);
        match &e.kind {
            ExprKind::Binary { op, .. } if !op.is_arithmetic() => format!("({s})"),
            _ => s,
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        if let Some(Ok(v)) = fold(e) {
            return format!("{v}i64");
        }
        match &e.kind {
            ExprKind::Int(v) => format!("{v}i64"),
            ExprKind::Float(v) => format!("{v:?}f64"),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Text(parts) => self.text(parts),
            ExprKind::Name(n) => {
                if self.types.get(n) == Some(&Ty::Text) {
                    format!("{}.clone()", ident(n))
                } else {
                    ident(n)
                }
            }
            ExprKind::Call { name, args, .. } => {
                if name == "print" {
                    let arg = args.first().map(|a| self.atom(a)).unwrap_or_default();
                    return format!("mz_print(&{arg})");
                }
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                format!("{}({})", fn_ident(name), args.join(", "))
            }
            ExprKind::Method {
                recv, name, args, ..
            } => self.method(e, recv, name, args),
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } if self.ty(operand) == Ty::Float => format!("(-{})", self.atom(operand)),
            ExprKind::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                let inner = self.expr(operand);
                let at = self.site(e);
                format!("mz_neg({inner}, {at})")
            }
            ExprKind::Unary {
                op: UnOp::Not,
                operand,
            } => format!("!{}", self.atom(operand)),
            ExprKind::Binary { op, lhs, rhs, .. }
                if op.is_arithmetic() && self.ty(lhs) == Ty::Float =>
            {
                // IEEE 754 exactly (RFC-0013 §4.2): no trap, `1.0 / 0.0` is `inf`.
                let l = self.atom(lhs);
                let r = self.atom(rhs);
                format!("({l} {} {r})", op.text())
            }
            ExprKind::Binary { op, lhs, rhs, .. } => {
                if op.is_arithmetic() {
                    let helper = match op {
                        BinOp::Add => "mz_add",
                        BinOp::Sub => "mz_sub",
                        BinOp::Mul => "mz_mul",
                        BinOp::Div => "mz_div",
                        _ => "mz_rem",
                    };
                    let l = self.expr(lhs);
                    let r = self.expr(rhs);
                    let at = self.site(e);
                    return format!("{helper}({l}, {r}, {at})");
                }
                let rust_op = match op {
                    BinOp::Is => "==",
                    BinOp::IsNot => "!=",
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    BinOp::Ge => ">=",
                    BinOp::And => "&&",
                    _ => "||",
                };
                format!("{} {rust_op} {}", self.atom(lhs), self.atom(rhs))
            }
            ExprKind::Error => "()".to_string(),
        }
    }

    /// A numeric method (RFC-0013 §4.4, §14.2). The ones that can trap go through a helper
    /// with the expression's site.
    fn method(&mut self, e: &Expr, recv: &Expr, name: &str, args: &[Expr]) -> String {
        let rt = self.ty(recv);
        let r = self.atom(recv);
        // A receiver that folded to a negative constant is `-4i64`, and Rust reads
        // `-4i64.min(n)` as `-(4i64.min(n))`: the parentheses keep Mzizi's `(-4).min(n)`.
        let r = if r.starts_with('-') {
            format!("({r})")
        } else {
            r
        };
        let args: Vec<String> = args.iter().map(|a| self.atom(a)).collect();
        let arg = args.first().cloned().unwrap_or_default();
        match (rt, name) {
            (Ty::Int, "to_float") => format!("({r} as f64)"),
            (Ty::Float, "to_int") => {
                let at = self.site(e);
                format!("mz_to_int({r}, {at})")
            }
            (Ty::Int, "abs") => {
                let at = self.site(e);
                format!("mz_abs({r}, {at})")
            }
            (Ty::Int, "pow") => {
                let at = self.site(e);
                format!("mz_pow({r}, {arg}, {at})")
            }
            // `as` from `i64` to `f64` rounds to nearest, which is `to_float` (§4.4).
            (Ty::Float, "pow") => format!("{r}.powf({arg} as f64)"),
            _ => format!("{r}.{name}({})", args.join(", ")),
        }
    }

    /// A text literal: `String::from` when it has no interpolation, else `format!` with
    /// each value through its text form.
    fn text(&mut self, parts: &[TextPart]) -> String {
        let mut fmt = String::new();
        let mut args = Vec::new();
        for part in parts {
            match part {
                TextPart::Lit(s) => fmt.push_str(&s.replace('{', "{{").replace('}', "}}")),
                TextPart::Expr(e) => {
                    fmt.push_str("{}");
                    let v = self.atom(e);
                    args.push(format!("MzText::mz_text(&{v})"));
                }
            }
        }
        if args.is_empty() {
            let lit: String = parts
                .iter()
                .map(|p| match p {
                    TextPart::Lit(s) => s.as_str(),
                    TextPart::Expr(_) => "",
                })
                .collect();
            return format!("String::from({lit:?})");
        }
        format!("format!({fmt:?}, {})", args.join(", "))
    }
}

/// Why `mz run` could not run a program.
#[derive(Debug)]
pub enum RunError {
    /// `cargo` could not be started, or failed for a reason that is not the lowered code
    /// (an old toolchain, a full disk): a usage or environment problem, exit 2.
    Cargo(String),
    /// The package could not be written: an I/O problem, exit 2.
    Io(String),
    /// `rustc` rejected the lowered code: a compiler bug by construction (`MZ0990`, exit 3).
    /// Holds `rustc`'s first error message.
    Build(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Cargo(s) | RunError::Io(s) | RunError::Build(s) => f.write_str(s),
        }
    }
}

/// Where `mz run` keeps a program's package: `<name>-<path hash>` under `$MZ_CACHE_DIR`, or
/// else `mzizi/mz-run/` under the user's cache directory (`$XDG_CACHE_HOME`, else
/// `$HOME/.cache`). Per user, so no other account can plant a file in a package `cargo`
/// builds; keyed by the source file's path, so each edit rebuilds in place and Cargo's own
/// fingerprint decides what an unchanged program skips. There is no fallback to the system's
/// temporary directory: a fixed name there is one another account can create first, so with
/// none of the three variables set this is an error that names them.
pub fn cache_dir(pkg: &Package, source: &Path) -> Result<PathBuf, RunError> {
    let env_dir = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = env_dir("MZ_CACHE_DIR")
        .or_else(|| env_dir("XDG_CACHE_HOME").map(|c| c.join("mzizi/mz-run")))
        .or_else(|| env_dir("HOME").map(|h| h.join(".cache/mzizi/mz-run")))
        .ok_or_else(|| {
            RunError::Io(
                "no per-user cache directory: set MZ_CACHE_DIR, XDG_CACHE_HOME or HOME".to_string(),
            )
        })?;
    let absolute = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let key = crate::hash::sha256(absolute.to_string_lossy().as_bytes()).short();
    Ok(base.join(format!("{}-{key}", pkg.name)))
}

/// Write the package's files under `dir`, leaving a file alone when it already holds the
/// same text, so `cargo` sees nothing new and rebuilds nothing.
pub fn write(pkg: &Package, dir: &Path) -> Result<(), RunError> {
    for (rel, text) in &pkg.files {
        let path = dir.join(rel);
        if std::fs::read_to_string(&path).is_ok_and(|old| old == *text) {
            continue;
        }
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, text))
            .map_err(|e| RunError::Io(format!("cannot write {}: {e}", path.display())))?;
    }
    Ok(())
}

/// `cargo build --offline --quiet` in `dir`, into `dir/target`. Returns the binary's path.
pub fn build(pkg: &Package, dir: &Path, release: bool) -> Result<PathBuf, RunError> {
    let target = dir.join("target");
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--offline")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &target);
    if release {
        cmd.arg("--release");
    }
    let output = cmd
        .output()
        .map_err(|e| RunError::Cargo(format!("cannot run `cargo`: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Only `rustc` rejecting `src/main.rs` is the lowering's fault. Anything else (a
        // toolchain without edition 2024, a full disk) is the environment's.
        let rejected = stderr.contains("error[E") || stderr.contains("src/main.rs:");
        let first = stderr
            .lines()
            .skip_while(|l| !l.starts_with("error"))
            .take_while(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let first = if first.is_empty() {
            stderr.trim().to_string()
        } else {
            first
        };
        return Err(if rejected {
            RunError::Build(first)
        } else {
            RunError::Cargo(format!("`cargo build` failed: {first}"))
        });
    }
    let profile = if release { "release" } else { "debug" };
    let exe = format!("{}{}", pkg.binary(), std::env::consts::EXE_SUFFIX);
    Ok(target.join(profile).join(exe))
}
