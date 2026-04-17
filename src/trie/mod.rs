pub mod entry;
pub mod node;
pub mod hot;

// Re-export the main types for easier access from outside
pub use entry::Entry;
pub use node::HOTNode;
pub use hot::{HOT, OverflowResult, MAX_FANOUT};