use crate::trie::Entry;
use crate::trie::{MAX_FANOUT, OverflowResult};

/// A node in the Height Optimized Trie.
#[derive(Debug, Clone)]
pub struct HOTNode<K, V> {
    /// The height of the node in the trie.
    pub height: u16,
    /// The entries stored in this node.
    pub entries: Vec<Entry<K, V>>,
}

impl<K, V> HOTNode<K, V> {
    /// Creates a new HOT node with the specified height and pre-allocated capacity.
    pub fn new(height: u16) -> Self {
        Self {
            height,
            entries: Vec::with_capacity(MAX_FANOUT),
        }
    }

    /// Performs a lookup for the given key in the HOT subtree.
    pub fn lookup(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        if self.entries.is_empty() {
            return None;
        }

        let idx = match self.entries.binary_search_by(|e| e.key().cmp(key)) {
            Ok(found_idx) => found_idx,
            Err(insert_idx) => {
                if insert_idx > 0 {
                    insert_idx - 1
                } else {
                    return None;
                }
            }
        };

        match &self.entries[idx] {
            Entry::Leaf(k, v) => {
                if k == key {
                    Some(v)
                } else {
                    None
                }
            }
            Entry::Child(_, node) => node.lookup(key),
        }
    }

    /// Inserts a key-value pair into the HOT subtree.
    ///
    /// This implementation handles height optimization rules:
    /// 1. **Normal Case**: Insertion with space or Parent Pull Up.
    /// 2. **Leaf-Node Pushdown**: Pushing high-level leaves into subtrees.
    /// 3. **Intermediate Node Creation**: Filling height gaps during splits.
    pub fn insert(&mut self, key: K, value: V) -> OverflowResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        if self.entries.is_empty() {
            self.entries.push(Entry::Leaf(key, value));
            return OverflowResult::Ok;
        }

        let search_result = self.entries.binary_search_by(|e| e.key().cmp(&key));
        let mut split_to_handle = None;

        let result = match search_result {
            Ok(idx) => {
                match &mut self.entries[idx] {
                    Entry::Leaf(k, v) => {
                        if k == &key {
                            *v = value;
                            OverflowResult::Ok
                        } else {
                            self.handle_insert_at(idx, key, value)
                        }
                    }
                    Entry::Child(_, node) => {
                        let res = node.insert(key, value);
                        if let OverflowResult::Split(e1, e2) = res {
                            split_to_handle = Some((idx, e1, e2));
                            OverflowResult::Ok
                        } else {
                            OverflowResult::Ok
                        }
                    }
                }
            }
            Err(idx) => {
                if idx > 0 {
                    let prev_idx = idx - 1;
                    match &mut self.entries[prev_idx] {
                        Entry::Child(_, node) => {
                            let res = node.insert(key, value);
                            if let OverflowResult::Split(e1, e2) = res {
                                split_to_handle = Some((prev_idx, e1, e2));
                                OverflowResult::Ok
                            } else {
                                OverflowResult::Ok
                            }
                        }
                        Entry::Leaf(k, _) => {
                            if k == &key {
                                if let Entry::Leaf(_, v) = &mut self.entries[prev_idx] {
                                    *v = value;
                                }
                                OverflowResult::Ok
                            } else {
                                self.handle_insert_at(prev_idx, key, value)
                            }
                        }
                    }
                } else {
                    if self.entries.len() < MAX_FANOUT {
                        self.entries.insert(0, Entry::Leaf(key, value));
                        OverflowResult::Ok
                    } else {
                        self.entries.insert(0, Entry::Leaf(key, value));
                        return self.split();
                    }
                }
            }
        };

        if let Some((idx, e1, e2)) = split_to_handle {
            return self.integrate_child_split(idx, e1, e2);
        }

