//! The language harness (RFC-0012): every feature of the language, registered once.
//!
//! RFC-0012 §1 calls the language harness the spine of the language. Each feature (a type,
//! an operator, a method, a statement form, a declaration kind, a diagnostic code, a
//! command) registers one [`HarnessEntry`] here, and what an agent reads is generated from
//! those entries: `mz harness definition` and `mz harness entry <name>` today, and in time
//! the agent skills and the benchmark guides (RFC-0012 §7).
//!
//! **What is a single source, and what is a parallel copy checked by tests.** The checker
//! does not read its rules from this registry. Some facts have one source, and the rest are
//! written twice, here and in the checker, and held together by `compiler/tests/harness.rs`:
//!
//! - **Single source, read from the checker:** each operator's spelling and precedence,
//!   which operators apply to which types (asked of [`binary_type`], the checker's own
//!   typing rule), the list of operators and types (`ALL`, generated with each enum in
//!   [`crate::expr`]), and each numeric method's signature ([`crate::numbers::method`]).
//! - **Single source, read by the compiler:** `mz` dispatches on [`COMMANDS`], so a command
//!   with no entry cannot run, and the program parser accepts exactly the surface types'
//!   names (`Ty::from_name`).
//! - **Parallel copies, checked by tests:** each code's severity, `say` text and fix kinds,
//!   and each feature's grammar, teaching text and examples. The checker emits its own
//!   diagnostics; the tests fail when the two disagree: a code the source can emit with no
//!   entry, a trigger that does not report its code at its severity with a declared fix
//!   kind, an example that does not check or run as stated. In a debug build (every
//!   `cargo test`), [`debug_assert_registered`] also checks every report against
//!   [`CODES`] and [`PENDING_CODES`], including each diagnostic's fix kind.
//! - **Exhaustive matches** make a new statement, expression, operator or type fail to
//!   compile until it names its entry ([`statement_entry`], [`expression_entry`],
//!   `binop_teach`, `unop_entry`, `type_teach`).
//!
//! **Registering a feature** (RFC-0012 §1.2, the registration rule): add one entry to
//! [`FEATURES`] (or to [`COMMANDS`] for a command), and one [`Code`] to [`CODES`] for each
//! diagnostic code it emits, with a `trigger` that emits it. Each entry's examples are
//! checked, and run where they give an expected output, by `compiler/tests/harness.rs`.
//!
//! This module does not change what the language accepts. The plugin host (RFC-0012 §4) is
//! not built; when it is, a third-party plugin may add entries of its own, and never change
//! these (§4.1).

use std::fmt::Write as _;

use crate::diagnostic::{CheckReport, Confidence, Severity, json_string};
use crate::expr::{BinOp, Ty, UnOp, binary_type, has_text_form};

/// The agent protocol's version (RFC-0012 §3). It changes when the shape of anything a
/// client reads changes: the NDJSON diagnostics, the summary line, or this definition's
/// JSON. Adding an entry is not a protocol change; the definition's hash (see [`version`])
/// tells a client the content changed.
pub const PROTOCOL: u32 = 1;

/// The language's version, as the harness reports it. The crates stay at `0.0.0` and
/// releases are git tags (CLAUDE.md), so the language names its phase and the RFC-0013 waves
/// that are built. The definition's SHA-256 is what pins exact content.
pub const LANGUAGE: &str = "phase-0, RFC-0013 wave 0 and wave 1 numbers";

/// What a [`HarnessEntry`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A top-level declaration: `program`, `component`, `service`.
    Declaration,
    /// A type an author writes.
    Type,
    /// An operator in an expression.
    Operator,
    /// A statement form in a function body.
    Statement,
    /// A built-in function.
    Function,
    /// A method on a value: the numeric methods of RFC-0013 §4.4.
    Method,
    /// Something the lexer reads: comments, text literals.
    Lexical,
    /// An `mz` command.
    Command,
    /// A diagnostic code.
    Diagnostic,
}

impl Kind {
    /// The kind as the definition writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Declaration => "declaration",
            Kind::Type => "type",
            Kind::Operator => "operator",
            Kind::Statement => "statement",
            Kind::Function => "function",
            Kind::Method => "method",
            Kind::Lexical => "lexical",
            Kind::Command => "command",
            Kind::Diagnostic => "diagnostic",
        }
    }
}

/// How much of a feature its entry covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// The whole feature as built: grammar, teaching, types, codes and tested examples.
    Full,
    /// The feature is named and summarised only. Its grammar and codes are in its RFCs; the
    /// entry is filled in by a later slice (component and service details, RFC-0012 §2).
    KindOnly,
}

impl Depth {
    fn as_str(self) -> &'static str {
        match self {
            Depth::Full => "full",
            Depth::KindOnly => "kind-only",
        }
    }
}

/// A runnable example: source that `mz check` accepts with no diagnostic, and, for a
/// `program`, what `mz run` prints. Both are tested by `compiler/tests/harness.rs`.
#[derive(Clone, Copy, Debug)]
pub struct Example {
    /// A whole `.mz` file.
    pub source: &'static str,
    /// Standard output of `mz run`, for a program; `None` when the example is checked only.
    pub output: Option<&'static str>,
}

/// A fix kind a diagnostic can carry, or none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixKind {
    /// Some occurrences carry no fix.
    None,
    /// Some carry an `exact` fix, which `mz fix` applies.
    Exact,
    /// Some carry a `guess`, which a reader should look at.
    Guess,
}

impl FixKind {
    /// As the protocol writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            FixKind::None => "none",
            FixKind::Exact => "exact",
            FixKind::Guess => "guess",
        }
    }

    /// The kind of an optional fix's confidence.
    pub fn of(c: Option<Confidence>) -> FixKind {
        match c {
            None => FixKind::None,
            Some(Confidence::Exact) => FixKind::Exact,
            Some(Confidence::Guess) => FixKind::Guess,
        }
    }
}

/// A registered diagnostic code.
#[derive(Clone, Copy, Debug)]
pub struct Code {
    /// `MZ` and four digits.
    pub code: &'static str,
    /// Its severity, which every occurrence has.
    pub severity: Severity,
    /// The command that reports it: `mz check` (and every command that checks first) or
    /// `mz run`.
    pub tool: &'static str,
    /// What it means, for an agent with no file open.
    pub say: &'static str,
    /// The fix kinds its occurrences carry.
    pub fixes: &'static [FixKind],
    /// The declaration kinds it is reported in.
    pub kinds: &'static [&'static str],
    /// The RFC section that defines it.
    pub rfc: &'static str,
    /// A file that `mz check` reports this code for, tested. `None` only for a code
    /// `mz check` cannot report (`MZ0990`, `MZ0991`), which the test names.
    pub trigger: Option<&'static str>,
}

/// A feature's registration: what it is called, how it is written, what an agent is told,
/// how it types, which codes it reports and examples that run (RFC-0012 §1.1).
#[derive(Clone, Debug)]
pub struct HarnessEntry {
    /// Unique across the registry: `int`, `let`, `is not`, `mz run`, `MZ0905`.
    pub name: &'static str,
    /// What it is.
    pub kind: Kind,
    /// How much of it the entry covers.
    pub depth: Depth,
    /// The RFC sections that design it.
    pub rfc: &'static str,
    /// Its forms, as an author writes them.
    pub grammar: Vec<&'static str>,
    /// One paragraph for an agent.
    pub teach: String,
    /// Its type rules in brief; empty where none apply.
    pub types: String,
    /// RFC-0013 §3.5's precedence level for an operator: lower binds tighter.
    pub precedence: Option<u8>,
    /// The diagnostic codes it reports, each one a registered [`Code`].
    pub codes: Vec<&'static str>,
    /// Runnable examples.
    pub examples: Vec<Example>,
    /// For a diagnostic entry, the code itself.
    pub diagnostic: Option<Code>,
}

/// A feature entry as written in [`FEATURES`]: everything but what the registry derives.
pub struct Feature {
    /// See [`HarnessEntry::name`].
    pub name: &'static str,
    /// See [`HarnessEntry::kind`].
    pub kind: Kind,
    /// See [`HarnessEntry::depth`].
    pub depth: Depth,
    /// See [`HarnessEntry::rfc`].
    pub rfc: &'static str,
    /// See [`HarnessEntry::grammar`].
    pub grammar: &'static [&'static str],
    /// See [`HarnessEntry::teach`].
    pub teach: &'static str,
    /// See [`HarnessEntry::types`].
    pub types: &'static str,
    /// See [`HarnessEntry::codes`].
    pub codes: &'static [&'static str],
    /// See [`HarnessEntry::examples`].
    pub examples: &'static [Example],
}

