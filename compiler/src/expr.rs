//! Expressions in a function body (RFC-0013 §3, §4): the tree, its canonical text, the
//! constant folding that finds faults at check time, and the typing rule of each operator.
//!
//! Kept apart from [`crate::program`] so that components and services can adopt
//! expressions later without taking a program's statements with them (RFC-0013 §18.1).
//! The parser for this tree is [`crate::parse`]'s, because it shares the token cursor.
//!
//! What is here is RFC-0013's Wave 0 subset, Wave 1's numbers (C1 and C5), C4's
//! additions (§7) and C9's (§12): `int`, `float`, `bool` and `text` values, the arithmetic
//! operators, comparison, `and` / `or` / `not`, calls, the numeric methods of §4.4,
//! interpolation, enum values and their columns, `when` and `match` used as values,
//! `result(T, E)` and prefix `try`. Collections and other methods are later waves'.

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use crate::diagnostic::Span;

/// Declare a fieldless enum and its `ALL`, every variant in declaration order, from one
/// list, so a variant cannot be added without being listed. The language harness
/// (RFC-0012 §1.2) walks `ALL` to register each type and operator.
macro_rules! listed_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident, )*
        }
    ) => {
        $(#[$meta])*
        pub enum $name {
            $( $(#[$vmeta])* $variant, )*
        }

        impl $name {
            /// Every variant, in declaration order. Generated with the enum, so it cannot
            /// miss one.
            pub const ALL: &'static [$name] = &[$($name::$variant),*];
        }
    };
}

/// `name` as a `&'static str`, so a [`Ty::Enum`] can carry its enum's name and stay `Copy`.
/// Each distinct name is stored once for the life of the process, so the memory this holds
/// is bounded by the number of distinct enum names `mz` has read, not by how often.
pub fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<BTreeSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(n) = names.get(name) {
        return n;
    }
    let stored: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(stored);
    stored
}

/// A type an expression in a program can have.
///
/// Not declared with `listed_enum!`, because [`Ty::Enum`] carries a name and
/// [`Ty::Result`] two types: [`Ty::ALL`] lists every other variant by hand, and [`Ty::listed`]'s exhaustive `match` stops a new
/// variant from compiling until it is listed there too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ty {
    /// A signed 64-bit integer (RFC-0013 §4.1).
    Int,
    /// An IEEE 754 binary64 number (RFC-0013 §4.2).
    Float,
    /// `true` or `false`.
    Bool,
    /// UTF-8 text.
    Text,
    /// The "value" of a call to a function that returns nothing.
    Nothing,
    /// An enum declared in the program, by name (RFC-0013 §7.2). The program's own type,
    /// not a built-in one: the language harness registers it as the `enum` feature.
    Enum(&'static str),
    /// A sub-expression that already failed. It is reported once and silent from then on
    /// (RFC-0013 §16: one diagnostic per true error).
    Error,
    /// `result(T, E)` (RFC-0013 §12.1): the success type, [`Ty::Nothing`] for
    /// `result(none, E)`, and the error type. Interned, as [`Ty::Enum`]'s name is.
    Result(&'static (Ty, Ty)),
}

impl Ty {
    /// Every built-in variant, in declaration order: all but [`Ty::Enum`], whose types the
    /// program declares.
    pub const ALL: &'static [Ty] = &[
        Ty::Int,
        Ty::Float,
        Ty::Bool,
        Ty::Text,
        Ty::Nothing,
        Ty::Error,
    ];

    /// Whether `self` is in [`Ty::ALL`]. The `match` has no wildcard, so a variant added
    /// later does not compile until it is listed here, and then in `ALL` (a unit test
    /// checks the two agree).
    pub fn listed(self) -> bool {
        match self {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Text | Ty::Nothing | Ty::Error => true,
            Ty::Enum(_) | Ty::Result(_) => false,
        }
    }

    /// Whether an author writes this type by a built-in name. `Nothing` and `Error` are the
    /// checker's own, an enum is the program's, and a `result(T, E)` is built from two
    /// others (the language harness registers it as the `result` feature); every other
    /// variant, including one added later, is a surface type, which the language harness
    /// must register (its tests fail otherwise).
    pub fn is_surface(self) -> bool {
        !matches!(self, Ty::Nothing | Ty::Error | Ty::Enum(_) | Ty::Result(_))
    }

    /// The surface types, in declaration order.
    pub fn surface() -> impl Iterator<Item = Ty> {
        Ty::ALL.iter().copied().filter(|t| t.is_surface())
    }

    /// The surface type a program writes as `name`, if any. The program parser reads type
    /// names through this, so the names it accepts are exactly the surface types'.
    pub fn from_name(name: &str) -> Option<Ty> {
        Ty::surface().find(|t| t.name() == name)
    }

    /// The type as Mzizi writes it.
    pub fn name(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Float => "float",
            Ty::Bool => "bool",
            Ty::Text => "text",
            Ty::Nothing => "nothing",
            Ty::Enum(name) => name,
            Ty::Error => "unknown",
            Ty::Result((ok, err)) => {
                let ok = if *ok == Ty::Nothing {
                    "none"
                } else {
                    ok.name()
                };
                intern(&format!("result({ok}, {})", err.name()))
            }
        }
    }

    /// `result(ok, err)`, interned.
    pub fn result(ok: Ty, err: Ty) -> Ty {
        Ty::Result(crate::intern::pair(ok, err))
    }

    /// The success and error types, when this is a `result`.
    pub fn as_result(self) -> Option<(Ty, Ty)> {
        match self {
            Ty::Result(&(ok, err)) => Some((ok, err)),
            _ => None,
        }
    }
}

