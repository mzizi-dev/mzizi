//! The content-addressed IR (RFC-0003).
//!
//! A node's identity **is** the hash of its content, children included. That single
//! decision buys the things listed in RFC-0003 §1: structural sharing (identical subtrees
//! are stored once), stable identity across time, a moved node keeping its hash so a diff
//! is a semantic diff, and an explicit reference graph instead of grep.
//!
//! Names live outside the nodes, in [`Store::names`], which is why a rename touches one
//! entry rather than every call site.

use std::collections::BTreeMap;

use crate::ast::{Component, Element};
use crate::hash::{Hash, sha256};

/// One IR node. Leaf data lives in `fields`; structure lives in `children`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// What sort of node this is (`component`, `prop`, `enum`, `variant`, `element`, …).
    pub kind: String,
    /// The author-visible name, for kinds that have one.
    pub label: Option<String>,
    /// Leaf data, kept sorted by key so authoring order cannot change identity.
    pub fields: BTreeMap<String, String>,
    /// Child hashes, in order — order is semantic (a view's children are a sequence).
    pub children: Vec<Hash>,
}

impl Node {
    /// A node with no fields and no children.
    pub fn leaf(kind: &str, label: Option<&str>) -> Self {
        Node {
            kind: kind.to_string(),
            label: label.map(str::to_string),
            fields: BTreeMap::new(),
            children: Vec::new(),
        }
    }

    /// Add a field, returning the node for chaining.
    pub fn field(mut self, key: &str, value: impl Into<String>) -> Self {
        self.fields.insert(key.to_string(), value.into());
        self
    }

    /// Add a child hash.
    pub fn child(mut self, hash: Hash) -> Self {
        self.children.push(hash);
        self
    }

    /// The canonical byte form that gets hashed (RFC-0003 §2.1).
    ///
    /// Length-prefixed rather than delimiter-separated, deliberately: a delimiter can occur
    /// inside a value (a Tailwind class string contains almost every punctuation mark), and
    /// two different nodes must never serialize to the same bytes. With lengths, they
    /// cannot.
    pub fn canonical(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(128);
        push_sized(&mut out, self.kind.as_bytes());
        match &self.label {
            Some(l) => push_sized(&mut out, l.as_bytes()),
            None => out.extend_from_slice(b"-"),
        }
        out.extend_from_slice(self.fields.len().to_string().as_bytes());
        out.push(b':');
        for (k, v) in &self.fields {
            push_sized(&mut out, k.as_bytes());
            push_sized(&mut out, v.as_bytes());
        }
        out.extend_from_slice(self.children.len().to_string().as_bytes());
        out.push(b':');
        for child in &self.children {
            out.extend_from_slice(child.as_bytes());
        }
        out
    }

    /// This node's content hash.
    pub fn hash(&self) -> Hash {
        sha256(&self.canonical())
    }
}

fn push_sized(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes.len().to_string().as_bytes());
    out.push(b':');
    out.extend_from_slice(bytes);
}

/// The node store plus the name namespace.
///
/// Inserting a node that is already present is a no-op returning the existing hash — which
/// is what makes structural sharing automatic rather than an optimization pass.
#[derive(Debug, Default)]
pub struct Store {
    nodes: BTreeMap<Hash, Node>,
    /// `name -> hash`. Renaming edits this and nothing else (RFC-0003 §3).
    pub names: BTreeMap<String, Hash>,
}

impl Store {
    /// An empty store.
    pub fn new() -> Self {
        Store::default()
    }

    /// Insert a node, returning its hash. Idempotent: identical content stores once.
    pub fn put(&mut self, node: Node) -> Hash {
        let hash = node.hash();
        self.nodes.entry(hash).or_insert(node);
        hash
    }

    /// Fetch a node by hash.
    pub fn get(&self, hash: Hash) -> Option<&Node> {
        self.nodes.get(&hash)
    }