/// An `mz` command. `mz` dispatches on this table.
pub struct Command {
    /// The entry's name: `mz check`.
    pub name: &'static str,
    /// The subcommand: `check`, `run`.
    pub word: &'static str,
    /// Its usage line.
    pub usage: &'static str,
    /// Whether it takes exactly one `.mz` file.
    pub takes_file: bool,
    /// What it does and how it exits.
    pub teach: &'static str,
    /// The RFC sections that design it.
    pub rfc: &'static str,
}

// ------------------------------------------------------------------- examples

const HELLO: &str = include_str!("../../examples/hello.mz");
const HELLO_OUT: &str = include_str!("../../examples/hello.expected");
const FIB: &str = include_str!("../../examples/fib.mz");
const FIB_OUT: &str = include_str!("../../examples/fib.expected");

const BINDINGS: &str = "program bindings\n\n  fn main\n    let width: int = 6\n    let height = 7\n    var area = 0\n    area = width * height\n    print(\"area {area}\")\n  end fn main\n\nend program bindings\n";
const WHEN: &str = "program sign\n\n  fn main\n    print(sign(-4))\n    print(sign(0))\n  end fn main\n\n  fn sign(n: int): text\n    when n < 0\n      return \"negative\"\n    else\n      when n is 0\n        return \"zero\"\n      end\n    end\n    return \"positive\"\n  end fn sign\n\nend program sign\n";
const ARITH: &str = "program arith\n\n  fn main\n    print(7 / 2)\n    print(-7 / 2)\n    print(-7 % 2)\n    print(1 + 2 * 3)\n    print((1 + 2) * 3)\n    print(10 - 4)\n  end fn main\n\nend program arith\n";
const ARITH_OUT: &str = "3\n-3\n-1\n7\n9\n6\n";
const LOGIC_OUT: &str = "true\ntrue\ntrue\nfalse\ntrue\n";
const LOGIC: &str = "program logic\n\n  fn main\n    let a = 3\n    print(a is 3 and not (a > 4))\n    print(a is not 3 or a <= 3)\n    print(\"apple\" < \"banana\")\n    print(true is false)\n    print(a >= 3)\n  end fn main\n\nend program logic\n";
const TEXT: &str = "program texts\n\n  fn main\n    let name = \"Mzizi\"\n    print(\"hello, {name}: {1 + 1} \\{braces\\} and \\\"quotes\\\"\")\n  end fn main\n\nend program texts\n";
const NUMBERS: &str = include_str!("../../examples/numbers.mz");
const NUMBERS_OUT: &str = include_str!("../../examples/numbers.expected");
const METHODS: &str = "program methods\n\n  fn main\n    let f = 2.5\n    print(f.round())\n    print(f.floor())\n    print(f.ceil())\n    print(f.to_int())\n    print(16.0.sqrt())\n    print(f.is_nan())\n    print((-3).abs())\n    print(3.max(5))\n    print(1.5.min(0.5))\n    print(2.pow(10))\n    print(4.to_float())\n  end fn main\n\nend program methods\n";
const METHODS_OUT: &str = "3.0\n2.0\n3.0\n2\n4.0\nfalse\n3\n5\n0.5\n1024\n4.0\n";
const COMPONENT: &str = include_str!("../../primitives/badge.mz");
const SERVICE: &str = "service t\n  route r\n    get \"/r\"\n    respond 200\n  end\n  contract\n    example get \"/r\" status is 200\n  end\nend service t\n";

const fn run(source: &'static str, output: &'static str) -> Example {
    Example {
        source,
        output: Some(output),
    }
}

const fn checked(source: &'static str) -> Example {
    Example {
        source,
        output: None,
    }
}

// ------------------------------------------------------------------- features