listed_enum! {
    /// A prefix operator.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum UnOp {
        /// `-x`, on `int` (trapping on overflow: `-` of `int` minimum) or `float`.
        Neg,
        /// `not b`, on `bool`.
        Not,
        /// `try r`, on a `result`: its success value, or its error returned from the
        /// enclosing function at once (RFC-0013 §12.2). Level 3, with prefix `-`.
        Try,
    }
}

listed_enum! {
    /// A binary operator.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum BinOp {
        /// `+`
        Add,
        /// `-`
        Sub,
        /// `*`
        Mul,
        /// `/`, truncating toward zero.
        Div,
        /// `%`, with the sign of the dividend.
        Rem,
        /// `is`
        Is,
        /// `is not`
        IsNot,
        /// `<`
        Lt,
        /// `<=`
        Le,
        /// `>`
        Gt,
        /// `>=`
        Ge,
        /// `and`, short-circuit.
        And,
        /// `or`, short-circuit.
        Or,
    }
}

impl BinOp {
    /// The operator as Mzizi writes it.
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Is => "is",
            BinOp::IsNot => "is not",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }

    /// RFC-0013 §3.5's level: lower binds tighter.
    pub fn level(self) -> u8 {
        match self {
            BinOp::Mul | BinOp::Div | BinOp::Rem => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Is | BinOp::IsNot | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 6,
            BinOp::And => 8,
            BinOp::Or => 9,
        }
    }

    /// Whether this is one of the arithmetic operators, on `int` or `float`.
    pub fn is_arithmetic(self) -> bool {
        self.level() <= 5
    }

    /// Whether this is a comparison (level 6, which does not chain).
    pub fn is_comparison(self) -> bool {
        self.level() == 6
    }
}

/// One piece of a text literal.
#[derive(Clone, Debug, PartialEq)]
pub enum TextPart {
    /// Literal text, escapes decoded.
    Lit(String),
    /// `{expr}`, interpolated through the value's text form (RFC-0013 §3.8).
    Expr(Expr),
}