    /// How many distinct nodes are stored — the number that shows sharing working.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the store holds nothing.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Bind a name to a hash.
    pub fn bind(&mut self, name: &str, hash: Hash) {
        self.names.insert(name.to_string(), hash);
    }

    /// Resolve a name to its hash.
    pub fn resolve(&self, name: &str) -> Option<Hash> {
        self.names.get(name).copied()
    }

    /// Rename a binding. Touches one entry; no node changes, so no hash changes.
    ///
    /// This is the operation that is free because names are metadata — and it is exactly
    /// the multi-site refactor that small models perform worst (RFC-0003 §3).
    pub fn rename(&mut self, from: &str, to: &str) -> bool {
        match self.names.remove(from) {
            Some(hash) => {
                self.names.insert(to.to_string(), hash);
                true
            }
            None => false,
        }
    }

    /// Every node that lists `target` among its children — "who points at this?" as a
    /// lookup rather than a grep (RB-3).
    pub fn referrers(&self, target: Hash) -> Vec<Hash> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.children.contains(&target))
            .map(|(h, _)| *h)
            .collect()
    }

    /// Every hash reachable from `root`, including it.
    pub fn reachable(&self, root: Hash) -> Vec<Hash> {
        let mut seen = Vec::new();
        let mut stack = vec![root];
        while let Some(h) = stack.pop() {
            if seen.contains(&h) {
                continue;
            }
            seen.push(h);
            if let Some(node) = self.get(h) {
                for c in &node.children {
                    stack.push(*c);
                }
            }
        }
        seen.sort();
        seen
    }
}

/// Lower a parsed component into the store, returning the root hash.
pub fn lower(component: &Component, store: &mut Store) -> Hash {
    let mut children = Vec::new();

    for cap in &component.uses {
        children.push(store.put(Node::leaf("capability", Some(cap))));
    }

    for e in &component.enums {
        let mut variants = Vec::new();
        for v in &e.variants {
            let mut node = Node::leaf("variant", Some(&v.name));
            for (col, value) in &v.columns {
                node = node.field(col, value.clone());
            }
            variants.push(store.put(node));
        }
        let mut enum_node = Node::leaf("enum", Some(&e.name));
        for v in variants {
            enum_node = enum_node.child(v);
        }
        children.push(store.put(enum_node));
    }

    for p in &component.props {
        let mut node = Node::leaf("prop", Some(&p.name)).field("type", p.ty.clone());
        if p.has_default {
            node = node.field("has_default", "true");
        }
        children.push(store.put(node));
    }

    if let Some(view) = &component.view {
        let mut view_node = Node::leaf("view", None);
        for el in view {
            view_node = view_node.child(lower_element(el, store));
        }
        children.push(store.put(view_node));
    }

    for f in &component.fns {
        children.push(store.put(Node::leaf("fn", Some(f))));
    }

    if let Some(contract) = &component.contract {
        // Contracts participate in the hash (RFC-0006 §6). A component that promises more
        // about its behaviour is not the same component, and RB-6's "re-verify a recorded
        // fact by asking whether that hash still exists" is only true if the hash covers
        // the promise. It costs nothing: the Merkle structure means changing an assertion
        // leaves every implementation subtree's hash untouched.
        //
        // Clause children are sorted by hash and de-duplicated, because a contract is a
        // set of assertions rather than a sequence — reordering the lines of a contract
        // changes no meaning and must therefore change no identity.
        let mut clause_hashes: Vec<Hash> = contract
            .clauses
            .iter()
            .map(|c| store.put(Node::leaf("clause", None).field("assert", c.canonical())))
            .collect();
        clause_hashes.sort();
        clause_hashes.dedup();
        let mut node = Node::leaf("contract", None);
        for clause in clause_hashes {
            node = node.child(clause);
        }
        children.push(store.put(node));
    }

    let mut root = Node::leaf("component", Some(&component.name));
    if !component.docs.is_empty() {
        root = root.field("doc", component.docs.join(" "));
    }
    for c in children {
        root = root.child(c);
    }
    let hash = store.put(root);
    store.bind(&component.name, hash);
    hash
}