/// The language's features, other than operators and types (which [`registry`] derives from
/// [`crate::expr`]) and commands ([`COMMANDS`]). A feature that lands adds its entry here.
pub const FEATURES: &[Feature] = &[
    Feature {
        name: "program",
        kind: Kind::Declaration,
        depth: Depth::Full,
        rfc: "RFC-0013 §1, §13, §18.6",
        grammar: &["program <name>\n  fn main\n    …\n  end fn main\nend program <name>"],
        teach: "A program is a file that runs. Its first line is `program <name>` and its last is `end program <name>`, with snake_case names; it holds `fn`s and nothing else, so every statement lives inside a `fn`. Exactly one `fn main`, with no parameters and no return type, is the entry point, and a program's output is what it prints. One program per file. `mz run` checks it, lowers it to Rust and runs it; `mz build` writes the Rust package.",
        types: "",
        codes: &[
            "MZ0901", "MZ0902", "MZ0204", "MZ0205", "MZ0207", "MZ0208", "MZ0310", "MZ0411",
        ],
        examples: &[run(HELLO, HELLO_OUT)],
    },
    Feature {
        name: "component",
        kind: Kind::Declaration,
        depth: Depth::KindOnly,
        rfc: "RFC-0001, RFC-0006, RFC-0008, RFC-0010",
        grammar: &[
            "component <name>\n  prop …\n  view\n    …\n  end\n  contract\n    …\n  end\nend component <name>",
        ],
        teach: "A component is a UI unit: props, records and enums, a view, and a contract block that `mz contract` evaluates against the component's own declarations. A component checks, has an outline, IR and hash (`mz outline`, `mz ir`, `mz hash`), and does not lower to Rust. Its full harness entry, with its grammar and codes, is a later slice; its codes are on the pending list.",
        types: "",
        codes: &[],
        examples: &[checked(COMPONENT)],
    },
    Feature {
        name: "service",
        kind: Kind::Declaration,
        depth: Depth::KindOnly,
        rfc: "RFC-0011, RFC-0010",
        grammar: &[
            "service <name>\n  route <name>\n    get \"/path\"\n    respond 200\n  end\n  contract\n    …\n  end\nend service <name>",
        ],
        teach: "A service is an HTTP backend: records, routes with handlers, a fallback and a contract block. `mz contract` runs it in process against its contract, and `mz build` lowers it to a local Rust + axum package. Its full harness entry is a later slice; its codes are on the pending list.",
        types: "",
        codes: &[],
        examples: &[checked(SERVICE)],
    },
    Feature {
        name: "fn",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §6",
        grammar: &[
            "fn <name>\n  …\nend fn <name>",
            "fn <name>(<param>: <type>, …): <type>\n  …\nend fn <name>",
        ],
        teach: "A function is declared with `fn`, its parameters each written `name: type`, and its return type after `:`. A function with no parameters has no parentheses, and one that returns nothing has no return type. It closes with `end fn <name>`. Calls give every argument by position, in the signature's order: there are no defaults, no named arguments and no overloading. Functions may recurse and may be called before they are declared.",
        types: "Each argument has its parameter's type. A function with a return type returns a value of that type on every path; a call to one that returns nothing has no value.",
        codes: &[
            "MZ0903", "MZ0904", "MZ0905", "MZ0906", "MZ0909", "MZ0204", "MZ0206", "MZ0207",
            "MZ0208", "MZ0306", "MZ0701",
        ],
        examples: &[run(FIB, FIB_OUT)],
    },
    Feature {
        name: "let",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §5",
        grammar: &["let <name> = <value>", "let <name>: <type> = <value>"],
        teach: "`let` binds a name that never changes. Every binding has a value. A name is visible from the line after its binding to the end of its block, and is bound at most once in a function: there is no shadowing. `const`, `val`, `auto` and Go's `:=` are not Mzizi.",
        types: "The binding has its value's type; a written type must equal it. A call that returns nothing cannot be bound.",
        codes: &["MZ0920", "MZ0921", "MZ0925", "MZ0926", "MZ0707", "MZ0711"],
        examples: &[run(BINDINGS, "area 42\n")],
    },
    Feature {
        name: "var",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §5",
        grammar: &["var <name> = <value>", "var <name>: <type> = <value>"],
        teach: "`var` binds a name that can be assigned again. It follows `let`'s rules of scope, and a `var` that is never assigned is a warning whose exact fix makes it a `let`. `let mut` and `mut` are Rust's: write `var`.",
        types: "As `let`; every later assignment has the binding's type.",
        codes: &["MZ0924", "MZ0925", "MZ0926"],
        examples: &[run(BINDINGS, "area 42\n")],
    },
    Feature {
        name: "assignment",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §5.1",
        grammar: &["<name> = <value>"],
        teach: "`name = value` assigns a `var` already bound in scope. A `let` or a parameter cannot be assigned. A first assignment with no `let` or `var` is an error whose fix inserts one. `+=`, `-=`, `*=`, `/=`, `++` and `--` are not Mzizi: write `x = x + 1`.",
        types: "The value has the binding's type.",
        codes: &["MZ0918", "MZ0922", "MZ0923", "MZ0711"],
        examples: &[run(BINDINGS, "area 42\n")],
    },
    Feature {
        name: "when",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §7.1",
        grammar: &[
            "when <condition>\n  …\nend",
            "when <condition>\n  …\nelse\n  …\nend",
        ],
        teach: "`when` is Mzizi's conditional statement, in a function body: a `bool` condition, a block, an optional `else` block, and a bare `end`. There is no truthiness, no trailing `:` and no braces, and `if` is not a word of the language. `else when`, `match` and loops are designed (RFC-0013 §7) and not built: nest a `when` inside `else`.",
        types: "The condition is a `bool`.",
        codes: &["MZ0712", "MZ0407", "MZ0937", "MZ0917", "MZ0919"],
        examples: &[run(WHEN, "negative\nzero\n")],
    },
    Feature {
        name: "return",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §6.2",
        grammar: &["return <value>", "return"],
        teach: "`return` leaves the function, with a value when the function has a return type and without one when it has none. Every path through a function with a return type ends in `return`. A line after a `return` that every path takes can never run, and is an error whose exact fix deletes it.",
        types: "The value has the function's return type.",
        codes: &["MZ0906", "MZ0907", "MZ0908"],
        examples: &[run(WHEN, "negative\nzero\n")],
    },
    Feature {
        name: "expression statement",
        kind: Kind::Statement,
        depth: Depth::Full,
        rfc: "RFC-0013 §3, §16",
        grammar: &["<call>"],
        teach: "A line that is only an expression must be a call whose value, if any, is used: `print(x)` or a call to a function that returns nothing. A value nothing reads is an error: bind it, return it or print it.",
        types: "",
        codes: &["MZ0916", "MZ0917"],
        examples: &[run(HELLO, HELLO_OUT)],
    },
    Feature {
        name: "print",
        kind: Kind::Function,
        depth: Depth::Full,
        rfc: "RFC-0013 §1, §3.8",
        grammar: &["print(<value>)"],
        teach: "`print(value)` writes one value's text form and a newline to standard output. It takes exactly one value; to print several, interpolate them into one text: `print(\"{a} and {b}\")`. `console.log`, `println!`, `fmt.Println`, `puts` and `print x` are other languages', with exact fixes where one exists.",
        types: "The value is an `int`, a `float`, a `bool` or a `text`: every surface type has a text form (RFC-0013 §3.8). `print` returns nothing.",
        codes: &["MZ0980", "MZ0905"],
        examples: &[run(HELLO, HELLO_OUT)],
    },
    Feature {
        name: "text literal",
        kind: Kind::Lexical,
        depth: Depth::Full,
        rfc: "RFC-0013 §3.1, §3.6",
        grammar: &["\"…\"", "\"… {<expr>} …\""],
        teach: "Text is written in double quotes on one line. `{expr}` interpolates a value's text form, and interpolation is the one way to build text: `+` on text is an error with a fix to interpolation. An interpolation holds one value and no string literal. The escapes are `\\n`, `\\t`, `\\\"`, `\\\\`, `\\{` and `\\}`.",
        types: "A literal is `text`. An interpolated value is an `int`, a `float`, a `bool` or a `text`.",
        codes: &["MZ0102", "MZ0714", "MZ0917", "MZ0912"],
        examples: &[run(TEXT, "hello, Mzizi: 2 {braces} and \"quotes\"\n")],
    },
    Feature {
        name: "comment",
        kind: Kind::Lexical,
        depth: Depth::Full,
        rfc: "RFC-0001 §1, RFC-0013 §16",
        grammar: &["## <text>"],
        teach: "A comment is a line starting with `##`. `//` and `#` do not start a comment in a program: each is an error whose exact fix writes `##`.",
        types: "",
        codes: &["MZ0911"],
        examples: &[run(FIB, FIB_OUT)],
    },
    Feature {
        name: "names",
        kind: Kind::Lexical,
        depth: Depth::Full,
        rfc: "RFC-0001 §2, RFC-0013 §5.2, §5.3",
        grammar: &["<name>", "snake_case"],
        teach: "A name reads the binding or parameter it names, from the line after its binding to the end of its block. Every name is snake_case; a camelCase name is an error whose exact fix writes its snake_case form. A binding cannot reuse a function's name, a built-in name (`int`, `bool`, `text`, `list`, `option`, `print`, `range`), a word of the language, or any name starting with `mz_`.",
        types: "",
        codes: &["MZ0101", "MZ0104", "MZ0921", "MZ0707", "MZ0920"],
        examples: &[run(BINDINGS, "area 42\n")],
    },
];

/// The `mz` commands. `mz` dispatches on this table, so a command runs only once it is
/// registered here.
pub const COMMANDS: &[Command] = &[
    Command {
        name: "mz check",
        word: "check",
        usage: "mz check [--agent] <file.mz>",
        takes_file: true,
        teach: "Check one file: every diagnostic, in source order, one per true error. `--agent` prints RFC-0001 §4's NDJSON, one diagnostic per line and then a summary line. Exits 0 with no errors (warnings allowed), 1 with errors, 2 for a usage or I/O problem.",
        rfc: "RFC-0001 §4",
    },
    Command {
        name: "mz fix",
        word: "fix",
        usage: "mz fix [--agent] <file.mz>",
        takes_file: true,
        teach: "Apply every `exact` fix in one pass, write the file, and check it again; `guess` fixes are never applied. Exits as `mz check` does on the result.",
        rfc: "RFC-0001 §4.3",
    },
    Command {
        name: "mz contract",
        word: "contract",
        usage: "mz contract [--agent] <file.mz>",
        takes_file: true,
        teach: "Check the file, then evaluate its `contract` block: a component's against its own declarations, a service's by running it in process. A failed assertion exits 1, as an error does. A program's contract block is not built.",
        rfc: "RFC-0006, RFC-0010, RFC-0011 §7",
    },
    Command {
        name: "mz outline",
        word: "outline",
        usage: "mz outline <file.mz>",
        takes_file: true,
        teach: "Print a component's interface only, as valid Mzizi. Services and programs are not covered yet (exit 2).",
        rfc: "RFC-0003 §4",
    },
    Command {
        name: "mz hash",
        word: "hash",
        usage: "mz hash <file.mz>",
        takes_file: true,
        teach: "Print a component's root hash in the content-addressed IR and its stored node count. Services and programs are not covered yet (exit 2).",
        rfc: "RFC-0003",
    },
    Command {
        name: "mz ir",
        word: "ir",
        usage: "mz ir <file.mz>",
        takes_file: true,
        teach: "Print every node of a component's IR with its hash and structural path. Services and programs are not covered yet (exit 2).",
        rfc: "RFC-0003",
    },
    Command {
        name: "mz build",
        word: "build",
        usage: "mz build <file.mz> --out <dir>",
        takes_file: true,
        teach: "Check, then lower a service to a local Rust + axum package, or a program to a dependency-free Rust package, in `<dir>`. Components do not lower (exit 2). A file with errors is not lowered (exit 1).",
        rfc: "RFC-0011 §8, RFC-0013 §14",
    },
    Command {
        name: "mz run",
        word: "run",
        usage: "mz run [--release] <program.mz>",
        takes_file: true,
        teach: "Check a program, lower it, build it with `cargo build --offline` and run it, with `mz`'s own messages on standard error. It exits with the program's status: 0, 101 for a trap, 141 for a closed standard output; 2 for a usage problem and 3 when the program did not compile.",
        rfc: "RFC-0013 §13.1",
    },
    Command {
        name: "mz harness",
        word: "harness",
        usage: "mz harness version | definition [--agent] | entry <name> [--agent]",
        takes_file: false,
        teach: "Read the language harness. `version` prints the protocol version, the language version and the definition's SHA-256. `definition` prints every entry as JSON, indented, or on one line with `--agent`. `entry <name>` prints one entry. Exits 0, or 2 for a usage problem or an unknown name.",
        rfc: "RFC-0012 §5",
    },
];

// ------------------------------------------------------------------- codes

