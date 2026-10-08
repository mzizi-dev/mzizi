//! Collections in a program (RFC-0013 §9): the methods of §9.2 and the named folds of §9.4,
//! as one table. The checker (`program/collections.rs`) types each call against it, the
//! lowering (`run/collections.rs`) emits it, and the language harness registers one entry per
//! row, so a method cannot be added to one without the others noticing: the harness's tests
//! compare this table with its entries.

/// What a method is called on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recv {
    /// `list(T)`.
    List,
    /// `map(K, V)`.
    Map,
    /// `set(K)`.
    Set,
}

/// One method of a collection.
#[derive(Clone, Copy, Debug)]
pub struct Method {
    /// Its name.
    pub name: &'static str,
    /// The collections that have it.
    pub on: &'static [Recv],
    /// How many arguments it takes.
    pub arity: usize,
    /// How it is written.
    pub grammar: &'static str,
    /// Its types, in brief.
    pub types: &'static str,
    /// What an agent is told.
    pub teach: &'static str,
    /// Whether it changes its receiver, which must then be a `var` (`MZ0960`).
    pub mutates: bool,
    /// Whether it takes a named `fn` (RFC-0013 §6.4, `MZ0909`).
    pub takes_fn: bool,
}

const L: &[Recv] = &[Recv::List];
const ALL: &[Recv] = &[Recv::List, Recv::Map, Recv::Set];