/// What an expression is.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// An `int` literal.
    Int(i64),
    /// A `float` literal (RFC-0013 §3.1): digits, a point and digits.
    Float(f64),
    /// `true` or `false`.
    Bool(bool),
    /// A text literal, with any interpolations.
    Text(Vec<TextPart>),
    /// A binding or parameter.
    Name(String),
    /// `f(a, b)`, a function of the program or `print`.
    Call {
        /// The function's name.
        name: String,
        /// Where the name is.
        name_span: Span,
        /// The arguments, in order.
        args: Vec<Expr>,
    },
    /// `recv.name(args)`, a method (RFC-0013 §3.7), or `recv.name` with no parentheses,
    /// which a program reads only to report it.
    Method {
        /// The value the method is called on.
        recv: Box<Expr>,
        /// The method's name.
        name: String,
        /// Where the name is.
        name_span: Span,
        /// The arguments, in order.
        args: Vec<Expr>,
        /// Whether the parentheses were written: `x.abs()` rather than `x.abs`.
        called: bool,
    },
    /// A prefix operator.
    Unary {
        /// Which.
        op: UnOp,
        /// Its operand.
        operand: Box<Expr>,
    },
    /// A binary operator.
    Binary {
        /// Which.
        op: BinOp,
        /// Where the operator is written.
        op_span: Span,
        /// The left operand.
        lhs: Box<Expr>,
        /// The right operand.
        rhs: Box<Expr>,
    },
    /// `<enum>.<variant>`, a variant named with its enum. A bare variant is a
    /// [`ExprKind::Name`] that the checker resolves (RFC-0008 §5).
    Variant {
        /// The enum's name.
        enum_name: String,
        /// The variant's name.
        name: String,
        /// Where the variant's name is.
        name_span: Span,
    },
    /// `when c` … `else when c2` … `else` … `end` used as a value (RFC-0013 §7.4): each
    /// branch is one line, an expression.
    When {
        /// `(condition, value)` for the `when` and each `else when`, in order.
        arms: Vec<(Expr, Expr)>,
        /// The `else` branch's value. `None` was reported as `MZ0932` by the parser.
        otherwise: Option<Box<Expr>>,
    },
    /// `match s` … `case v` … `else` … `end` used as a value (RFC-0013 §7.4).
    Match {
        /// The value matched.
        scrutinee: Box<Expr>,
        /// Each `case` and its one-line value.
        arms: Vec<Arm<Expr>>,
        /// The `else` and its value, if written.
        otherwise: Option<Box<ElseArm<Expr>>>,
    },
    /// `base.name` with no parentheses: an enum value's column (`problem.say`, RFC-0013
    /// §12.1). A variant named with its enum is [`ExprKind::Variant`]; records' fields are
    /// C8's (§11).
    Field {
        /// What the dot follows.
        base: Box<Expr>,
        /// The word after the dot.
        name: String,
        /// Where that word is.
        name_span: Span,
    },
    /// Something the parser could not read. Already reported.
    Error,
}

/// One `case` of a `match` (RFC-0013 §7.2): its values, and a body that is statements in a
/// `match` statement and one expression in a `match` used as a value.
#[derive(Clone, Debug, PartialEq)]
pub struct Arm<B> {
    /// The values the case lists, in order: literals or variants.
    pub values: Vec<Expr>,
    /// The `case` line, from `case` to its last value.
    pub span: Span,
    /// What runs, or what the case is worth.
    pub body: B,
    /// The case's last line, for a fix that deletes it whole.
    pub last_line: u32,
    /// Whether the case was written after the `else`, where it can never be reached.
    pub after_else: bool,
    /// The name a result's case binds (RFC-0013 §12.2): `v` in `case ok v` or
    /// `case error v`. Only a `match` on a result has one; the parser reads a second word
    /// as a binding only when it names no variant, since no binding may (`MZ0921`).
    pub binding: Option<(String, Span)>,
}