const PROGRAM: &[&str] = &["program"];
const ALL_KINDS: &[&str] = &["program", "component", "service"];
const NONE: &[FixKind] = &[FixKind::None];
const EXACT: &[FixKind] = &[FixKind::Exact];
const NONE_EXACT: &[FixKind] = &[FixKind::None, FixKind::Exact];
const NONE_GUESS: &[FixKind] = &[FixKind::None, FixKind::Guess];
const ALL_FIXES: &[FixKind] = &[FixKind::None, FixKind::Exact, FixKind::Guess];
const EXACT_GUESS: &[FixKind] = &[FixKind::Exact, FixKind::Guess];

const fn code(
    code: &'static str,
    rfc: &'static str,
    fixes: &'static [FixKind],
    kinds: &'static [&'static str],
    say: &'static str,
    trigger: &'static str,
) -> Code {
    Code {
        code,
        severity: Severity::Error,
        tool: "mz check",
        say,
        fixes,
        kinds,
        rfc,
        trigger: Some(trigger),
    }
}

/// Every registered diagnostic code, in code order. A code a feature emits is registered
/// here, with a trigger that emits it; component and service codes not yet registered are
/// on [`PENDING_CODES`].
pub const CODES: &[Code] = &[
    code(
        "MZ0101",
        "RFC-0001 §2, RFC-0008 §1",
        ALL_FIXES,
        ALL_KINDS,
        "a name that is not snake_case; the fix writes its snake_case form (a guess where the resolver picks the nearest name it could mean)",
        "program t\n  fn main\n    let myName = 1\n    print(myName)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0102",
        "RFC-0001 §2, §4",
        EXACT_GUESS,
        ALL_KINDS,
        "a string that is not closed on its line; strings cannot span lines, and the fix closes it at the line's end (a guess when it holds an escaped quote)",
        "program t\n  fn main\n    print(\"open)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0103",
        "RFC-0013 §3.1",
        NONE,
        ALL_KINDS,
        "an integer literal that does not fit in an `int`",
        "program t\n  fn main\n    print(99999999999999999999)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0104",
        "RFC-0001 §2, RFC-0013 §16",
        NONE,
        ALL_KINDS,
        "a character Mzizi does not use",
        "program t\n  fn main\n    print(1 @ 2)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0105",
        "RFC-0008 §1, §6",
        EXACT,
        ALL_KINDS,
        "a type spelt with symbols, such as `list<text>` or `text[]`; the exact fix writes `list(text)`",
        "component a\n  prop xs: list<text>\nend component a\n",
    ),
    code(
        "MZ0106",
        "RFC-0001 §4, RFC-0013 §16",
        EXACT,
        ALL_KINDS,
        "a spread (`...props`): Mzizi has none, and the exact fix deletes it",
        "component a\n  view\n    mark\n      ...rest\n    end\n  end\nend component a\n",
    ),
    code(
        "MZ0204",
        "RFC-0001 §1.1, §4",
        ALL_FIXES,
        ALL_KINDS,
        "a block that is never closed; the fix inserts its `end`",
        "program t\n  fn main\n    print(1)\nend program t\n",
    ),
    code(
        "MZ0205",
        "RFC-0001 §1.1, §4",
        EXACT,
        ALL_KINDS,
        "an `end` with nothing open to close; the exact fix deletes its line",
        "program t\n  fn main\n    print(1)\n  end fn main\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0206",
        "RFC-0001 §1.1, §4",
        EXACT,
        ALL_KINDS,
        "an `end` that closes the wrong kind of block, such as `end when` for a `when`, which closes with a bare `end`; the exact fix writes the right closer",
        "program t\n  fn main\n    when true\n      print(1)\n    end when\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0207",
        "RFC-0001 §1.1, RFC-0013 §16",
        EXACT,
        ALL_KINDS,
        "an `end` whose name does not match the block it closes; the exact fix writes the right name",
        "program t\n  fn main\n    print(1)\n  end fn mian\nend program t\n",
    ),
    code(
        "MZ0208",
        "RFC-0001 §1.1, RFC-0013 §16",
        EXACT,
        ALL_KINDS,
        "a bare `end` closing a named block (a `fn` or the program); the exact fix adds its name",
        "program t\n  fn main\n    print(1)\n  end\nend program t\n",
    ),
    code(
        "MZ0306",
        "RFC-0001 §4",
        NONE,
        ALL_KINDS,
        "a type expected and something else found",
        "program t\n  fn main\n  end fn main\n  fn f(n: 1): int\n    return 1\n  end fn f\nend program t\n",
    ),
    code(
        "MZ0310",
        "RFC-0008 §2",
        NONE,
        ALL_KINDS,
        "tokens left over after a declaration line: one declaration per line",
        "program t extra\n  fn main\n    print(1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0407",
        "RFC-0001 §1.2, §4",
        EXACT,
        ALL_KINDS,
        "`if`, which is not a word of the language; the exact fix writes `when`",
        "program t\n  fn main\n    if true\n      print(1)\n    end\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0411",
        "RFC-0001 §4.7, RFC-0013 §18.6",
        NONE,
        ALL_KINDS,
        "blocks or expressions nested past the cap (32 levels in a program); reported once per file",
        "program t\n  fn main\n    print(((((((((((((((((((((((((((((((((((1)))))))))))))))))))))))))))))))))))\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0701",
        "RFC-0008 §6",
        ALL_FIXES,
        ALL_KINDS,
        "an unknown type; the fix names the nearest one",
        "program t\n  fn main\n  end fn main\n  fn f(n: integer): int\n    return 1\n  end fn f\nend program t\n",
    ),
    code(
        "MZ0707",
        "RFC-0008 §5.2, §6",
        ALL_FIXES,
        ALL_KINDS,
        "a name bound nowhere in scope, or a function that does not exist; the fix names the nearest (`exact` or a guess, as the checker judges)",
        "program t\n  fn main\n    let count = 1\n    print(cuont)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0708",
        "RFC-0008 §6, RFC-0013 §16",
        ALL_FIXES,
        ALL_KINDS,
        "no such field, variant or method: in a program, a method a number does not have (`f.sqrt2()`); in a component or service, a field, variant or column; the fix names the nearest (`exact` or a guess, as the checker judges)",
        "program t\n  fn main\n    let f = 2.0\n    print(f.sqrt2())\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0711",
        "RFC-0008 §6",
        NONE,
        ALL_KINDS,
        "a value of the wrong kind: a binding's declared type, an assignment, a call to a binding, or an interpolation with no text form",
        "program t\n  fn main\n    let n: int = \"one\"\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0712",
        "RFC-0008 §4, §6",
        ALL_FIXES,
        ALL_KINDS,
        "a condition that is not a `bool`: Mzizi has no truthiness",
        "program t\n  fn main\n    let n = 1\n    when n\n      print(n)\n    end\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0714",
        "RFC-0008 §6, RFC-0013 §3.6",
        NONE_EXACT,
        ALL_KINDS,
        "a malformed `{…}` in text: empty, unclosed, a stray `}`, or more than one value",
        "program t\n  fn main\n    print(\"{}\")\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0901",
        "RFC-0013 §1, §16",
        NONE,
        PROGRAM,
        "a `program` line without a name, statements outside every `fn`, a block a program cannot hold (`view`, `prop`, `route`, …), or anything after `end program`",
        "program t\n  print(1)\n  fn main\n    print(1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0902",
        "RFC-0013 §1, §13, §16",
        NONE,
        PROGRAM,
        "no `fn main`, a second one, or `main` with parameters or a return type",
        "program t\n  fn helper\n    print(1)\n  end fn helper\nend program t\n",
    ),
    code(
        "MZ0903",
        "RFC-0013 §6.1, §16",
        NONE_EXACT,
        PROGRAM,
        "a malformed signature: `def` / `function` / `func` (exact `fn`), `->` (exact `:`), `()` on no parameters (exact delete), `: none` / `: void` (exact delete), an untyped parameter, a default value, a keyword as a name",
        "program t\n  fn main()\n    print(1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0904",
        "RFC-0013 §6.1, §16",
        NONE,
        PROGRAM,
        "two `fn`s with one name, a `fn` named like a built-in function, or a parameter named twice",
        "program t\n  fn main\n    print(1)\n  end fn main\n  fn f\n    print(1)\n  end fn f\n  fn f\n    print(2)\n  end fn f\nend program t\n",
    ),
    code(
        "MZ0905",
        "RFC-0013 §6.3, §16",
        NONE_GUESS,
        PROGRAM,
        "a call with the wrong number of arguments or an argument of the wrong type, or a named argument; the say quotes the signature. A `float` exponent to `pow` gets a guess (`2.0` to `2`, `0.5` to `x.sqrt()`)",
        "program t\n  fn main\n    print(twice(1, 2))\n  end fn main\n  fn twice(n: int): int\n    return n * 2\n  end fn twice\nend program t\n",
    ),
    code(
        "MZ0906",
        "RFC-0013 §6.2, §16",
        NONE_EXACT,
        PROGRAM,
        "a path through a function with a return type that reaches `end fn` without `return`; exact fix when the last line is a value of that type",
        "program t\n  fn main\n    print(one())\n  end fn main\n  fn one: int\n    let n = 1\n  end fn one\nend program t\n",
    ),
    code(
        "MZ0907",
        "RFC-0013 §6.2, §16",
        EXACT,
        PROGRAM,
        "a statement after a `return` that every path takes, which can never run; the exact fix deletes it",
        "program t\n  fn main\n    print(one())\n  end fn main\n  fn one: int\n    return 1\n    print(2)\n  end fn one\nend program t\n",
    ),
    code(
        "MZ0908",
        "RFC-0013 §6.2, §16",
        NONE,
        PROGRAM,
        "a `return` that does not fit its function: a value where none is returned, none where one is, or the wrong type",
        "program t\n  fn main\n    print(one())\n  end fn main\n  fn one: int\n    return \"one\"\n  end fn one\nend program t\n",
    ),
    code(
        "MZ0909",
        "RFC-0013 §6.4, §16",
        NONE_EXACT,
        PROGRAM,
        "a `fn` used as a value; exact fix `()` when it takes no parameters",
        "program t\n  fn main\n    print(one)\n  end fn main\n  fn one: int\n    return 1\n  end fn one\nend program t\n",
    ),
    code(
        "MZ0910",
        "RFC-0013 §3.3, §3.4, §16",
        ALL_FIXES,
        PROGRAM,
        "an operator spelt from another language: `==` `===` `!=` `!==` `&&` `||` `!` (exact), `not a is b` without parentheses (exact), a contract predicate in an expression (exact), or `**` (guess `x.pow(n)`; no fix when an operand could not be read)",
        "program t\n  fn main\n    print(1 == 1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0911",
        "RFC-0013 §16",
        ALL_FIXES,
        PROGRAM,
        "a comment Mzizi lacks: a line starting with `//` or `#` (exact `##`; a guess when code follows the marker with no space), `/* … */`, or a comment after code (guess: move it to its own line above); `/* … */` on a line of its own, or an unclosed `/*`, has no fix",
        "program t\n  // hello\n  fn main\n    print(1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0912",
        "RFC-0013 §3.2, §3.6, §16",
        ALL_FIXES,
        PROGRAM,
        "operands of the wrong type: `int` with `float` (exact on an `int` literal), `+` on text (fix to interpolation), arithmetic on a non-number, unlike types compared, `not` or `-` on the wrong type",
        "program t\n  fn main\n    let a = \"x\"\n    print(a + \"y\")\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0913",
        "RFC-0013 §3.3, §3.4, §16",
        EXACT_GUESS,
        PROGRAM,
        "`and` mixed with `or` without parentheses (the exact fix writes the parentheses the precedence implies), or a chained comparison `a < b < c` (guess: split it with `and`)",
        "program t\n  fn main\n    let a = true\n    let b = false\n    print(a and b or a)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0914",
        "RFC-0013 §3.1, §16",
        NONE_EXACT,
        PROGRAM,
        "a malformed number: `1.` or `.5` (exact: add the `0`), `1_000` (exact `1000`), `0x10`, `1e3`, or a float literal too large for a `float`",
        "program t\n  fn main\n    print(1.)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0915",
        "RFC-0013 §4.1, §4.4, §16",
        NONE,
        PROGRAM,
        "a fault visible in constants: division or remainder by a literal `0`, a literal negative `int` exponent, or a constant computation (including `pow` and `abs`) that overflows an `int`",
        "program t\n  fn main\n    print(1 / 0)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0916",
        "RFC-0013 §16",
        NONE,
        PROGRAM,
        "an expression statement whose value nothing reads",
        "program t\n  fn main\n    print(1)\n    one()\n  end fn main\n  fn one: int\n    return 1\n  end fn one\nend program t\n",
    ),
    code(
        "MZ0917",
        "RFC-0013 §16",
        NONE,
        PROGRAM,
        "a line a function body cannot read: a value expected and something else found, a missing `)`, an unknown escape, a second `else`, a Rust macro, or tokens left over",
        "program t\n  fn main\n    print(1) print(2)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0918",
        "RFC-0013 §5.1, §16",
        EXACT,
        PROGRAM,
        "`+=`, `-=`, `*=`, `/=`, `++` or `--`; the exact fix writes `x = x + 1`",
        "program t\n  fn main\n    var n = 1\n    n += 1\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0919",
        "RFC-0013 §16, §18",
        NONE,
        PROGRAM,
        "a form RFC-0013 designs that this compiler does not build yet (`while`, `for each`, `match`, `else when`, collections and options, methods on `text`, a program's `contract`, …), named and reported once",
        "program t\n  fn main\n    let n = 1\n    match n\n    end\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0920",
        "RFC-0013 §5.2, §16",
        NONE_GUESS,
        PROGRAM,
        "a name used before its binding, or after its block ended; the guess declares a `var` before the block",
        "program t\n  fn main\n    print(n)\n    let n = 1\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0921",
        "RFC-0013 §5.3, §16",
        NONE_GUESS,
        PROGRAM,
        "a name a program reserves: a word of the language, a built-in or function name, a name starting with `mz_`, or a name already bound in the function (no shadowing)",
        "program t\n  fn main\n    let print = 1\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0922",
        "RFC-0013 §5.1, §16",
        NONE_EXACT,
        PROGRAM,
        "assignment to a `let` (exact fix `var`) or to a parameter",
        "program t\n  fn main\n    let n = 1\n    n = 2\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0923",
        "RFC-0013 §5.1, §16",
        EXACT_GUESS,
        PROGRAM,
        "assignment to an unbound name; the fix inserts `let` or `var`",
        "program t\n  fn main\n    n = 2\n    print(n)\n  end fn main\nend program t\n",
    ),
    Code {
        code: "MZ0924",
        rfc: "RFC-0013 §5.1, §16",
        severity: Severity::Warning,
        tool: "mz check",
        say: "a warning: a `var` never assigned again; the exact fix writes `let`",
        fixes: EXACT,
        kinds: PROGRAM,
        trigger: Some(
            "program t\n  fn main\n    var n = 1\n    print(n)\n  end fn main\nend program t\n",
        ),
    },
    code(
        "MZ0925",
        "RFC-0013 §5.1, §16",
        EXACT,
        PROGRAM,
        "a binding form from another language: `const` `val` `auto` (exact `let`), `let mut` `mut` (exact `var`), `x := e`, or a type on an assignment",
        "program t\n  fn main\n    const n = 1\n    print(n)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0926",
        "RFC-0013 §5.1, §16",
        NONE,
        PROGRAM,
        "a binding with no value",
        "program t\n  fn main\n    let n\n    print(1)\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0937",
        "RFC-0013 §7.1, §16",
        EXACT,
        PROGRAM,
        "a trailing `:` on a block line; the exact fix deletes it",
        "program t\n  fn main\n    when true:\n      print(1)\n    end\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0962",
        "RFC-0013 §3.7, §3.8, §4.4, §16",
        ALL_FIXES,
        PROGRAM,
        "an operation spelt another way: `str(x)` or `x.to_string()` (exact `\"{x}\"`), or Python's free numeric functions, such as `abs(x)` (exact `x.abs()`) and `round(x)` or `pow(a, b)` (a guess, since the methods differ)",
        "program t\n  fn main\n    let n = 2\n    print(str(n))\n  end fn main\nend program t\n",
    ),
    code(
        "MZ0980",
        "RFC-0013 §1, §16",
        NONE_EXACT,
        PROGRAM,
        "a print spelt from another language: `print(a, b)` (exact, to one interpolated text), `console.log`, `println!`, `fmt.Println`, `puts`, `print x`",
        "program t\n  fn main\n    console.log(1)\n  end fn main\nend program t\n",
    ),
    Code {
        code: "MZ0990",
        rfc: "RFC-0013 §13.1, §16",
        severity: Severity::Error,
        tool: "mz run",
        say: "the Rust lowered from a program did not compile: a bug in mz, not in the program. It names the program and quotes rustc's first message; exit 3",
        fixes: NONE,
        kinds: PROGRAM,
        trigger: None,
    },
    Code {
        code: "MZ0991",
        rfc: "RFC-0013 §4.3, §16",
        severity: Severity::Error,
        tool: "mz run",
        say: "a trap while the program ran: integer overflow (including `pow` and `abs`), `int` division or remainder by zero, a negative `int` exponent, or `to_int()` on a float out of range. The line names the `.mz` position and the expression; exit 101",
        fixes: NONE,
        kinds: PROGRAM,
        trigger: None,
    },
];

