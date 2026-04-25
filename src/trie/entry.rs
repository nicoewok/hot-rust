use crate::trie::HOTNode;

/// An entry in a HOT node, which can be either a leaf containing a key-value pair
/// or a child pointing to another HOT node.
#[derive(Debug, Clone)]
pub enum Entry<K, V> {
    Leaf(K, V, u32),
    Child(K, Box<HOTNode<K, V>>, u32),
}

impl<K, V> Entry<K, V> {
    /// Returns the representative key for this entry.
    /// For leaves, this is the actual key. For children, it's the stored minimum key.
    pub fn key(&self) -> &K {
        match self {
            Entry::Leaf(k, _, _) => k,
            Entry::Child(k, _, _) => k,
        }
    }

    /// Returns the partial key stored in this entry.
    pub fn partial_key(&self) -> u32 {
        match self {
            Entry::Leaf(_, _, pk) => *pk,
            Entry::Child(_, _, pk) => *pk,
        }
    }

    /// Sets the partial key for this entry.
    pub fn set_partial_key(&mut self, pk: u32) {
        match self {
            Entry::Leaf(_, _, p) => *p = pk,
            Entry::Child(_, _, p) => *p = pk,
        }
    }
}
