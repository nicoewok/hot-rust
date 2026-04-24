use crate::trie::Entry;
use crate::trie::{InsertResult, RemoveResult};
use std::sync::atomic::{AtomicU64, Ordering};

static NODE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A node in the Height Optimized Trie.
#[derive(Debug, Clone)]
pub struct HOTNode<K, V> {
    /// Unique ID for this node.
    pub id: u64,
    /// The height of the node in the trie.
    pub height: u16,
    /// The entries stored in this node.
    pub entries: Vec<Entry<K, V>>,
}

impl<K, V> HOTNode<K, V> {
    /// Creates a new HOT node with the specified height and pre-allocated capacity.
    pub fn new(height: u16, fanout: usize) -> Self {
        Self {
            id: NODE_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            height,
            entries: Vec::with_capacity(fanout),
        }
    }

    /// Performs a lookup for the given key in the HOT subtree.
    #[allow(dead_code)]
    pub fn lookup(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        self.lookup_recursive(key)
    }

    #[allow(dead_code)]
    fn lookup_recursive(&self, key: &K) -> Option<&V>
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
            Entry::Child(_, node) => node.lookup_recursive(key),
        }
    }

    /// Performs a lookup and returns the path of nodes visited.
    pub fn lookup_with_path(&self, key: &K, path: &mut Vec<u64>) -> Option<&V>
    where
        K: Ord,
    {
        path.push(self.id);
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
                    path.push(k as *const _ as u64);
                    Some(v)
                } else {
                    None
                }
            }
            Entry::Child(_, node) => node.lookup_with_path(key, path),
        }
    }

    /// Removes a node or leaf from the trie if its memory address matches target_id.
    pub fn remove_by_id(&mut self, target_id: u64) -> RemoveResult {
        let mut to_remove = None;
        for (i, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                Entry::Child(_, node) => {
                    if node.id == target_id {
                        to_remove = Some(i);
                        break;
                    }
                    match node.remove_by_id(target_id) {
                        RemoveResult::NotFound => {}
                        RemoveResult::Removed => return RemoveResult::Removed,
                        RemoveResult::Empty => {
                            to_remove = Some(i);
                            break;
                        }
                    }
                }
                Entry::Leaf(k, _) => {
                    if (k as *const _ as u64) == target_id {
                        to_remove = Some(i);
                        break;
                    }
                }
            }
        }

        if let Some(idx) = to_remove {
            self.entries.remove(idx);
            if self.entries.is_empty() {
                return RemoveResult::Empty;
            } else {
                return RemoveResult::Removed;
            }
        }
        RemoveResult::NotFound
    }

    /// Inserts a key-value pair into the HOT subtree.
    ///
    /// This implementation handles height optimization rules:
    /// 1. **Normal Case**: Insertion with space or Parent Pull Up.
    /// 2. **Leaf-Node Pushdown**: Pushing high-level leaves into subtrees.
    /// 3. **Intermediate Node Creation**: Filling height gaps during splits.
    pub fn insert(&mut self, key: K, value: V, fanout: usize) -> InsertResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        if self.entries.is_empty() {
            self.entries.push(Entry::Leaf(key.clone(), value));
            return InsertResult::NewMin(key);
        }

        let search_result = self.entries.binary_search_by(|e| e.key().cmp(&key));
        let mut split_to_handle = None;

        let result = match search_result {
            Ok(idx) => {
                match &mut self.entries[idx] {
                    Entry::Leaf(k, v) => {
                        if k == &key {
                            *v = value;
                            InsertResult::Ok
                        } else {
                            self.handle_insert_at(idx, key, value, fanout)
                        }
                    }
                    Entry::Child(rep, node) => {
                        let res = node.insert(key, value, fanout);
                        match res {
                            InsertResult::NewMin(new_k) => {
                                *rep = new_k.clone();
                                if idx == 0 {
                                    InsertResult::NewMin(new_k)
                                } else {
                                    InsertResult::Ok
                                }
                            }
                            InsertResult::Split(e1, e2) => {
                                split_to_handle = Some((idx, e1, e2));
                                InsertResult::Ok
                            }
                            InsertResult::Ok => InsertResult::Ok,
                        }
                    }
                }
            }
            Err(idx) => {
                if idx > 0 {
                    let prev_idx = idx - 1;
                    match &mut self.entries[prev_idx] {
                        Entry::Child(rep, node) => {
                            let res = node.insert(key, value, fanout);
                            match res {
                                InsertResult::NewMin(new_k) => {
                                    *rep = new_k.clone();
                                    if prev_idx == 0 {
                                        InsertResult::NewMin(new_k)
                                    } else {
                                        InsertResult::Ok
                                    }
                                }
                                InsertResult::Split(e1, e2) => {
                                    split_to_handle = Some((prev_idx, e1, e2));
                                    InsertResult::Ok
                                }
                                InsertResult::Ok => InsertResult::Ok,
                            }
                        }
                        Entry::Leaf(k, _) => {
                            if k == &key {
                                if let Entry::Leaf(_, v) = &mut self.entries[prev_idx] {
                                    *v = value;
                                }
                                InsertResult::Ok
                            } else {
                                self.handle_insert_at(prev_idx, key, value, fanout)
                            }
                        }
                    }
                } else {
                    if self.entries.len() < fanout {
                        self.entries.insert(0, Entry::Leaf(key.clone(), value));
                        InsertResult::NewMin(key)
                    } else {
                        self.entries.insert(0, Entry::Leaf(key, value));
                        self.split()
                    }
                }
            }
        };

        if let Some((idx, e1, e2)) = split_to_handle {
            return self.integrate_child_split(idx, e1, e2, fanout);
        }

        result
    }

    fn integrate_child_split(
        &mut self,
        idx: usize,
        e1: Entry<K, V>,
        e2: Entry<K, V>,
        fanout: usize,
    ) -> InsertResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        let child_height = match &e1 {
            Entry::Child(_, node) => node.height,
            Entry::Leaf(_, _) => 0,
        };

        if self.height > child_height + 1 {
            let mut intermediate = HOTNode::new(child_height + 1, fanout);
            let rep_key = e1.key().clone();
            intermediate.entries.push(e1);
            intermediate.entries.push(e2);
            let is_idx_0 = idx == 0;
            self.entries[idx] = Entry::Child(rep_key.clone(), Box::new(intermediate));
            if is_idx_0 {
                InsertResult::NewMin(rep_key)
            } else {
                InsertResult::Ok
            }
        } else {
            let is_idx_0 = idx == 0;
            self.entries.remove(idx);
            self.entries.insert(idx, e1);
            let e1_key = self.entries[idx].key().clone();
            self.entries.insert(idx + 1, e2);

            if self.entries.len() > fanout {
                self.split()
            } else if is_idx_0 {
                InsertResult::NewMin(e1_key)
            } else {
                InsertResult::Ok
            }
        }
    }

    fn split(&mut self) -> InsertResult<K, V>
    where
        K: Clone,
    {
        let mid = self.entries.len() / 2;
        let right_entries = self.entries.split_off(mid);
        let left_entries = std::mem::take(&mut self.entries);

        let mut left_node = HOTNode::new(self.height, self.entries.len());
        left_node.entries = left_entries;
        let left_rep = left_node.entries[0].key().clone();

        let mut right_node = HOTNode::new(self.height, right_entries.len());
        right_node.entries = right_entries;
        let right_rep = right_node.entries[0].key().clone();

        InsertResult::Split(
            Entry::Child(left_rep, Box::new(left_node)),
            Entry::Child(right_rep, Box::new(right_node)),
        )
    }

    fn handle_insert_at(&mut self, idx: usize, key: K, value: V, fanout: usize) -> InsertResult<K, V>
    where
        K: Ord + Clone,
        V: Clone,
    {
        let is_pushdown = self.height > 1;

        if is_pushdown {
            if let Entry::Leaf(old_k, old_v) = self.entries.remove(idx) {
                let mut new_node = HOTNode::new(self.height - 1, 2);
                if key < old_k {
                    new_node.entries.push(Entry::Leaf(key.clone(), value));
                    new_node.entries.push(Entry::Leaf(old_k, old_v));
                } else {
                    new_node.entries.push(Entry::Leaf(old_k, old_v));
                    new_node.entries.push(Entry::Leaf(key.clone(), value));
                }

                let rep_key = new_node.entries[0].key().clone();
                self.entries.insert(idx, Entry::Child(rep_key.clone(), Box::new(new_node)));
                if idx == 0 {
                    return InsertResult::NewMin(rep_key);
                }
            }
            InsertResult::Ok
        } else {
            let insert_pos = match self.entries.binary_search_by(|e| e.key().cmp(&key)) {
                Ok(i) => i,
                Err(i) => i,
            };

            if insert_pos < self.entries.len() && self.entries[insert_pos].key() == &key {
                if let Entry::Leaf(_, v) = &mut self.entries[insert_pos] {
                    *v = value;
                    return InsertResult::Ok;
                }
            }

            self.entries.insert(insert_pos, Entry::Leaf(key.clone(), value));

            if self.entries.len() > fanout {
                self.split()
            } else if insert_pos == 0 {
                InsertResult::NewMin(key)
            } else {
                InsertResult::Ok
            }
        }
    }

    #[allow(dead_code)]
    pub fn to_dot_internal(&self, dot: &mut String)
    where
        K: std::fmt::Debug,
    {
        let node_id = self.id;
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
                        child.id,
                        entry.key()
                    ));
                }
            }
        }
    }
}
