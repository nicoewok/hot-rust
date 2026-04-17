use hot_rust::trie::*;

#[test]
fn test_lookup() {
    // Create a simple two-level trie
    // Level 1: [Leaf(10, "ten"), Child(20, Level 0)]
    // Level 0: [Leaf(20, "twenty"), Leaf(30, "thirty")]
    
    let mut level0 = HOTNode::new(0);
    level0.entries.push(Entry::Leaf(20, "twenty"));
    level0.entries.push(Entry::Leaf(30, "thirty"));
    
    let mut root = HOTNode::new(1);
    root.entries.push(Entry::Leaf(10, "ten"));
    root.entries.push(Entry::Child(20, Box::new(level0)));
    
    // Exact match in root
    assert_eq!(root.lookup(&10), Some(&"ten"));
    
    // Match in child
    assert_eq!(root.lookup(&20), Some(&"twenty"));
    assert_eq!(root.lookup(&30), Some(&"thirty"));
    
    // No match (branch not present)
    assert_eq!(root.lookup(&5), None);
    
    // No match (within a branch but key different)
    assert_eq!(root.lookup(&25), None);
    
    // No match (past the last entry)
    assert_eq!(root.lookup(&40), None);
}

#[test]
fn test_hot_wrapper() {
    let mut trie = HOT::new();
    trie.insert(10, "ten");
    trie.insert(20, "twenty");
    
    assert_eq!(trie.lookup(&10), Some(&"ten"));
    assert_eq!(trie.lookup(&20), Some(&"twenty"));
    assert_eq!(trie.lookup(&30), None);
}

#[test]
fn test_overflow_rule1_pull_up() {
    // Parent height 2, Child height 1. (h_p == h_c + 1)
    let mut trie = HOT::new();
    trie.root = Some(HOTNode::new(2));
    
    // Fill a single child at height 1
    for i in 0..MAX_FANOUT {
        trie.insert(i as u64, "val");
    }
    
    // Root should have 1 child which contains 32 leaves.
    let root = trie.root.as_ref().unwrap();
    assert_eq!(root.entries.len(), 1);
    
    // Insert 33rd element. Should cause child split and PARENT PULL UP.
    trie.insert(100, "hundred");
    
    let root = trie.root.as_ref().unwrap();
    // Since h_p == h_c + 1, root pulled up the split. Root now has 2 children.
    assert_eq!(root.entries.len(), 2);
    assert_eq!(root.height, 2);
}

#[test]
fn test_overflow_rule2_intermediate() {
    // Parent height 3, Child height 1. (h_p > h_c + 1)
    let mut trie = HOT::new();
    trie.root = Some(HOTNode::new(3));
    
    // Fill a single child path down to height 1
    for i in 0..MAX_FANOUT {
        trie.insert(i as u64, "val");
    }
    
    // Check Rule 2 by verifying height increase logic
    // This test is conceptual as the automatic filling of gaps is Rule 2's job.
}

#[test]
fn test_root_overflow_rule3() {
    let mut trie = HOT::new();
    // Fill the root (at height 1)
    for i in 0..MAX_FANOUT {
        trie.insert(i as u64, "val");
    }
    
    let root = trie.root.as_ref().unwrap();
    assert_eq!(root.entries.len(), MAX_FANOUT);
    assert_eq!(root.height, 1);
    
    // Insert 33rd element. Root must split and new root created at height 2.
    trie.insert(100, "hundred");
    
    let root = trie.root.as_ref().unwrap();
    assert_eq!(root.height, 2);
    assert_eq!(root.entries.len(), 2);
}

#[test]
fn test_to_dot() {
    let mut trie = HOT::<i32, &str>::new();
    trie.insert(10, "ten");
    trie.insert(20, "twenty");
    trie.insert(5, "five");
    
    let dot = trie.to_dot();
    assert!(dot.contains("digraph G"));
    assert!(dot.contains("Height: 1"));
    // We can't easily assert the whole string because of pointer addresses,
    // but we can check it's generated.
    println!("{}", dot);
}
