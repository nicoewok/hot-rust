use crate::trie::HOTNode;

/// An entry in a HOT node, which can be either a leaf containing a key-value pair
/// or a child pointing to another HOT node.
///
/// For `Child` entries, we store a representative key (the minimum key in its subtree)
/// to enable efficient binary searching within the `entries` vector.
#[derive(Debug, Clone)]
pub enum Entry<K, V> {
    Leaf(K, V),
    Child(K, Box<HOTNode<K, V>>),
}

impl<K, V> Entry<K, V> {
    /// Returns the representative key for this entry.
    /// For leaves, this is the actual key. For children, it's the stored minimum key.
    pub fn key(&self) -> &K {
        match self {
            Entry::Leaf(k, _) => k,
            Entry::Child(k, _) => k,
        }
    }
}