/// A `match`'s `else` (RFC-0013 §7.2).
#[derive(Clone, Debug, PartialEq)]
pub struct ElseArm<B> {
    /// The `else` line.
    pub span: Span,
    /// What runs, or what the `else` is worth.
    pub body: B,
    /// The `else` branch's last line, for a fix that deletes it whole.
    pub last_line: u32,
}

/// An expression with its source span.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// What it is.
    pub kind: ExprKind,
    /// Where it is, first character to last.
    pub span: Span,
}

impl Expr {
    /// RFC-0013 §3.5's level of the expression's outermost operator: 1 for a primary, 2
    /// for a method or a dotted read.
    pub fn level(&self) -> u8 {
        match &self.kind {
            ExprKind::Binary { op, .. } => op.level(),
            ExprKind::Unary {
                op: UnOp::Neg | UnOp::Try,
                ..
            } => 3,
            ExprKind::Unary { op: UnOp::Not, .. } => 7,
            ExprKind::Method { .. } | ExprKind::Field { .. } => 2,
            _ => 1,
        }
    }

    /// Whether the expression holds a sub-expression the parser could not read, whose
    /// canonical text would be a placeholder rather than the author's code.
    pub fn has_error(&self) -> bool {
        match &self.kind {
            ExprKind::Error => true,
            ExprKind::Text(parts) => parts.iter().any(|p| match p {
                TextPart::Expr(e) => e.has_error(),
                TextPart::Lit(_) => false,
            }),
            ExprKind::Call { args, .. } => args.iter().any(Expr::has_error),
            ExprKind::Method { recv, args, .. } => {
                recv.has_error() || args.iter().any(Expr::has_error)
            }
            ExprKind::Field { base, .. } => base.has_error(),
            ExprKind::Unary { operand, .. } => operand.has_error(),
            ExprKind::Binary { lhs, rhs, .. } => lhs.has_error() || rhs.has_error(),
            ExprKind::When { arms, otherwise } => {
                arms.iter().any(|(c, v)| c.has_error() || v.has_error())
                    || otherwise.as_ref().is_some_and(|o| o.has_error())
            }
            ExprKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                scrutinee.has_error()
                    || arms
                        .iter()
                        .any(|a| a.body.has_error() || a.values.iter().any(Expr::has_error))
                    || otherwise.as_ref().is_some_and(|o| o.body.has_error())
            }
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Bool(_)
            | ExprKind::Name(_)
            | ExprKind::Variant { .. } => false,
        }
    }

    /// Whether the parentheses that make `e` one operand were written: an operator whose
    /// span reaches past its operands' (`(a and b)`), or any other expression.
    pub fn is_parenthesised(&self) -> bool {
        match &self.kind {
            ExprKind::Binary { lhs, rhs, .. } => {
                (self.span.start_line, self.span.start_col)
                    != (lhs.span.start_line, lhs.span.start_col)
                    || (self.span.end_line, self.span.end_col)
                        != (rhs.span.end_line, rhs.span.end_col)
            }
            _ => true,
        }
    }

    /// Whether the expression holds a text literal anywhere. Interpolation may not
    /// (RFC-0013 §3.6), so a fix that moves an expression into `{…}` checks this first.
    pub fn has_text_literal(&self) -> bool {
        match &self.kind {
            ExprKind::Text(_) => true,
            ExprKind::Call { args, .. } => args.iter().any(Expr::has_text_literal),
            ExprKind::Method { recv, args, .. } => {
                recv.has_text_literal() || args.iter().any(Expr::has_text_literal)
            }
            ExprKind::Unary { operand, .. } => operand.has_text_literal(),
            ExprKind::Binary { lhs, rhs, .. } => lhs.has_text_literal() || rhs.has_text_literal(),
            ExprKind::When { .. } | ExprKind::Match { .. } => true,
            ExprKind::Field { base, .. } => base.has_text_literal(),
            _ => false,
        }
    }
}

