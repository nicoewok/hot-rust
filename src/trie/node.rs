use crate::trie::Entry;
use crate::trie::{InsertResult, RemoveResult};
use std::collections::HashSet;
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
    /// The bit-positions that distinguish the keys in this node.
    pub mask: Vec<usize>,
}

/// Trait for keys that can be used in a HOT node, allowing bit-level access.
pub trait HotKey {
    /// Returns the bit at the specified position.
    fn get_bit(&self, pos: usize) -> bool;
    /// Returns the first bit position where two keys differ.
    fn first_differing_bit(&self, other: &Self) -> Option<usize>;
}

/// Helper function to find the first bit position where two strings differ.
pub fn find_first_diff_bit(a: &str, b: &str) -> Option<usize> {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let min_len = a_bytes.len().min(b_bytes.len());

    for i in 0..min_len {
        if a_bytes[i] != b_bytes[i] {
            let diff = a_bytes[i] ^ b_bytes[i];
            return Some(i * 8 + diff.leading_zeros() as usize);
        }
    }

    if a_bytes.len() != b_bytes.len() {
        // Return the first bit of the extra byte to distinguish prefix from longer string.
        return Some(min_len * 8);
    }

    None
}

impl HotKey for String {
    fn get_bit(&self, pos: usize) -> bool {
        let byte_idx = pos / 8;
        let bit_idx = (pos % 8) as u8;
        let bytes = self.as_bytes();
        if byte_idx < bytes.len() {
            (bytes[byte_idx] & (1 << (7 - bit_idx))) != 0
        } else {
            false
        }
    }

    fn first_differing_bit(&self, other: &Self) -> Option<usize> {
        find_first_diff_bit(self, other)
    }
}

impl HotKey for u64 {
    fn get_bit(&self, pos: usize) -> bool {
        if pos < 64 {
            (self & (1 << (63 - pos))) != 0
        } else {
            false
        }
    }

    fn first_differing_bit(&self, other: &Self) -> Option<usize> {
        let diff = self ^ other;
        if diff == 0 {
            None
        } else {
            Some(diff.leading_zeros() as usize)
        }
    }
}

impl HotKey for u32 {
    fn get_bit(&self, pos: usize) -> bool {
        if pos < 32 {
            (self & (1 << (31 - pos))) != 0
        } else {
            false
        }
    }

    fn first_differing_bit(&self, other: &Self) -> Option<usize> {
        let diff = self ^ other;
        if diff == 0 {
            None
        } else {
            Some(diff.leading_zeros() as usize)
        }
    }
}

impl<K, V> HOTNode<K, V> {
    /// Creates a new HOT node with the specified height and pre-allocated capacity.
    pub fn new(height: u16, fanout: usize) -> Self {
        Self {
            id: NODE_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            height,
            entries: Vec::with_capacity(fanout),
            mask: Vec::new(),
        }
    }

    /// Performs a lookup for the given key in the HOT subtree.
    #[allow(dead_code)]
    pub fn lookup(&self, key: &K) -> Option<&V>
    where
        K: Ord + HotKey,
    {
        self.lookup_recursive(key)
    }

    #[allow(dead_code)]
    fn lookup_recursive(&self, key: &K) -> Option<&V>
    where
        K: Ord + HotKey,
    {
        if self.entries.is_empty() {
            return None;
        }

        let search_pk = self.extract_partial_key(key);

        for entry in &self.entries {
            if entry.partial_key() == search_pk {
                match entry {
                    Entry::Leaf(k, v, _) => {
                        if k == key {
                            return Some(v);
                        }
                    }
                    Entry::Child(_, node, _) => {
                        if let Some(v) = node.lookup_recursive(key) {
                            return Some(v);
                        }
                    }
                }
            }
        }
        None
    }

    /// Performs a lookup and returns the path of nodes visited.
    pub fn lookup_with_path(&self, key: &K, path: &mut Vec<u64>) -> Option<&V>
    where
        K: Ord + HotKey,
    {
        path.push(self.id);
        if self.entries.is_empty() {
            return None;
        }

        let search_pk = self.extract_partial_key(key);

        for entry in &self.entries {
            if entry.partial_key() == search_pk {
                match entry {
                    Entry::Leaf(k, v, _) => {
                        if k == key {
                            path.push(k as *const _ as u64);
                            return Some(v);
                        }
                    }
                    Entry::Child(_, node, _) => {
                        if let Some(v) = node.lookup_with_path(key, path) {
                            return Some(v);
                        }
                    }
                }
            }
        }
        None
    }