        result
    }

    fn integrate_child_split(
        &mut self,
        idx: usize,
        e1: Entry<K, V>,
        e2: Entry<K, V>,
    ) -> OverflowResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        let child_height = match &e1 {
            Entry::Child(_, node) => node.height,
            Entry::Leaf(_, _) => 0,
        };

        if self.height > child_height + 1 {
            let mut intermediate = HOTNode::new(child_height + 1);
            let rep_key = e1.key().clone();
            intermediate.entries.push(e1);
            intermediate.entries.push(e2);
            self.entries[idx] = Entry::Child(rep_key, Box::new(intermediate));
            OverflowResult::Ok
        } else {
            self.entries.remove(idx);
            self.entries.insert(idx, e1);
            self.entries.insert(idx + 1, e2);

            if self.entries.len() > MAX_FANOUT {
                self.split()
            } else {
                OverflowResult::Ok
            }
        }
    }

    fn split(&mut self) -> OverflowResult<K, V>
    where
        K: Clone,
    {
        let mid = self.entries.len() / 2;
        let right_entries = self.entries.split_off(mid);
        let left_entries = std::mem::take(&mut self.entries);

        let mut left_node = HOTNode::new(self.height);
        left_node.entries = left_entries;
        let left_rep = left_node.entries[0].key().clone();

        let mut right_node = HOTNode::new(self.height);
        right_node.entries = right_entries;
        let right_rep = right_node.entries[0].key().clone();

        OverflowResult::Split(
            Entry::Child(left_rep, Box::new(left_node)),
            Entry::Child(right_rep, Box::new(right_node)),
        )
    }

    fn handle_insert_at(&mut self, idx: usize, key: K, value: V) -> OverflowResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        let is_pushdown = self.height > 1;

        if is_pushdown {
            if let Entry::Leaf(old_k, old_v) = self.entries.remove(idx) {
                let mut new_node = HOTNode::new(self.height - 1);
                if key < old_k {
                    new_node.entries.push(Entry::Leaf(key.clone(), value));
                    new_node.entries.push(Entry::Leaf(old_k, old_v));
                } else {
                    new_node.entries.push(Entry::Leaf(old_k, old_v));
                    new_node.entries.push(Entry::Leaf(key.clone(), value));
                }

                let rep_key = new_node.entries[0].key().clone();
                self.entries.insert(idx, Entry::Child(rep_key, Box::new(new_node)));
            }
            OverflowResult::Ok
        } else {
            let insert_pos = match self.entries.binary_search_by(|e| e.key().cmp(&key)) {
                Ok(i) => i,
                Err(i) => i,
            };

            if insert_pos < self.entries.len() && self.entries[insert_pos].key() == &key {
                if let Entry::Leaf(_, v) = &mut self.entries[insert_pos] {
                    *v = value;
                }
            } else {
                self.entries.insert(insert_pos, Entry::Leaf(key, value));
            }

            if self.entries.len() > MAX_FANOUT {
                self.split()
            } else {
                OverflowResult::Ok
            }
        }
    }

    #[allow(dead_code)]
    pub fn to_dot_internal(&self, dot: &mut String)
    where
        K: std::fmt::Debug,
    {
        let node_id = self as *const _ as usize;
        dot.push_str(&format!(
            "  n{} [label=\"Height: {} | Entries: {}\", shape=record, style=filled, fillcolor=\"#f9f9f9\"];\n",
            node_id, self.height, self.entries.len()
        ));

        for (i, entry) in self.entries.iter().enumerate() {
            match entry {
                Entry::Leaf(k, _) => {
                    let leaf_id = format!("l_{}_{}", node_id, i);
                    dot.push_str(&format!(
                        "  {} [label=\"{:?}\", shape=circle, style=filled, fillcolor=\"#e1f5fe\"];\n",
                        leaf_id, k
                    ));
                    dot.push_str(&format!("  n{} -> {};\n", node_id, leaf_id));
                }
                Entry::Child(_, child) => {
                    child.to_dot_internal(dot);
                    dot.push_str(&format!(
                        "  n{} -> n{} [label=\"Rep: {:?}\", fontsize=10];\n",
                        node_id,
                        child.as_ref() as *const _ as usize,
                        entry.key()
                    ));
                }
            }
        }
    }
}