/// Codes the compiler emits that have no entry yet: the component and service codes of
/// RFC-0001, RFC-0006, RFC-0008, RFC-0010 and RFC-0011. They are pending, not exempt: the
/// slice that gives `component` and `service` their full entries moves each one into
/// [`CODES`], and the drift test fails on any code that is on neither list. Kept sorted, and
/// frozen: `compiler/tests/harness.rs` holds a snapshot it may only shrink from.
pub const PENDING_CODES: &[&str] = &[
    "MZ0201", "MZ0202", "MZ0203", "MZ0209", "MZ0301", "MZ0302", "MZ0303", "MZ0304", "MZ0305",
    "MZ0307", "MZ0308", "MZ0309", "MZ0312", "MZ0313", "MZ0401", "MZ0402", "MZ0403", "MZ0404",
    "MZ0405", "MZ0406", "MZ0408", "MZ0409", "MZ0410", "MZ0501", "MZ0502", "MZ0601", "MZ0602",
    "MZ0603", "MZ0605", "MZ0606", "MZ0611", "MZ0612", "MZ0613", "MZ0702", "MZ0703", "MZ0704",
    "MZ0705", "MZ0706", "MZ0709", "MZ0710", "MZ0713", "MZ0715", "MZ0716", "MZ0801", "MZ0802",
    "MZ0803", "MZ0804", "MZ0805", "MZ0806", "MZ0807", "MZ0808", "MZ0809", "MZ0810", "MZ0811",
    "MZ0812",
];