    /// Removes a node or leaf from the trie if its memory address matches target_id.
    pub fn remove_by_id(&mut self, target_id: u64) -> RemoveResult {
        let mut to_remove = None;
        for (i, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                Entry::Child(_, node, _) => {
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
                Entry::Leaf(k, _, _) => {
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
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        if self.entries.is_empty() {
            let pk = 0; // Empty mask means partial key is always 0
            self.entries.push(Entry::Leaf(key.clone(), value, pk));
            return InsertResult::NewMin(key);
        }

        // --- Bit-Aware Insertion Logic ---
        // 1. Convert keys to bit-representations (handled by HotKey trait)
        // 2. Find the first bit index where the new key differs from existing keys
        let mut min_diff_bit = None;
        for entry in &self.entries {
            if let Some(diff) = key.first_differing_bit(entry.key()) {
                if min_diff_bit.is_none() || diff < min_diff_bit.unwrap() {
                    min_diff_bit = Some(diff);
                }
            }
        }

        // 3. If that bit index isn't in self.mask, add it and sort the mask
        if let Some(diff_bit) = min_diff_bit {
            if !self.mask.contains(&diff_bit) {
                self.mask.push(diff_bit);
                self.mask.sort();
                // 4. Recalculate ALL existing entries' partial_keys
                self.refresh_partial_keys();
            }
        }
        // ---------------------------------

        let search_result = self.entries.binary_search_by(|e| e.key().cmp(&key));
        let mut split_to_handle = None;

        let result = match search_result {
            Ok(idx) => {
                match &mut self.entries[idx] {
                    Entry::Leaf(k, v, _) => {
                        if k == &key {
                            *v = value;
                            InsertResult::Ok
                        } else {
                            self.handle_insert_at(idx, key, value, fanout)
                        }
                    }
                    Entry::Child(rep, node, pk) => {
                        let res = node.insert(key, value, fanout);
                        match res {
                            InsertResult::NewMin(new_k) => {
                                *rep = new_k.clone();
                                *pk = Self::extract_partial_key_static(&self.mask, &new_k);
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
                        Entry::Child(rep, node, pk) => {
                            let res = node.insert(key, value, fanout);
                            match res {
                                InsertResult::NewMin(new_k) => {
                                    *rep = new_k.clone();
                                    *pk = Self::extract_partial_key_static(&self.mask, &new_k);
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
                        Entry::Leaf(k, _, _) => {
                            if k == &key {
                                if let Entry::Leaf(_, v, _) = &mut self.entries[prev_idx] {
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
                        let pk = self.extract_partial_key(&key);
                        self.entries.insert(0, Entry::Leaf(key.clone(), value, pk));
                        self.update_mask_from_entries();
                        InsertResult::NewMin(key)
                    } else {
                        let pk = self.extract_partial_key(&key);
                        self.entries.insert(0, Entry::Leaf(key, value, pk));
                        self.update_mask_from_entries();
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
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        let child_height = match &e1 {
            Entry::Child(_, node, _) => node.height,
            Entry::Leaf(_, _, _) => 0,
        };

        if self.height > child_height + 1 {
            let mut intermediate = HOTNode::new(child_height + 1, fanout);
            let rep_key = e1.key().clone();
            intermediate.entries.push(e1);
            intermediate.entries.push(e2);
            intermediate.update_mask_from_entries();
            let is_idx_0 = idx == 0;
            let pk = self.extract_partial_key(&rep_key);
            self.entries[idx] = Entry::Child(rep_key.clone(), Box::new(intermediate), pk);
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
            self.update_mask_from_entries();

            if self.entries.len() > fanout || self.mask.len() > 32 {
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
        K: Clone + HotKey,
    {
        let mid = self.entries.len() / 2;
        let right_entries = self.entries.split_off(mid);
        let left_entries = std::mem::take(&mut self.entries);

        let mut left_node = HOTNode::new(self.height, self.entries.len());
        left_node.entries = left_entries;
        left_node.update_mask_from_entries();
        let left_rep = left_node.entries[0].key().clone();
        let left_pk = self.extract_partial_key(&left_rep);

        let mut right_node = HOTNode::new(self.height, right_entries.len());
        right_node.entries = right_entries;
        right_node.update_mask_from_entries();
        let right_rep = right_node.entries[0].key().clone();
        let right_pk = self.extract_partial_key(&right_rep);

        InsertResult::Split(
            Entry::Child(left_rep, Box::new(left_node), left_pk),
            Entry::Child(right_rep, Box::new(right_node), right_pk),
        )
    }

    fn handle_insert_at(&mut self, idx: usize, key: K, value: V, fanout: usize) -> InsertResult<K, V>
    where
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        let is_pushdown = self.height > 1;

        if is_pushdown {
            if let Entry::Leaf(old_k, old_v, _) = self.entries.remove(idx) {
                let mut new_node = HOTNode::new(self.height - 1, 2);
                let k1_pk = new_node.extract_partial_key(&key);
                let k2_pk = new_node.extract_partial_key(&old_k);
                
                if key < old_k {
                    new_node.entries.push(Entry::Leaf(key.clone(), value, k1_pk));
                    new_node.entries.push(Entry::Leaf(old_k, old_v, k2_pk));
                } else {
                    new_node.entries.push(Entry::Leaf(old_k, old_v, k2_pk));
                    new_node.entries.push(Entry::Leaf(key.clone(), value, k1_pk));
                }

                let rep_key = new_node.entries[0].key().clone();
                let pk = self.extract_partial_key(&rep_key);
                self.entries.insert(idx, Entry::Child(rep_key.clone(), Box::new(new_node), pk));
                self.update_mask_from_entries();
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
                if let Entry::Leaf(_, v, _) = &mut self.entries[insert_pos] {
                    *v = value;
                    return InsertResult::Ok;
                }
            }

            let pk = self.extract_partial_key(&key);
            self.entries.insert(insert_pos, Entry::Leaf(key.clone(), value, pk));
            self.update_mask_from_entries();

            if self.entries.len() > fanout || self.mask.len() > 32 {
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
                Entry::Leaf(k, _, _) => {
                    let leaf_id = format!("l_{}_{}", node_id, i);
                    dot.push_str(&format!(
                        "  {} [label=\"{:?}\", shape=circle, style=filled, fillcolor=\"#e1f5fe\"];\n",
                        leaf_id, k
                    ));
                    dot.push_str(&format!("  n{} -> {};\n", node_id, leaf_id));
                }
                Entry::Child(_, child, _) => {
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

    /// Recalculates and updates the partial keys for all entries in this node.
    pub fn refresh_partial_keys(&mut self)
    where
        K: HotKey,
    {
        let mask = &self.mask;
        for entry in self.entries.iter_mut() {
            let pk = Self::extract_partial_key_static(mask, entry.key());
            match entry {
                Entry::Leaf(_, _, p) => *p = pk,
                Entry::Child(_, _, p) => *p = pk,
            }
        }
    }

    /// Extracts a partial key using a specific discriminative mask.
    pub fn extract_partial_key_static(mask: &[usize], key: &K) -> u32
    where
        K: HotKey,
    {
        let mut partial_key = 0u32;
        for (i, &bit_pos) in mask.iter().enumerate() {
            if i >= 32 {
                break;
            }
            if key.get_bit(bit_pos) {
                partial_key |= 1 << (mask.len().min(32) - 1 - i);
            }
        }
        partial_key
    }

    /// Extracts a partial key from the given key using the node's discriminative mask.
    pub fn extract_partial_key(&self, key: &K) -> u32
    where
        K: HotKey,
    {
        Self::extract_partial_key_static(&self.mask, key)
    }

    /// Automatically updates the discriminative mask based on the current entries in the node.
    /// This should be called whenever the set of representative keys changes.
    pub fn update_mask_from_entries(&mut self)
    where
        K: HotKey,
    {
        if self.entries.len() < 2 {
            self.mask.clear();
            self.refresh_partial_keys();
            return;
        }

        let mut new_mask = HashSet::new();
        for i in 0..self.entries.len() - 1 {
            if let Some(pos) = self.entries[i].key().first_differing_bit(self.entries[i + 1].key()) {
                new_mask.insert(pos);
            }
        }

        let mut mask_vec: Vec<usize> = new_mask.into_iter().collect();
        mask_vec.sort();

        if self.mask != mask_vec {
            self.mask = mask_vec;
            self.refresh_partial_keys();
        }
    }
}