fn lower_element(el: &Element, store: &mut Store) -> Hash {
    let mut node = Node::leaf("element", Some(&el.tag));
    for (name, value) in &el.attrs {
        let key = if name.is_empty() {
            "value"
        } else {
            name.as_str()
        };
        node = node.field(key, value.clone());
    }
    let mut children = Vec::new();
    for child in &el.children {
        children.push(lower_element(child, store));
    }
    for c in children {
        node = node.child(c);
    }
    store.put(node)
}

/// The structural path of every node reachable from a component root, e.g.
/// `connectivity_bar/fn:retry` or `alert/view/element:notice/element:row@alert-title`.
///
/// This is the address an agent patches by, and it must be **unique** — two nodes sharing a
/// path would make a patch ambiguous. Sibling elements with the same tag are therefore
/// disambiguated: by their `slot` when they have one (readable, and `slot` is already the
/// design system's identity attribute), and by index when they do not.
///
/// Paths are readable and survive reformatting, which a line number does not (RB-1). They
/// are **not** permanently stable: inserting a sibling shifts an index-disambiguated path.
/// The permanently stable identity is the hash — paths address a tree as it is now, hashes
/// address content forever. Callers holding a reference across edits should hold the hash.
pub fn paths(store: &Store, root: Hash) -> Vec<(String, Hash)> {
    let mut out = Vec::new();
    let Some(node) = store.get(root) else {
        return out;
    };
    let base = node.label.clone().unwrap_or_else(|| node.kind.clone());
    out.push((base.clone(), root));
    walk_paths(store, root, &base, &mut out);
    out
}