/// The registered code, if any.
pub fn code_entry(code: &str) -> Option<&'static Code> {
    CODES.iter().find(|c| c.code == code)
}

/// Whether a code is registered or pending.
pub fn is_known(code: &str) -> bool {
    code_entry(code).is_some() || PENDING_CODES.contains(&code)
}

/// The registration rule at the protocol boundary: in a debug build (every `cargo test`),
/// a report that carries a code with no entry, or a registered code at another severity or
/// with a fix kind its entry does not declare, panics. A release `mz` does not pay for the check.
pub fn debug_assert_registered(report: &CheckReport) {
    if cfg!(debug_assertions) {
        for d in &report.diagnostics {
            assert!(
                is_known(d.code),
                "{} has no language-harness entry: register it in compiler/src/harness.rs (RFC-0012 §1.2)",
                d.code
            );
            if let Some(c) = code_entry(d.code) {
                assert_eq!(
                    c.severity, d.severity,
                    "{}'s harness entry gives another severity",
                    d.code
                );
                let kind = FixKind::of(d.fix.as_ref().map(|f| f.confidence));
                assert!(
                    c.fixes.contains(&kind),
                    "{} carried a `{}` fix, and its harness entry declares {:?}: {}",
                    d.code,
                    kind.as_str(),
                    c.fixes,
                    d.say
                );
            }
        }
    }
}

// ------------------------------------------------------------------- derived entries

/// An operator's teaching text and its RFC section. Matched on every variant: a new
/// operator in [`crate::expr`] does not compile until it is taught here.
fn binop_teach(op: BinOp) -> (&'static str, &'static str) {
    match op {
        BinOp::Add => (
            "Addition of two `int`s (trapping on overflow, exit 101) or two `float`s (IEEE 754, never a trap). An `int` and a `float` do not mix: convert with `to_float()` or `to_int()`. `+` does not join text: interpolate instead, `\"{a}{b}\"`.",
            "RFC-0013 §3.2, §4.1",
        ),
        BinOp::Sub => (
            "Subtraction of two `int`s (trapping on overflow) or two `float`s.",
            "RFC-0013 §3.2, §4.1",
        ),
        BinOp::Mul => (
            "Multiplication of two `int`s (trapping on overflow) or two `float`s.",
            "RFC-0013 §3.2, §4.1",
        ),
        BinOp::Div => (
            "Division. On `int` it truncates toward zero (`-7 / 2` is `-3`) and division by zero traps; by a literal `0` it is an error at check time. On `float` it is IEEE 754: `1.0 / 0.0` is `inf`, and nothing traps.",
            "RFC-0013 §3.2, §4.1",
        ),
        BinOp::Rem => (
            "Remainder, with the dividend's sign: `-7 % 2` is `-1`, `5.0 % 3.0` is `2.0`. A zero `int` divisor traps.",
            "RFC-0013 §3.2, §4.1",
        ),
        BinOp::Is => (
            "Equality of two values of one type. `==` and `===` are other languages': the exact fix writes `is`.",
            "RFC-0013 §3.3",
        ),
        BinOp::IsNot => (
            "Inequality of two values of one type. `!=` and `!==` are other languages': the exact fix writes `is not`.",
            "RFC-0013 §3.3",
        ),
        BinOp::Lt => (
            "Less than. Comparisons do not chain: write `a < b and b < c`.",
            "RFC-0013 §3.3",
        ),
        BinOp::Le => (
            "Less than or equal. Comparisons do not chain.",
            "RFC-0013 §3.3",
        ),
        BinOp::Gt => ("Greater than. Comparisons do not chain.", "RFC-0013 §3.3"),
        BinOp::Ge => (
            "Greater than or equal. Comparisons do not chain.",
            "RFC-0013 §3.3",
        ),
        BinOp::And => (
            "Logical and, short-circuit. `&&` is another language's: the exact fix writes `and`.",
            "RFC-0013 §3.4",
        ),
        BinOp::Or => (
            "Logical or, short-circuit. `||` is another language's: the exact fix writes `or`.",
            "RFC-0013 §3.4",
        ),
    }
}

/// A prefix operator's name, grammar, teaching text, types and precedence level. Matched on
/// every variant.
fn unop_entry(op: UnOp) -> (&'static str, &'static str, &'static str, &'static str, u8) {
    match op {
        UnOp::Neg => (
            "prefix -",
            "-<number>",
            "Negation of an `int` (negating `int`'s minimum traps) or a `float`.",
            "int → int; float → float",
            3,
        ),
        UnOp::Not => (
            "not",
            "not <bool>",
            "Logical not. It binds looser than comparison, so `not a is 3` is `not (a is 3)`. `!` is another language's: the exact fix writes `not`.",
            "bool → bool",
            7,
        ),
    }
}

/// A type's grammar, teaching text and RFC section. Matched on every variant, so a new type
/// does not compile until it has an arm here; `compiler/tests/harness.rs` fails unless the
/// arm is `Some` for exactly the surface types ([`Ty::is_surface`]).
fn type_teach(t: Ty) -> Option<(&'static [&'static str], &'static str, &'static str)> {
    match t {
        Ty::Int => Some((
            &["int", "42", "-7"],
            "A signed 64-bit integer. Overflow and division by zero trap when the program runs (exit 101, a line naming the `.mz` position); a constant one is an error at check time. An `int` never mixes with a `float`: `i.to_float()` converts.",
            "RFC-0013 §4.1, §4.3",
        )),
        Ty::Float => Some((
            &["float", "1.5", "0.25", "3.0"],
            "An IEEE 754 binary64 number, written with a digit on both sides of the point (`1.` and `.5` are `MZ0914`). Float arithmetic never traps: `1.0 / 0.0` is `inf`, `0.0 / 0.0` is `nan`, and `nan is nan` is `false`, so ask `f.is_nan()`. A float never mixes with an `int`: `f.to_int()` converts, and traps on `nan` or out of range. Its text form is the shortest that reads back exactly, such as `0.30000000000000004` and `1.0e21`.",
            "RFC-0013 §3.1, §3.8, §4.2",
        )),
        Ty::Bool => Some((
            &["bool", "true", "false"],
            "`true` or `false`. Conditions take a `bool` and nothing else: Mzizi has no truthiness.",
            "RFC-0013 §3.4",
        )),
        Ty::Text => Some((
            &["text", "\"hello\"", "\"{name}\""],
            "UTF-8 text. It is built by interpolation, not by `+`, and ordered by Unicode scalar value.",
            "RFC-0013 §3.6, §3.8",
        )),
        Ty::Nothing | Ty::Error => None,
    }
}

const TYPE_CODES: &[&str] = &["MZ0701", "MZ0711", "MZ0912"];