/// The canonical text of an expression (RFC-0013 §17): one space around a binary operator,
/// none after unary `-`, `f(a, b)`, and parentheses only where precedence needs them. It is
/// what a trap quotes (§4.3), so it reads as the author would have written it.
pub fn canonical(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Float(v) => crate::numbers::mz_float_layout(*v, true),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Text(parts) => canonical_text(parts),
        ExprKind::Name(n) => n.clone(),
        // `range`'s second parameter is labelled `to` (RFC-0013 §6.5, §7.3).
        ExprKind::Call { name, args, .. } if name == "range" && args.len() == 2 => {
            format!(
                "range({}, to = {})",
                canonical(&args[0]),
                canonical(&args[1])
            )
        }
        ExprKind::Call { name, args, .. } => {
            let args: Vec<String> = args.iter().map(canonical).collect();
            format!("{name}({})", args.join(", "))
        }
        ExprKind::Method {
            recv,
            name,
            args,
            called,
            ..
        } => {
            let r = canonical(recv);
            let r = if recv.level() > 2 {
                format!("({r})")
            } else {
                r
            };
            if *called {
                let args: Vec<String> = args.iter().map(canonical).collect();
                format!("{r}.{name}({})", args.join(", "))
            } else {
                format!("{r}.{name}")
            }
        }
        ExprKind::Unary { op, operand } => {
            let inner = canonical(operand);
            match op {
                // `-(-x)`: written bare, `--x` would lex as the decrement idiom.
                UnOp::Neg
                    if operand.level() > 3 || matches!(operand.kind, ExprKind::Unary { .. }) =>
                {
                    format!("-({inner})")
                }
                UnOp::Neg => format!("-{inner}"),
                UnOp::Not if operand.level() > 7 => format!("not ({inner})"),
                UnOp::Not => format!("not {inner}"),
                UnOp::Try if operand.level() > 3 => format!("try ({inner})"),
                UnOp::Try => format!("try {inner}"),
            }
        }
        ExprKind::Field { base, name, .. } => {
            if base.level() > 2 {
                format!("({}).{name}", canonical(base))
            } else {
                format!("{}.{name}", canonical(base))
            }
        }
        ExprKind::Binary { op, lhs, rhs, .. } => {
            let level = op.level();
            let left = canonical(lhs);
            let right = canonical(rhs);
            // Left-associative: a left operand at the same level needs no parentheses, a
            // right one does. Comparisons do not associate, so both sides need them. Where
            // `and` meets `or`, the parentheses are always written (§3.4, §17).
            let meets = |o: &Expr| {
                *op == BinOp::Or && matches!(o.kind, ExprKind::Binary { op: BinOp::And, .. })
            };
            let left_parens =
                lhs.level() > level || (op.is_comparison() && lhs.level() == level) || meets(lhs);
            let right_parens = rhs.level() >= level || meets(rhs);
            let wrap = |s: String, p: bool| if p { format!("({s})") } else { s };
            format!(
                "{} {} {}",
                wrap(left, left_parens),
                op.text(),
                wrap(right, right_parens)
            )
        }
        ExprKind::Variant {
            enum_name, name, ..
        } => format!("{enum_name}.{name}"),
        // A block used as a value spans lines; a one-line quote names its first.
        ExprKind::When { arms, .. } => match arms.first() {
            Some((cond, _)) => format!("when {} …", canonical(cond)),
            None => "when …".to_string(),
        },
        ExprKind::Match { scrutinee, .. } => format!("match {} …", canonical(scrutinee)),
        ExprKind::Error => "…".to_string(),
    }
}

/// A text literal as canonical Mzizi: escapes re-applied, interpolations canonical.
pub fn canonical_text(parts: &[TextPart]) -> String {
    let mut out = String::from("\"");
    for part in parts {
        match part {
            TextPart::Lit(s) => {
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '{' => out.push_str("\\{"),
                        '}' => out.push_str("\\}"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        c => out.push(c),
                    }
                }
            }
            TextPart::Expr(e) => {
                out.push('{');
                out.push_str(&canonical(e));
                out.push('}');
            }
        }
    }
    out.push('"');
    out
}