/// Every method of a collection, in RFC-0013 §9.2's order and then §9.4's.
pub const METHODS: &[Method] = &[
    Method {
        name: "length",
        on: ALL,
        arity: 0,
        grammar: "c.length()",
        types: "list(T), map(K, V), set(K) → int",
        teach: "How many elements a list or a set has, or how many keys a map has. `len(x)`, `.len()`, `.size()`, `.count()` and `.length` with no parentheses are other languages': the exact fix writes `x.length()`. Emptiness is `c is none`, not `c.length() is 0`.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "slice",
        on: L,
        arity: 2,
        grammar: "xs.slice(<from>, to = <to>)",
        types: "list(T).slice(int, to = int) → option(list(T))",
        teach: "The elements from position `a` up to but not including `b`, as an option: `none` when either end is outside the list or `a` is past `b`. Give it a default with `otherwise`: `xs.slice(1, to = 3) otherwise []`.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "keys",
        on: &[Recv::Map],
        arity: 0,
        grammar: "m.keys()",
        types: "map(K, V) → list(K)",
        teach: "A map's keys, as a list in key order. `for each k in m` iterates a map through its keys: the exact fix writes `m.keys()`.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "values",
        on: &[Recv::Map],
        arity: 0,
        grammar: "m.values()",
        types: "map(K, V) → list(V)",
        teach: "A map's values, as a list in the order of their keys.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "to_list",
        on: &[Recv::Set],
        arity: 0,
        grammar: "s.to_list()",
        types: "set(K) → list(K)",
        teach: "A set's elements, as a list in key order. `for each x in s` iterates a set through it: the exact fix writes `s.to_list()`.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "map",
        on: L,
        arity: 1,
        grammar: "xs.map(<fn>)",
        types: "list(T).map(fn(T): U) → list(U)",
        teach: "A new list of `f(x)` for each element, in order. `f` is the name of a `fn` of the program with one parameter of the element type: Mzizi has no lambdas, so `x => …` is an error that says to declare the `fn`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "filter",
        on: L,
        arity: 1,
        grammar: "xs.filter(<fn>)",
        types: "list(T).filter(fn(T): bool) → list(T)",
        teach: "A new list of the elements `f` is `true` for, in order. `f` is a named `fn` from the element type to `bool`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "join",
        on: L,
        arity: 1,
        grammar: "xs.join(<separator>)",
        types: "list(text).join(text) → text",
        teach: "A list of text as one text, with the separator between each two.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "push",
        on: L,
        arity: 1,
        grammar: "xs.push(<value>)",
        types: "list(T).push(T), on a var",
        teach: "Appends a value to a list held by a `var`; on a `let` it is an error whose exact fix makes the binding a `var`. Python's `.append` gets the exact fix `.push`.",
        mutates: true,
        takes_fn: false,
    },
    Method {
        name: "insert",
        on: &[Recv::Set],
        arity: 1,
        grammar: "s.insert(<value>)",
        types: "set(K).insert(K), on a var",
        teach: "Adds a value to a set held by a `var`; a value already there stays once. A map's entry is set with `m[k] = v`.",
        mutates: true,
        takes_fn: false,
    },
    Method {
        name: "remove",
        on: &[Recv::Map, Recv::Set],
        arity: 1,
        grammar: "c.remove(<key>)",
        types: "map(K, V).remove(K), set(K).remove(K), on a var",
        teach: "Removes a key from a map, or a value from a set, held by a `var`, if it is there.",
        mutates: true,
        takes_fn: false,
    },
    Method {
        name: "count",
        on: L,
        arity: 1,
        grammar: "xs.count(<fn>)",
        types: "list(T).count(fn(T): bool) → int",
        teach: "How many elements `f` is `true` for. `count()` with no argument is the size, whose exact fix is `length()`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "sum",
        on: L,
        arity: 0,
        grammar: "xs.sum()",
        types: "list(int) → int; list(float) → float",
        teach: "The sum of a list of ints or of floats: `0` or `0.0` when it is empty. An `int` sum traps on overflow (exit 101), as `+` does.",
        mutates: false,
        takes_fn: false,
    },
    Method {
        name: "any",
        on: L,
        arity: 1,
        grammar: "xs.any(<fn>)",
        types: "list(T).any(fn(T): bool) → bool",
        teach: "Whether `f` is `true` for some element, stopping at the first; `false` on an empty list. JavaScript's `some` gets the exact fix `any`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "all",
        on: L,
        arity: 1,
        grammar: "xs.all(<fn>)",
        types: "list(T).all(fn(T): bool) → bool",
        teach: "Whether `f` is `true` for every element, stopping at the first that is not; `true` on an empty list. JavaScript's `every` gets the exact fix `all`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "first",
        on: L,
        arity: 1,
        grammar: "xs.first(<fn>)",
        types: "list(T).first(fn(T): bool) → option(T)",
        teach: "The first element `f` is `true` for, as an option: `none` when there is none. JavaScript's `find` gets the exact fix `first`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "fold",
        on: L,
        arity: 2,
        grammar: "xs.fold(<init>, step = <fn>)",
        types: "list(T).fold(A, step = fn(A, T): A) → A",
        teach: "Combines the elements left to right: starting from `init`, each step is `f(acc, x)`, and the last is the value. `f` is a named `fn` whose first parameter and return type are the accumulator's. JavaScript's `reduce(f, init)` gets the guess `fold(init, step = f)`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "sort_by",
        on: L,
        arity: 1,
        grammar: "xs.sort_by(<fn>)",
        types: "list(T).sort_by(fn(T): K) → list(T), K an int, text, bool or enum",
        teach: "A new list of the elements in ascending order of `f(x)`; stable, so elements with equal keys keep their order. The key is an int, a text, a bool or an enum: a float has no total order (`MZ0964`). Python's `sorted(xs, key = f)` gets the exact fix `xs.sort_by(f)`.",
        mutates: false,
        takes_fn: true,
    },
    Method {
        name: "group_by",
        on: L,
        arity: 1,
        grammar: "xs.group_by(<fn>)",
        types: "list(T).group_by(fn(T): K) → map(K, list(T))",
        teach: "A map from each key `f(x)` to the elements with that key, each list in the elements' order. The key is an int, a text, a bool or an enum.",
        mutates: false,
        takes_fn: true,
    },
];

/// The method `name` on a collection of kind `on`, if it has one.
pub fn method(on: Recv, name: &str) -> Option<&'static Method> {
    METHODS
        .iter()
        .find(|m| m.name == name && m.on.contains(&on))
}

/// The methods a collection of kind `on` has, alphabetically, for the nearest-name fix.
pub fn methods_of(on: Recv) -> Vec<&'static str> {
    let mut names: Vec<&str> = METHODS
        .iter()
        .filter(|m| m.on.contains(&on))
        .map(|m| m.name)
        .collect();
    names.sort_unstable();
    names
}

/// Whether `name` is a method of some collection.
pub fn is_method(name: &str) -> bool {
    METHODS.iter().any(|m| m.name == name)
}