/// A numeric method's grammar, teaching text and RFC section, for each name in
/// [`crate::numbers::METHODS`], the table the checker types methods with.
/// `compiler/tests/harness.rs` fails when a method there has no text here.
fn method_teach(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "to_float" => (
            "i.to_float()",
            "The nearest `float` to an `int`; exact up to 2^53.",
        ),
        "to_int" => (
            "f.to_int()",
            "A `float` truncated toward zero, as an `int`. It traps (exit 101) on `nan`, an infinity, or a value outside `int`.",
        ),
        "round" => (
            "f.round()",
            "The nearest whole `float`, with halves away from zero: `2.5.round()` is `3.0`. Python's `round(x)` is `MZ0962` with a guess fix, because it rounds halves to even.",
        ),
        "floor" => ("f.floor()", "The largest whole `float` not above `f`."),
        "ceil" => ("f.ceil()", "The smallest whole `float` not below `f`."),
        "sqrt" => (
            "f.sqrt()",
            "The square root; `nan` below zero, as IEEE 754 says.",
        ),
        "is_nan" => (
            "f.is_nan()",
            "Whether `f` is `nan`: the only way to ask, since `nan is nan` is `false`.",
        ),
        "abs" => (
            "x.abs()",
            "The absolute value of an `int` or a `float`. On `int` it traps on `int`'s minimum.",
        ),
        "min" => (
            "x.min(y)",
            "The smaller of two values of one numeric type. On floats it is Rust's `f64::min`, so `nan.min(1.0)` is `1.0`.",
        ),
        "max" => (
            "x.max(y)",
            "The larger of two values of one numeric type. On floats it is Rust's `f64::max`.",
        ),
        "pow" => (
            "x.pow(n)",
            "`x` to the power `n`, an `int`. On `int` it traps on overflow and on a negative `n` (a literal negative `n` is `MZ0915`); on `float` it is `x.powf(n.to_float())`. `**` is `MZ0910`, with the guess fix `x.pow(n)`.",
        ),
        _ => return None,
    })
}

