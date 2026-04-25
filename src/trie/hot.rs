use crate::trie::{Entry, HOTNode};

#[derive(Debug, Clone)]
pub enum InsertResult<K, V> {
    Ok,
    NewMin(K),
    Split(Entry<K, V>, Entry<K, V>),
}

#[derive(Debug, Clone)]
pub enum RemoveResult<K, V> {
    NotFound,
    Removed(Vec<u64>),
    Empty,
    Underflow(Entry<K, V>, Vec<u64>),
}

#[derive(Debug, Clone, Default)]
pub struct RemovalResult {
    pub success: bool,
    pub removed_id: Option<u64>,
    pub collapsed_node_ids: Vec<u64>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SearchStep {
    pub node_id: u64,
    pub partial_key: u32,
    pub mask: Vec<usize>,
    pub byte_offset: usize,
    pub matched_entry_id: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub visited_nodes: Vec<u64>,
    pub visited_edges: Vec<(u64, u64)>, // (parent_id, child_id)
    pub steps: Vec<SearchStep>,
    pub leaf_id: Option<u64>,
    pub is_match: bool,
    pub is_false_positive: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchState {
    Idle,
    EvaluatingNode(usize), // step_index
    EvaluatingEdge(usize), // step_index
    ReachedLeaf,
    Scanning(usize),       // step_index for range scan
    Finished(bool),
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

    /// Performs a full search following the HOT algorithm with detailed path info.
    pub fn search(&self, key: &K) -> SearchResult {
        let mut res = SearchResult::default();
        if let Some(root) = &self.root {
            let (leaf_id, is_match) = root.search(key, &mut res.visited_nodes, &mut res.visited_edges, &mut res.steps);
            res.leaf_id = leaf_id;
            if let Some(_lid) = leaf_id {
                if is_match {
                    res.is_match = true;
                    res.message = "Match Found".to_string();
                } else {
                    res.is_false_positive = true;
                    res.message = "False Positive (Key mismatch at leaf)".to_string();
                }
            } else {
                res.message = "Not Found (No matching partial key)".to_string();
            }
        } else {
            res.message = "Trie is empty".to_string();
        }
        res
    }

    /// Removes a node by its ID.
    pub fn remove_by_id(&mut self, target_id: u64) -> RemovalResult {
        let mut res = RemovalResult::default();
        if let Some(root) = &mut self.root {
            if root.id == target_id {
                res.success = true;
                res.removed_id = Some(root.id);
                self.root = None;
                res.message = "Root removed".to_string();
                return res;
            }
            match root.remove_by_id(target_id) {
                RemoveResult::NotFound => {
                    res.success = false;
                    res.message = "Not found".to_string();
                }
                RemoveResult::Removed(ids) => {
                    res.success = true;
                    res.removed_id = Some(target_id);
                    res.collapsed_node_ids = ids;
                    res.message = "Entry removed".to_string();
                }
                RemoveResult::Empty => {
                    res.success = true;
                    res.removed_id = Some(target_id);
                    self.root = None;
                    res.message = "Trie became empty".to_string();
                }
                RemoveResult::Underflow(entry, mut collapsed) => {
                    res.success = true;
                    res.removed_id = Some(target_id);
                    collapsed.push(root.id);
                    res.collapsed_node_ids = collapsed;
                    self.handle_root_underflow(entry);
                    res.message = "Underflow handled: level collapsed".to_string();
                }
            };
        }
        res
    }

    /// Removes a key from the trie.
    pub fn remove(&mut self, key: &K) -> RemovalResult {
        let mut res = RemovalResult::default();
        if let Some(root) = &mut self.root {
            match root.remove(key) {
                RemoveResult::NotFound => {
                    res.success = false;
                    res.message = "Key not found".to_string();
                }
                RemoveResult::Removed(ids) => {
                    res.success = true;
                    res.collapsed_node_ids = ids;
                    res.message = "Key removed".to_string();
                }
                RemoveResult::Empty => {
                    res.success = true;
                    self.root = None;
                    res.message = "Trie became empty".to_string();
                }
                RemoveResult::Underflow(entry, mut collapsed) => {
                    res.success = true;
                    collapsed.push(root.id);
                    res.collapsed_node_ids = collapsed;
                    self.handle_root_underflow(entry);
                    res.message = "Underflow handled: level collapsed".to_string();
                }
            };
        }
        res
    }

    fn handle_root_underflow(&mut self, entry: Entry<K, V>) {
        match entry {
            Entry::Leaf(k, v, _) => {
                let mut new_root = HOTNode::new(1, self.fanout);
                new_root.entries.push(Entry::Leaf(k, v, 0));
                new_root.update_mask_from_entries();
                self.root = Some(new_root);
            }
            Entry::Child(_, node, _) => {
                self.root = Some(*node);
            }
        }
    }

    /// Performs a range scan from `start_key` to `end_key` (inclusive).
    pub fn range_scan(&self, start_key: &K, end_key: &K) -> Vec<K> {
        let mut results = Vec::new();
        let root = match &self.root {
            Some(r) => r,
            None => return results,
        };

        // 1. Find Start: Use existing search logic to find the leaf for start_key.
        let mut stack = Vec::new();
        let mut current_node = root;
        loop {
            let search_pk = current_node.extract_partial_key(start_key);
            let mut found = false;
            for (i, entry) in current_node.entries.iter().enumerate() {
                if entry.partial_key() == search_pk {
                    stack.push((current_node, i));
                    match entry {
                        Entry::Leaf(_, _, _) => {
                            found = true;
                        }
                        Entry::Child(_, node, _) => {
                            current_node = node;
                            found = true;
                        }
                    }
                    break;
                }
            }
            
            if !found { break; }
            
            // Check if we just pushed a leaf
            let (node, idx) = stack.last().unwrap();
            if matches!(node.entries[*idx], Entry::Leaf(_, _, _)) {
                break;
            }
        }

        if stack.is_empty() {
            return results;
        }

        // 3. Ascend/Descend and Collect
        while let Some(&(node, idx)) = stack.last() {
            let entry = &node.entries[idx];
            let key = entry.key();

            // While the current key is <= end_key
            if key > end_key {
                break;
            }

            match entry {
                Entry::Leaf(k, _, _) => {
                    // Add the current leaf to the results (if it's within range)
                    if k >= start_key {
                        results.push(k.clone());
                    }

                    // Increment the index in the current node.
                    if let Some((_, i)) = stack.last_mut() {
                        *i += 1;
                    }
                }
                Entry::Child(_, child, _) => {
                    // If moving into a child node, always start at index 0.
                    stack.push((child, 0));
                    continue;
                }
            }

            // If the index exceeds the node's capacity, move up to the parent and take the next branch.
            while let Some((n, i)) = stack.last() {
                if *i >= n.entries.len() {
                    stack.pop();
                    if let Some((_, pi)) = stack.last_mut() {
                        *pi += 1;
                    }
                } else {
                    break;
                }
            }
        }

        results
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_scan_simple() {
        let mut trie = HOT::new(4);
        trie.insert("apple".to_string(), 1);
        trie.insert("banana".to_string(), 2);
        trie.insert("cherry".to_string(), 3);
        trie.insert("date".to_string(), 4);
        trie.insert("eggplant".to_string(), 5);

        let results = trie.range_scan(&"banana".to_string(), &"date".to_string());
        assert_eq!(results, vec!["banana".to_string(), "cherry".to_string(), "date".to_string()]);
    }

    #[test]
    fn test_range_scan_full() {
        let mut trie = HOT::new(4);
        trie.insert("a".to_string(), 1);
        trie.insert("b".to_string(), 2);
        trie.insert("c".to_string(), 3);

        let results = trie.range_scan(&"a".to_string(), &"z".to_string());
        assert_eq!(results, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_range_scan_empty() {
        let trie: HOT<String, i32> = HOT::new(4);
        let results = trie.range_scan(&"a".to_string(), &"z".to_string());
        assert!(results.is_empty());
    }

    #[test]
    fn test_range_scan_out_of_bounds() {
        let mut trie = HOT::new(4);
        trie.insert("b".to_string(), 1);
        trie.insert("c".to_string(), 2);

        let results = trie.range_scan(&"x".to_string(), &"z".to_string());
        assert!(results.is_empty());

        let results2 = trie.range_scan(&"a".to_string(), &"a".to_string());
        assert!(results2.is_empty());
    }
}