/// A fault the checker can see in constants (RFC-0013 §4.1, `MZ0915`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    /// `x / 0` or `x % 0` with a literal zero.
    DivideByZero,
    /// A literal computation that does not fit in an `int`.
    Overflow,
    /// `x.pow(n)` on `int` with a literal `n` below zero.
    NegativeExponent,
}

/// The value of an integer expression built only from literals, or `None` when it reads a
/// name or calls a function. `Some(Err(_))` is a constant fault: the program would trap
/// every time this expression ran.
pub fn fold(e: &Expr) -> Option<Result<i64, Fault>> {
    match &e.kind {
        ExprKind::Int(v) => Some(Ok(*v)),
        ExprKind::Unary {
            op: UnOp::Neg,
            operand,
        } => Some(fold(operand)?.and_then(|v| v.checked_neg().ok_or(Fault::Overflow))),
        ExprKind::Binary { op, lhs, rhs, .. } if op.is_arithmetic() => {
            let (a, b) = (fold(lhs)?, fold(rhs)?);
            let (a, b) = match (a, b) {
                (Ok(a), Ok(b)) => (a, b),
                (Err(f), _) | (_, Err(f)) => return Some(Err(f)),
            };
            Some(int_op(*op, a, b))
        }
        ExprKind::Method {
            recv,
            name,
            args,
            called: true,
            ..
        } => {
            let x = fold(recv)?;
            let mut values = Vec::new();
            for a in args {
                values.push(fold(a)?);
            }
            let x = match x {
                Ok(x) => x,
                Err(f) => return Some(Err(f)),
            };
            let mut ints = Vec::new();
            for v in values {
                match v {
                    Ok(v) => ints.push(v),
                    Err(f) => return Some(Err(f)),
                }
            }
            crate::numbers::fold_int_method(name, x, &ints)
        }
        _ => None,
    }
}

/// One integer operation with RFC-0013 §4.1's semantics: overflow and division by zero are
/// faults, division truncates toward zero, and `%` takes the dividend's sign. The lowering
/// emits the same rule through `checked_*` (§14.2).
pub fn int_op(op: BinOp, a: i64, b: i64) -> Result<i64, Fault> {
    let r = match op {
        BinOp::Add => a.checked_add(b),
        BinOp::Sub => a.checked_sub(b),
        BinOp::Mul => a.checked_mul(b),
        BinOp::Div | BinOp::Rem if b == 0 => return Err(Fault::DivideByZero),
        BinOp::Div => a.checked_div(b),
        // `int` minimum `% -1` is 0, which fits: only a zero divisor is a fault.
        BinOp::Rem => Some(a.wrapping_rem(b)),
        _ => return Err(Fault::Overflow),
    };
    r.ok_or(Fault::Overflow)
}

/// Whether a value of this type has a text form (RFC-0013 §3.8), so it can be printed or
/// interpolated.
pub fn has_text_form(t: Ty) -> bool {
    matches!(
        t,
        Ty::Int | Ty::Float | Ty::Bool | Ty::Text | Ty::Enum(_) | Ty::Error
    )
}

