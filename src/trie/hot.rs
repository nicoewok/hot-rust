use crate::trie::{Entry, HOTNode};

#[derive(Debug, Clone)]
pub enum InsertResult<K, V> {
    Ok,
    NewMin(K),
    Split(Entry<K, V>, Entry<K, V>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RemoveResult {
    NotFound,
    Removed,
    Empty,
}

/// A wrapper for the Height Optimized Trie to manage the root and height increases.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HOT<K, V> {
    pub root: Option<HOTNode<K, V>>,
    pub fanout: usize,
}

impl<K, V> HOT<K, V>
where
    K: Ord + Clone + crate::trie::node::HotKey,
    V: Clone,
{
    /// Creates a new empty HOT with specified fanout.
    pub fn new(fanout: usize) -> Self {
        Self { root: None, fanout }
    }

    /// Inserts a key-value pair into the trie.
    pub fn insert(&mut self, key: K, value: V) {
        if let Some(root) = &mut self.root {
            match root.insert(key, value, self.fanout) {
                InsertResult::Ok | InsertResult::NewMin(_) => {}
                InsertResult::Split(e1, e2) => {
                    // Rule 3: Tree height increases only when the root overflows.
                    let mut new_root = HOTNode::new(root.height + 1, self.fanout);
                    new_root.entries.push(e1);
                    new_root.entries.push(e2);
                    new_root.update_mask_from_entries();
                    self.root = Some(new_root);
                }
            }
        } else {
            let mut root = HOTNode::new(1, self.fanout);
            root.insert(key, value, self.fanout);
            self.root = Some(root);
        }
    }

    /// Looks up a key in the trie.
    #[allow(dead_code)]
    pub fn lookup(&self, key: &K) -> Option<&V> {
        self.root.as_ref()?.lookup(key)
    }

    /// Looks up a key and returns the path of nodes visited.
    pub fn lookup_with_path(&self, key: &K) -> (Option<&V>, Vec<u64>) {
        let mut path = Vec::new();
        let val = if let Some(root) = &self.root {
            root.lookup_with_path(key, &mut path)
        } else {
            None
        };
        (val, path)
    }

    /// Removes a node by its ID.
    pub fn remove_by_id(&mut self, target_id: u64) -> bool {
        if let Some(root) = &mut self.root {
            if root.id == target_id {
                self.root = None;
                return true;
            }
            return match root.remove_by_id(target_id) {
                RemoveResult::NotFound => false,
                RemoveResult::Removed => true,
                RemoveResult::Empty => {
                    self.root = None;
                    true
                }
            };
        }
        false
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
