//! Height Optimized Trie (HOT) implementation in Rust.

use crate::trie::{Entry, HOTNode};

/// The maximum fanout for a HOT node.
pub const MAX_FANOUT: usize = 32;

/// The result of an insertion operation, which may result in a node split.
#[derive(Debug, Clone)]
pub enum OverflowResult<K, V> {
    /// Insertion succeeded without overflowing the current node.
    Ok,
    /// The node overflowed and was split into two new entries.
    Split(Entry<K, V>, Entry<K, V>),
}

/// A wrapper for the Height Optimized Trie to manage the root and height increases.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HOT<K, V> {
    pub root: Option<HOTNode<K, V>>,
}

impl<K, V> HOT<K, V>
where
    K: Ord + Clone,
    V: Clone,
{
    /// Creates a new empty HOT.
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Inserts a key-value pair into the trie.
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(root) = &mut self.root {
            match root.insert(key, value) {
                OverflowResult::Ok => {}
                OverflowResult::Split(e1, e2) => {
                    // Rule 3: Tree height increases only when the root overflows.
                    let mut new_root = HOTNode::new(root.height + 1);
                    new_root.entries.push(e1);
                    new_root.entries.push(e2);
                    self.root = Some(new_root);
                }
            }
        } else {
            let mut root = HOTNode::new(1);
            root.insert(key, value);
            self.root = Some(root);
        }
    }

    /// Looks up a key in the trie.
    pub fn lookup(&self, key: &K) -> Option<&V> {
        self.root.as_ref()?.lookup(key)
    }

    /// Generates a Graphviz DOT representation of the trie.
    #[allow(dead_code)]
    pub fn to_dot(&self) -> String
    where
        K: std::fmt::Debug,
    {
        let mut dot = String::from("digraph G {\n  rankdir=TD;\n  node [fontname=\"Arial\"];\n");
        if let Some(root) = &self.root {
            root.to_dot_internal(&mut dot);
        }
        dot.push_str("}\n");
        dot
    }
}