/// The type a binary operator gives two operand types, or the reason it does not apply.
/// `Ty::Error` on either side is silent: that operand was already reported.
pub fn binary_type(op: BinOp, l: Ty, r: Ty) -> Result<Ty, String> {
    if l == Ty::Error || r == Ty::Error {
        return Ok(
            if op.is_comparison() || matches!(op, BinOp::And | BinOp::Or) {
                Ty::Bool
            } else {
                Ty::Error
            },
        );
    }
    match op {
        _ if op.is_arithmetic() => {
            if l == r && matches!(l, Ty::Int | Ty::Float) {
                Ok(l)
            } else {
                Err(format!(
                    "`{}` takes two ints or two floats, and this is {} {} {}",
                    op.text(),
                    l.name(),
                    op.text(),
                    r.name()
                ))
            }
        }
        BinOp::Is | BinOp::IsNot => {
            if l == r && l != Ty::Nothing {
                Ok(Ty::Bool)
            } else {
                Err(format!(
                    "`{}` compares two values of one type, and this is {} {} {}",
                    op.text(),
                    l.name(),
                    op.text(),
                    r.name()
                ))
            }
        }
        BinOp::And | BinOp::Or => {
            if l == Ty::Bool && r == Ty::Bool {
                Ok(Ty::Bool)
            } else {
                Err(format!(
                    "`{}` takes two bools, and this is {} {} {} — Mzizi has no truthiness",
                    op.text(),
                    l.name(),
                    op.text(),
                    r.name()
                ))
            }
        }
        _ => {
            // Ordering: int, float, text (by Unicode scalar value) and an enum (in
            // declaration order), with one type on both sides.
            if l == r && matches!(l, Ty::Int | Ty::Float | Ty::Text | Ty::Enum(_)) {
                Ok(Ty::Bool)
            } else {
                Err(format!(
                    "`{}` orders two ints, two floats, two texts or two values of one enum, and this is {} {} {}",
                    op.text(),
                    l.name(),
                    op.text(),
                    r.name()
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ty_all_lists_every_built_in_variant_and_no_enum() {
        assert!(Ty::ALL.iter().all(|t| t.listed()));
        assert!(!Ty::Enum("shape").listed());
        assert!(!Ty::Enum("shape").is_surface());
        assert_eq!(Ty::from_name("shape"), None);
    }

    fn at() -> Span {
        Span::single(1, 1, 1)
    }

    fn int(v: i64) -> Expr {
        Expr {
            kind: ExprKind::Int(v),
            span: at(),
        }
    }

    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr {
            kind: ExprKind::Binary {
                op,
                op_span: at(),
                lhs: Box::new(l),
                rhs: Box::new(r),
            },
            span: at(),
        }
    }

    #[test]
    fn integer_division_truncates_and_remainder_takes_the_dividends_sign() {
        assert_eq!(int_op(BinOp::Div, -7, 2), Ok(-3));
        assert_eq!(int_op(BinOp::Rem, -7, 2), Ok(-1));
        assert_eq!(int_op(BinOp::Div, 7, 0), Err(Fault::DivideByZero));
        assert_eq!(int_op(BinOp::Div, i64::MIN, -1), Err(Fault::Overflow));
        assert_eq!(int_op(BinOp::Rem, i64::MIN, -1), Ok(0));
        assert_eq!(int_op(BinOp::Add, i64::MAX, 1), Err(Fault::Overflow));
    }

    #[test]
    fn canonical_text_keeps_only_the_parentheses_precedence_needs() {
        // (1 + 2) * 3 needs them; 1 + (2 * 3) does not; 1 - (2 - 3) does.
        let e = bin(BinOp::Mul, bin(BinOp::Add, int(1), int(2)), int(3));
        assert_eq!(canonical(&e), "(1 + 2) * 3");
        let e = bin(BinOp::Add, int(1), bin(BinOp::Mul, int(2), int(3)));
        assert_eq!(canonical(&e), "1 + 2 * 3");
        let e = bin(BinOp::Sub, int(1), bin(BinOp::Sub, int(2), int(3)));
        assert_eq!(canonical(&e), "1 - (2 - 3)");
    }

    #[test]
    fn folding_finds_constant_faults() {
        assert_eq!(
            fold(&bin(BinOp::Div, int(1), int(0))),
            Some(Err(Fault::DivideByZero))
        );
        assert_eq!(
            fold(&bin(BinOp::Mul, int(i64::MAX), int(2))),
            Some(Err(Fault::Overflow))
        );
        assert_eq!(fold(&bin(BinOp::Add, int(2), int(3))), Some(Ok(5)));
    }
}