/// The whole registry, in a stable order: kind by kind, and within a kind in the order the
/// tables above give.
pub fn registry() -> Vec<HarnessEntry> {
    let mut out = Vec::new();
    for f in FEATURES {
        out.push(HarnessEntry {
            name: f.name,
            kind: f.kind,
            depth: f.depth,
            rfc: f.rfc,
            grammar: f.grammar.to_vec(),
            teach: f.teach.to_string(),
            types: f.types.to_string(),
            precedence: None,
            codes: f.codes.to_vec(),
            examples: f.examples.to_vec(),
            diagnostic: None,
        });
    }
    for t in Ty::surface() {
        // `None` for a surface type fails `compiler/tests/harness.rs`; it is never skipped
        // silently there.
        let Some((grammar, teach, rfc)) = type_teach(t) else {
            continue;
        };
        let ops: Vec<&str> = BinOp::ALL
            .iter()
            .filter(|op| binary_type(**op, t, t).is_ok())
            .map(|op| op.text())
            .collect();
        let text_form = if has_text_form(t) {
            "has a text form, so it can be printed and interpolated"
        } else {
            "has no text form"
        };
        out.push(HarnessEntry {
            name: t.name(),
            kind: Kind::Type,
            depth: Depth::Full,
            rfc,
            grammar: grammar.to_vec(),
            teach: teach.to_string(),
            types: {
                let methods = crate::numbers::methods_of(t);
                let methods = if methods.is_empty() {
                    String::new()
                } else {
                    format!(
                        " Methods: {}.",
                        methods
                            .iter()
                            .map(|m| format!("`{m}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    "Binary operators on two `{}`s: {}. It {text_form}.{methods}",
                    t.name(),
                    ops.iter()
                        .map(|o| format!("`{o}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            precedence: None,
            codes: match t {
                Ty::Int => vec!["MZ0701", "MZ0711", "MZ0912", "MZ0103", "MZ0915", "MZ0991"],
                Ty::Float => vec!["MZ0701", "MZ0711", "MZ0912", "MZ0914", "MZ0991"],
                _ => TYPE_CODES.to_vec(),
            },
            examples: vec![match t {
                Ty::Int => run(ARITH, ARITH_OUT),
                Ty::Float => run(NUMBERS, NUMBERS_OUT),
                Ty::Bool => run(LOGIC, LOGIC_OUT),
                _ => run(TEXT, "hello, Mzizi: 2 {braces} and \"quotes\"\n"),
            }],
            diagnostic: None,
        });
    }
    for &op in BinOp::ALL {
        let (teach, rfc) = binop_teach(op);
        let operands: Vec<String> = Ty::surface()
            .filter_map(|t| {
                binary_type(op, t, t)
                    .ok()
                    .map(|r| format!("{0} {1} {0} → {2}", t.name(), op.text(), r.name()))
            })
            .collect();
        let mut codes = vec!["MZ0912"];
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                codes.extend(["MZ0915", "MZ0991"])
            }
            BinOp::Is | BinOp::IsNot | BinOp::And | BinOp::Or => codes.push("MZ0910"),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => codes.push("MZ0913"),
        }
        if matches!(op, BinOp::And | BinOp::Or) {
            codes.push("MZ0913");
        }
        out.push(HarnessEntry {
            name: op.text(),
            kind: Kind::Operator,
            depth: Depth::Full,
            rfc,
            grammar: vec![op_grammar(op)],
            teach: teach.to_string(),
            types: operands.join("; "),
            precedence: Some(op.level()),
            codes,
            examples: vec![if op.is_arithmetic() {
                run(ARITH, ARITH_OUT)
            } else {
                run(LOGIC, LOGIC_OUT)
            }],
            diagnostic: None,
        });
    }
    for &op in UnOp::ALL {
        let (name, grammar, teach, types, level) = unop_entry(op);
        out.push(HarnessEntry {
            name,
            kind: Kind::Operator,
            depth: Depth::Full,
            rfc: "RFC-0013 §3.2, §3.4, §3.5",
            grammar: vec![grammar],
            teach: teach.to_string(),
            types: types.to_string(),
            precedence: Some(level),
            codes: if op == UnOp::Not {
                vec!["MZ0912", "MZ0910"]
            } else {
                vec!["MZ0912", "MZ0915", "MZ0991"]
            },
            examples: vec![if op == UnOp::Not {
                run(LOGIC, LOGIC_OUT)
            } else {
                run(ARITH, ARITH_OUT)
            }],
            diagnostic: None,
        });
    }
    for name in crate::numbers::METHODS {
        let Some((grammar, teach)) = method_teach(name) else {
            continue;
        };
        let sigs: Vec<String> = Ty::surface()
            .filter_map(|t| {
                crate::numbers::method(t, name).map(|(params, ret)| {
                    let params: Vec<&str> = params.iter().map(|p| p.name()).collect();
                    format!(
                        "{}.{name}({}) → {}",
                        t.name(),
                        params.join(", "),
                        ret.name()
                    )
                })
            })
            .collect();
        out.push(HarnessEntry {
            name,
            kind: Kind::Method,
            depth: Depth::Full,
            rfc: "RFC-0013 §3.7, §4.4",
            grammar: vec![grammar],
            teach: teach.to_string(),
            types: sigs.join("; "),
            precedence: Some(2),
            codes: vec!["MZ0708", "MZ0905", "MZ0915", "MZ0991"],
            examples: vec![run(METHODS, METHODS_OUT)],
            diagnostic: None,
        });
    }
    for c in COMMANDS {
        out.push(HarnessEntry {
            name: c.name,
            kind: Kind::Command,
            depth: Depth::Full,
            rfc: c.rfc,
            grammar: vec![c.usage],
            teach: c.teach.to_string(),
            types: String::new(),
            precedence: None,
            codes: if c.word == "run" {
                vec!["MZ0990", "MZ0991"]
            } else {
                Vec::new()
            },
            examples: Vec::new(),
            diagnostic: None,
        });
    }
    for c in CODES {
        out.push(HarnessEntry {
            name: c.code,
            kind: Kind::Diagnostic,
            depth: Depth::Full,
            rfc: c.rfc,
            grammar: Vec::new(),
            teach: c.say.to_string(),
            types: String::new(),
            precedence: None,
            codes: vec![c.code],
            examples: Vec::new(),
            diagnostic: Some(*c),
        });
    }
    // Kind by kind; a stable sort keeps each table's own order within a kind.
    out.sort_by_key(|e| e.kind);
    out
}

fn op_grammar(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "<a> + <b>",
        BinOp::Sub => "<a> - <b>",
        BinOp::Mul => "<a> * <b>",
        BinOp::Div => "<a> / <b>",
        BinOp::Rem => "<a> % <b>",
        BinOp::Is => "<a> is <b>",
        BinOp::IsNot => "<a> is not <b>",
        BinOp::Lt => "<a> < <b>",
        BinOp::Le => "<a> <= <b>",
        BinOp::Gt => "<a> > <b>",
        BinOp::Ge => "<a> >= <b>",
        BinOp::And => "<a> and <b>",
        BinOp::Or => "<a> or <b>",
    }
}

/// The entry that registers a statement form. Matched on every variant, so a statement the
/// parser can build does not compile until it names its entry; `compiler/tests/harness.rs`
/// checks the name is registered.
pub fn statement_entry(kind: &crate::program::StmtKind) -> &'static str {
    use crate::program::StmtKind;
    match kind {
        StmtKind::Bind { mutable: false, .. } => "let",
        StmtKind::Bind { mutable: true, .. } => "var",
        StmtKind::Assign { .. } => "assignment",
        StmtKind::Return(_) => "return",
        StmtKind::When { .. } => "when",
        StmtKind::Expr(_) => "expression statement",
    }
}

/// The entry that registers an expression form, as [`statement_entry`] does for statements.
/// Each form names its own entry: a literal its type's (or `text literal`), a name `names`,
/// a call `print` or `fn`, an operator or a method its own, and a method the checker does not
/// know `MZ0708`, the code that rejects it.
pub fn expression_entry(kind: &crate::expr::ExprKind) -> &'static str {
    use crate::expr::ExprKind;
    match kind {
        ExprKind::Int(_) => "int",
        ExprKind::Float(_) => "float",
        ExprKind::Bool(_) => "bool",
        ExprKind::Text(_) => "text literal",
        ExprKind::Name(_) => "names",
        ExprKind::Method { name, .. } => crate::numbers::METHODS
            .iter()
            .find(|m| *m == name)
            .copied()
            .unwrap_or("MZ0708"),
        ExprKind::Call { name, .. } if name == "print" => "print",
        ExprKind::Call { .. } => "fn",
        ExprKind::Unary { op, .. } => unop_entry(*op).0,
        ExprKind::Binary { op, .. } => op.text(),
        ExprKind::Error => "MZ0917",
    }
}

/// Every operator spelling a program's lexer reads ([`crate::lex::OPERATORS`]), and the
/// entry an agent reads for it: the operator's own, or the code that rejects a spelling from
/// another language. `compiler/tests/harness.rs` checks that the two lists match, that each
/// entry exists, and that each rejected spelling is reported with its code.
pub const LEXED_OPERATORS: &[(&str, &str)] = &[
    ("===", "MZ0910"),
    ("!==", "MZ0910"),
    ("==", "MZ0910"),
    ("!=", "MZ0910"),
    ("<=", "<="),
    (">=", ">="),
    ("->", "MZ0903"),
    ("=>", "MZ0917"),
    ("&&", "MZ0910"),
    ("||", "MZ0910"),
    ("+=", "MZ0918"),
    ("-=", "MZ0918"),
    ("*=", "MZ0918"),
    ("/=", "MZ0918"),
    ("++", "MZ0918"),
    ("--", "MZ0918"),
    ("**", "MZ0910"),
    ("+", "+"),
    ("-", "-"),
    ("*", "*"),
    ("/", "/"),
    ("%", "%"),
    ("<", "<"),
    (">", ">"),
    ("!", "MZ0910"),
];

/// One entry by name.
pub fn entry(name: &str) -> Option<HarnessEntry> {
    registry().into_iter().find(|e| e.name == name)
}

/// Whether `mz <word> <file>` is a registered command.
pub fn takes_file(word: &str) -> bool {
    COMMANDS.iter().any(|c| c.word == word && c.takes_file)
}

// ------------------------------------------------------------------- JSON

/// A JSON value, rendered by hand: the crate has no dependencies.
enum J {
    S(String),
    N(u64),
    A(Vec<J>),
    O(Vec<(&'static str, J)>),
}

fn s(v: &str) -> J {
    J::S(v.to_string())
}

/// Write `j` to `out`: on one line, or with `indent` spaces per level and a space after
/// each `:`.
fn render(j: &J, out: &mut String, indent: Option<usize>, depth: usize) {
    let pad = |out: &mut String, depth: usize| {
        if let Some(n) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(n * depth));
        }
    };
    match j {
        J::S(v) => out.push_str(&json_string(v)),
        J::N(n) => write!(out, "{n}").unwrap(),
        J::A(items) if items.is_empty() => out.push_str("[]"),
        J::A(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                render(item, out, indent, depth + 1);
            }
            pad(out, depth);
            out.push(']');
        }
        J::O(fields) => {
            out.push('{');
            for (i, (k, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                out.push_str(&json_string(k));
                out.push_str(if indent.is_some() { ": " } else { ":" });
                render(v, out, indent, depth + 1);
            }
            pad(out, depth);
            out.push('}');
        }
    }
}

fn code_json(c: &Code) -> J {
    let mut f = vec![
        ("code", s(c.code)),
        (
            "severity",
            s(match c.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            }),
        ),
        ("tool", s(c.tool)),
        ("say", s(c.say)),
        (
            "fixes",
            J::A(c.fixes.iter().map(|k| s(k.as_str())).collect()),
        ),
        ("kinds", J::A(c.kinds.iter().map(|k| s(k)).collect())),
    ];
    if let Some(t) = c.trigger {
        f.push(("trigger", s(t)));
    }
    J::O(f)
}

fn entry_json(e: &HarnessEntry) -> J {
    let mut f = vec![
        ("name", s(e.name)),
        ("kind", s(e.kind.as_str())),
        ("depth", s(e.depth.as_str())),
        ("rfc", s(e.rfc)),
        ("teach", s(&e.teach)),
    ];
    if !e.grammar.is_empty() {
        f.push(("grammar", J::A(e.grammar.iter().map(|g| s(g)).collect())));
    }
    if !e.types.is_empty() {
        f.push(("types", s(&e.types)));
    }
    if let Some(p) = e.precedence {
        f.push(("precedence", J::N(u64::from(p))));
    }
    if let Some(c) = &e.diagnostic {
        f.push(("diagnostic", code_json(c)));
    } else if !e.codes.is_empty() {
        f.push((
            "codes",
            J::A(
                e.codes
                    .iter()
                    .map(|code| match code_entry(code) {
                        Some(c) => J::O(vec![
                            ("code", s(c.code)),
                            ("say", s(c.say)),
                            (
                                "fixes",
                                J::A(c.fixes.iter().map(|k| s(k.as_str())).collect()),
                            ),
                        ]),
                        None => J::O(vec![("code", s(code))]),
                    })
                    .collect(),
            ),
        ));
    }
    if !e.examples.is_empty() {
        f.push((
            "examples",
            J::A(
                e.examples
                    .iter()
                    .map(|x| {
                        let mut o = vec![("source", s(x.source))];
                        if let Some(out) = x.output {
                            o.push(("output", s(out)));
                        }
                        J::O(o)
                    })
                    .collect(),
            ),
        ));
    }
    J::O(f)
}

fn to_text(j: &J, pretty: bool) -> String {
    let mut out = String::new();
    render(j, &mut out, pretty.then_some(2), 0);
    out.push('\n');
    out
}

/// The language as an agent reads it (RFC-0012 §1, §5): every entry, and the pending
/// codes, as deterministic JSON. `pretty` indents it; otherwise it is one line.
pub fn definition(pretty: bool) -> String {
    let doc = J::O(vec![
        ("protocol", J::N(u64::from(PROTOCOL))),
        ("language", s(LANGUAGE)),
        ("entries", J::A(registry().iter().map(entry_json).collect())),
        (
            "pending_codes",
            J::A(PENDING_CODES.iter().map(|c| s(c)).collect()),
        ),
    ]);
    to_text(&doc, pretty)
}

/// One entry as JSON, or `None` for an unknown name.
pub fn entry_text(name: &str, pretty: bool) -> Option<String> {
    entry(name).map(|e| to_text(&entry_json(&e), pretty))
}

/// `mz harness version`: one JSON line with the protocol version, the language version and
/// the SHA-256 of the one-line definition, which changes whenever any entry does.
pub fn version() -> String {
    let hash = crate::hash::sha256(definition(false).as_bytes()).to_hex();
    format!(
        "{{\"protocol\":{PROTOCOL},\"language\":{},\"definition_sha256\":\"{hash}\"}}\n",
        json_string(LANGUAGE)
    )
}