fn walk_paths(store: &Store, hash: Hash, prefix: &str, out: &mut Vec<(String, Hash)>) {
    let Some(node) = store.get(hash) else {
        return;
    };

    // Base segment per child, then disambiguate any that repeat.
    let mut segments: Vec<String> = Vec::with_capacity(node.children.len());
    for child in &node.children {
        let Some(cn) = store.get(*child) else {
            segments.push("missing".to_string());
            continue;
        };
        segments.push(match &cn.label {
            Some(label) => format!("{}:{}", cn.kind, label),
            None => cn.kind.clone(),
        });
    }

    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    for (i, child) in node.children.iter().enumerate() {
        let base = &segments[i];
        let repeats = segments.iter().filter(|s| *s == base).count() > 1;
        let segment = if repeats {
            // Prefer a `slot`, which names the node in the way the design system already
            // does; fall back to the ordinal among same-named siblings.
            let by_slot = store
                .get(*child)
                .and_then(|cn| cn.fields.get("slot"))
                .map(|s| s.trim_matches('"').to_string())
                .filter(|s| !s.is_empty());
            let ordinal = used.entry(base.clone()).or_insert(0);
            let suffix = by_slot.unwrap_or_else(|| ordinal.to_string());
            *ordinal += 1;
            format!("{base}@{suffix}")
        } else {
            base.clone()
        };
        let path = format!("{prefix}/{segment}");
        out.push((path.clone(), *child));
        walk_paths(store, *child, &path, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_with_ast;

    fn lowered(src: &str) -> (Store, Hash) {
        let (component, report) = check_with_ast(src, "t.mz");
        assert_eq!(report.error_count(), 0, "fixture must parse");
        let mut store = Store::new();
        let root = lower(&component.unwrap(), &mut store);
        (store, root)
    }

    const A: &str = "component a\n  prop x: bool\n  contract\n  end\nend component a\n";

    #[test]
    fn the_same_source_always_produces_the_same_root_hash() {
        let (_, h1) = lowered(A);
        let (_, h2) = lowered(A);
        assert_eq!(h1, h2);
    }

    #[test]
    fn meaningless_whitespace_does_not_change_identity() {
        // Canonical formatting means this should rarely arise, but identity must not depend
        // on it — otherwise every reformat invalidates every cached fact (RB-4, RB-6).
        let (_, spaced) =
            lowered("component a\n\n  prop x: bool\n\n  contract\n  end\nend component a\n");
        let (_, tight) = lowered(A);
        assert_eq!(spaced, tight);
    }

    #[test]
    fn a_real_change_changes_the_root_hash() {
        let (_, base) = lowered(A);
        let (_, changed) =
            lowered("component a\n  prop y: bool\n  contract\n  end\nend component a\n");
        assert_ne!(base, changed);
    }

    #[test]
    fn a_change_deep_in_the_tree_propagates_to_the_root() {
        // The Merkle property: this is what makes incremental compilation keyed on hashes
        // correct, and what makes "has anything changed" a single comparison.
        let (_, base) = lowered(
            "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n  end\nend component a\n",
        );
        let (_, changed) = lowered(
            "component a\n  enum e\n    one k \"2\"\n  end\n  contract\n  end\nend component a\n",
        );
        assert_ne!(base, changed, "a variant column change must reach the root");
    }

    #[test]
    fn identical_subtrees_are_stored_once() {
        // Structural sharing (RB-7). Two props with identical content are one node.
        let (store, _) = lowered(
            "component a\n  prop x: bool\n  prop x: bool\n  contract\n  end\nend component a\n",
        );
        let prop_nodes = store
            .nodes
            .values()
            .filter(|n| n.kind == "prop" && n.label.as_deref() == Some("x"));
        assert_eq!(prop_nodes.count(), 1, "duplicate props must share one node");
    }

    #[test]
    fn field_order_in_source_does_not_change_identity() {
        // Fields are sorted before hashing, so authoring order is not meaning.
        let mut a = Node::leaf("variant", Some("v"));
        a = a.field("label", "\"x\"").field("color", "\"y\"");
        let mut b = Node::leaf("variant", Some("v"));
        b = b.field("color", "\"y\"").field("label", "\"x\"");
        assert_eq!(a.hash(), b.hash());
    }

    #[test]
    fn child_order_does_change_identity() {
        // Children are a sequence, so their order is meaning and must be hashed.
        let x = Node::leaf("element", Some("x")).hash();
        let y = Node::leaf("element", Some("y")).hash();
        let xy = Node::leaf("view", None).child(x).child(y).hash();
        let yx = Node::leaf("view", None).child(y).child(x).hash();
        assert_ne!(xy, yx);
    }

    #[test]
    fn length_prefixing_stops_two_different_nodes_colliding() {
        // A delimiter-separated form would let field values containing the delimiter merge
        // into the same bytes. Length prefixes make that impossible.
        let a = Node::leaf("element", Some("x")).field("a", "b:c").hash();
        let b = Node::leaf("element", Some("x")).field("a:b", "c").hash();
        assert_ne!(a, b);
    }

    #[test]
    fn reordering_a_contract_does_not_change_identity() {
        // A contract is a set of assertions, not a sequence: saying the same four things in
        // a different order is the same promise. Clause children are therefore sorted
        // before hashing, unlike a view's children, whose order is meaning.
        let (_, a) = lowered(
            "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n    every e k not_empty\n    e.one k is \"1\"\n  end\nend component a\n",
        );
        let (_, b) = lowered(
            "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n    e.one k is \"1\"\n    every e k not_empty\n  end\nend component a\n",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn changing_an_assertion_changes_the_root_hash() {
        // The reason contracts participate in the hash at all (RFC-0006 §6): a component
        // that promises something different is a different component, and RB-6's
        // re-verification story is only sound if the hash covers the promise.
        let (_, base) = lowered(
            "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n    every e k not_empty\n  end\nend component a\n",
        );
        let (_, changed) = lowered(
            "component a\n  enum e\n    one k \"1\"\n  end\n  contract\n    every e k contains \"1\"\n  end\nend component a\n",
        );
        assert_ne!(base, changed);
    }

    #[test]
    fn a_contract_change_leaves_the_implementation_subtrees_untouched() {
        // What makes participating in the hash cheap: only the contract node and the root
        // move, so incremental compilation of the implementation is unaffected and it is
        // the assertions, not the code, that get re-run.
        let before =
            "component a\n  prop x: bool\n  contract\n    x is true\n  end\nend component a\n";
        let after =
            "component a\n  prop x: bool\n  contract\n    x is false\n  end\nend component a\n";
        let (store_a, root_a) = lowered(before);
        let (store_b, root_b) = lowered(after);
        assert_ne!(root_a, root_b);
        let prop_a = store_a
            .get(root_a)
            .unwrap()
            .children
            .iter()
            .find(|h| store_a.get(**h).map(|n| n.kind.as_str()) == Some("prop"))
            .copied();
        let prop_b = store_b
            .get(root_b)
            .unwrap()
            .children
            .iter()
            .find(|h| store_b.get(**h).map(|n| n.kind.as_str()) == Some("prop"))
            .copied();
        assert_eq!(prop_a, prop_b, "the prop node must keep its hash");
    }

    #[test]
    fn a_rename_touches_one_binding_and_no_node() {
        let (mut store, root) = lowered(A);
        let before = store.len();
        assert!(store.rename("a", "renamed"));
        assert_eq!(
            store.resolve("renamed"),
            Some(root),
            "the hash is unchanged"
        );
        assert_eq!(store.resolve("a"), None);
        assert_eq!(
            store.len(),
            before,
            "renaming must not create or alter a node"
        );
    }

    #[test]
    fn referrers_answers_who_points_at_this_without_a_search() {
        let (store, root) = lowered(A);
        let node = store.get(root).unwrap();
        let first_child = node.children[0];
        assert!(store.referrers(first_child).contains(&root));
    }

    #[test]
    fn structural_paths_are_readable_and_name_their_target() {
        let (store, root) = lowered(
            "component a\n  prop x: bool\n  fn go\n  end\n  contract\n  end\nend component a\n",
        );
        let all = paths(&store, root);
        let names: Vec<&str> = all.iter().map(|(p, _)| p.as_str()).collect();
        assert!(names.contains(&"a"), "the root is addressable: {names:?}");
        assert!(
            names.contains(&"a/prop:x"),
            "props are addressable: {names:?}"
        );
        assert!(names.contains(&"a/fn:go"), "fns are addressable: {names:?}");
    }

    #[test]
    fn sibling_elements_with_the_same_tag_get_distinct_paths() {
        // Found by the measured suite on the real `alert` primitive: two `row` children of
        // one parent collided, which would make a patch address ambiguous.
        let (store, root) = lowered(
            "component a\n  view\n    notice\n      row\n        slot = \"one\"\n      end\n      row\n        slot = \"two\"\n      end\n    end\n  end\n  contract\n  end\nend component a\n",
        );
        let all = paths(&store, root);
        let mut names: Vec<&str> = all.iter().map(|(p, _)| p.as_str()).collect();
        let before = names.len();
        names.sort();
        names.dedup();
        assert_eq!(before, names.len(), "paths must be unique: {names:?}");
        // And the disambiguator should be the readable slot, not a bare index.
        assert!(
            names.iter().any(|p| p.ends_with("element:row@one")),
            "expected a slot-disambiguated path: {names:?}"
        );
    }

    #[test]
    fn every_reachable_node_is_actually_in_the_store() {
        let (store, root) = lowered(A);
        for h in store.reachable(root) {
            assert!(store.get(h).is_some(), "dangling hash {}", h.short());
        }
    }
}
