use crate::trie::{Entry, SearchStep};
use crate::trie::{InsertResult, RemoveResult};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

static NODE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::_pext_u64;

/// Software fallback for PEXT when BMI2 is not available.
fn pext_soft(val: u64, mut mask: u64) -> u64 {
    let mut res = 0;
    let mut bit = 1;
    while mask != 0 {
        let lowest = mask & mask.wrapping_neg();
        if (val & lowest) != 0 {
            res |= bit;
        }
        bit <<= 1;
        mask ^= lowest;
    }
    res
}

/// Hardware accelerated PEXT with software fallback.
pub fn pext(val: u64, mask: u64) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        // BMI2 check
        if std::is_x86_feature_detected!("bmi2") {
            return unsafe { _pext_u64(val, mask) };
        }
    }
    pext_soft(val, mask)
}

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
    /// The byte offset for the 8-byte chunk used for partial key extraction.
    pub byte_offset: usize,
}

/// Trait for keys that can be used in a HOT node, allowing bit-level access.
pub trait HotKey {
    /// Returns the bit at the specified position.
    fn get_bit(&self, pos: usize) -> bool;
    /// Returns the first bit position where two keys differ.
    fn first_differing_bit(&self, other: &Self) -> Option<usize>;
    /// Returns 8 bytes from the key starting at the given byte offset as a big-endian u64.
    fn get_u64_at(&self, byte_offset: usize) -> u64;
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

    fn get_u64_at(&self, byte_offset: usize) -> u64 {
        let bytes = self.as_bytes();
        let mut val = 0u64;
        for i in 0..8 {
            if byte_offset + i < bytes.len() {
                val |= (bytes[byte_offset + i] as u64) << (8 * (7 - i));
            }
        }
        val
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

    fn get_u64_at(&self, _byte_offset: usize) -> u64 {
        *self
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

    fn get_u64_at(&self, _byte_offset: usize) -> u64 {
        (*self as u64) << 32
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
            byte_offset: 0,
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

    /// Performs a full search following the HOT algorithm with detailed path info.
    pub fn search(
        &self,
        key: &K,
        path: &mut Vec<u64>,
        edges: &mut Vec<(u64, u64)>,
        steps: &mut Vec<SearchStep>,
    ) -> (Option<u64>, bool)
    where
        K: Ord + HotKey,
    {
        path.push(self.id);
        if self.entries.is_empty() {
            return (None, false);
        }

        let search_pk = self.extract_partial_key(key);
        
        let mut step = SearchStep {
            node_id: self.id,
            partial_key: search_pk,
            mask: self.mask.clone(),
            byte_offset: self.byte_offset,
            matched_entry_id: None,
        };

        for entry in &self.entries {
            if entry.partial_key() == search_pk {
                match entry {
                    Entry::Leaf(k, _, _) => {
                        let leaf_id = k as *const _ as u64;
                        step.matched_entry_id = Some(leaf_id);
                        steps.push(step);
                        edges.push((self.id, leaf_id));
                        return (Some(leaf_id), k == key);
                    }
                    Entry::Child(_, node, _) => {
                        step.matched_entry_id = Some(node.id);
                        steps.push(step);
                        edges.push((self.id, node.id));
                        return node.search(key, path, edges, steps);
                    }
                }
            }
        }
        steps.push(step);
        (None, false)
    }

    /// Removes a node or leaf from the trie if its memory address matches target_id.
    pub fn remove_by_id(&mut self, target_id: u64) -> RemoveResult<K, V>
    where
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        let mut to_remove = None;
        let mut underflow_entry = None;
        let child_updated = false;
        let mask = self.mask.clone();

        for (i, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                Entry::Child(rep, node, pk) => {
                    if node.id == target_id {
                        to_remove = Some(i);
                        break;
                    }
                    match node.remove_by_id(target_id) {
                        RemoveResult::NotFound => {}
                        RemoveResult::Removed(ids) => {
                            let new_rep = node.entries[0].key().clone();
                            if &new_rep != rep {
                                *rep = new_rep.clone();
                                *pk = Self::extract_partial_key_static(&mask, mask.first().map(|&b| b / 8).unwrap_or(0), &new_rep);
                            }
                            return RemoveResult::Removed(ids);
                        }
                        RemoveResult::Empty => {
                            to_remove = Some(i);
                            break;
                        }
                        RemoveResult::Underflow(child_entry, collapsed) => {
                            underflow_entry = Some((i, child_entry, collapsed));
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

        self.handle_removal_post_process(to_remove, underflow_entry, child_updated)
    }

    /// Performs a removal by key with underflow handling.
    pub fn remove(&mut self, key: &K) -> RemoveResult<K, V>
    where
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        let mut to_remove = None;
        let mut underflow_entry = None;
        let child_updated = false;
        let mask = self.mask.clone();

        let search_pk = self.extract_partial_key(key);

        for (i, entry) in self.entries.iter_mut().enumerate() {
            if entry.partial_key() == search_pk {
                match entry {
                    Entry::Leaf(k, _, _) => {
                        if k == key {
                            to_remove = Some(i);
                            break;
                        }
                    }
                    Entry::Child(rep, node, pk) => {
                        match node.remove(key) {
                            RemoveResult::NotFound => return RemoveResult::NotFound,
                            RemoveResult::Removed(ids) => {
                                let new_rep = node.entries[0].key().clone();
                                if &new_rep != rep {
                                    *rep = new_rep.clone();
                                    *pk = Self::extract_partial_key_static(&mask, mask.first().map(|&b| b / 8).unwrap_or(0), &new_rep);
                                }
                                return RemoveResult::Removed(ids);
                            }
                            RemoveResult::Empty => {
                                to_remove = Some(i);
                                break;
                            }
                            RemoveResult::Underflow(child_entry, collapsed) => {
                                underflow_entry = Some((i, child_entry, collapsed));
                                break;
                            }
                        }
                    }
                }
            }
        }

        self.handle_removal_post_process(to_remove, underflow_entry, child_updated)
    }

    fn handle_removal_post_process(
        &mut self,
        to_remove: Option<usize>,
        underflow_entry: Option<(usize, Entry<K, V>, Vec<u64>)>,
        child_updated: bool,
    ) -> RemoveResult<K, V>
    where
        K: Ord + Clone + HotKey,
        V: Clone,
    {
        if let Some(idx) = to_remove {
            self.entries.remove(idx);
            self.update_mask_from_entries();
            if self.entries.is_empty() {
                return RemoveResult::Empty;
            } else if self.entries.len() == 1 {
                return RemoveResult::Underflow(self.entries.remove(0), vec![self.id]);
            } else {
                return RemoveResult::Removed(vec![self.id]);
            }
        }

        if let Some((idx, mut child_entry, mut collapsed)) = underflow_entry {
            let new_pk = self.extract_partial_key(child_entry.key());
            child_entry.set_partial_key(new_pk);
            self.entries[idx] = child_entry;
            self.update_mask_from_entries();
            if self.entries.len() == 1 {
                collapsed.push(self.id);
                return RemoveResult::Underflow(self.entries.remove(0), collapsed);
            } else {
                return RemoveResult::Removed(collapsed);
            }
        }

        if child_updated {
            return RemoveResult::Removed(vec![self.id]);
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
                                *pk = Self::extract_partial_key_static(&self.mask, self.byte_offset, &new_k);
                                self.update_mask_from_entries();
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
                                    *pk = Self::extract_partial_key_static(&self.mask, self.byte_offset, &new_k);
                                    self.update_mask_from_entries();
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
        // Bit-Aware Splitting: Find the most significant bit (smallest bit index)
        // that distinguishes any pair of adjacent representative keys.
        let mut best_bit = None;
        let mut split_idx = self.entries.len() / 2;

        for i in 0..self.entries.len() - 1 {
            if let Some(pos) = self.entries[i].key().first_differing_bit(self.entries[i + 1].key()) {
                if best_bit.is_none() || pos < best_bit.unwrap() {
                    best_bit = Some(pos);
                    split_idx = i + 1;
                }
            }
        }

        let right_entries = self.entries.split_off(split_idx);
        let left_entries = std::mem::take(&mut self.entries);

        let mut left_node = HOTNode::new(self.height, self.entries.len());
        left_node.entries = left_entries;
        left_node.update_mask_from_entries();
        let left_rep = left_node.entries[0].key().clone();
        let left_pk = 0; // Temporary, will be updated by parent

        let mut right_node = HOTNode::new(self.height, right_entries.len());
        right_node.entries = right_entries;
        right_node.update_mask_from_entries();
        let right_rep = right_node.entries[0].key().clone();
        let right_pk = 0; // Temporary, will be updated by parent

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

                new_node.update_mask_from_entries();

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
        let byte_offset = self.byte_offset;
        for entry in self.entries.iter_mut() {
            let pk = Self::extract_partial_key_static(mask, byte_offset, entry.key());
            match entry {
                Entry::Leaf(_, _, p) => *p = pk,
                Entry::Child(_, _, p) => *p = pk,
            }
        }
    }

    /// Extracts a partial key using a specific discriminative mask and byte offset.
    pub fn extract_partial_key_static(mask_bits: &[usize], byte_offset: usize, key: &K) -> u32
    where
        K: HotKey,
    {
        if mask_bits.is_empty() {
            return 0;
        }

        let val = key.get_u64_at(byte_offset);
        let mut mask = 0u64;
        for &bit_pos in mask_bits {
            let relative_pos = bit_pos.saturating_sub(byte_offset * 8);
            if relative_pos < 64 {
                mask |= 1 << (63 - relative_pos);
            }
        }

        pext(val, mask) as u32
    }

    /// Extracts a partial key from the given key using the node's discriminative mask and offset.
    pub fn extract_partial_key(&self, key: &K) -> u32
    where
        K: HotKey,
    {
        Self::extract_partial_key_static(&self.mask, self.byte_offset, key)
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

        let new_byte_offset = mask_vec.first().map(|&b| b / 8).unwrap_or(0);

        if self.mask != mask_vec || self.byte_offset != new_byte_offset {
            self.mask = mask_vec;
            self.byte_offset = new_byte_offset;
            self.refresh_partial_keys();
        }
    }
}
