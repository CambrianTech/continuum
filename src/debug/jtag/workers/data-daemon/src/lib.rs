pub mod storage;
pub mod entities;

// Re-export for easier test access
pub use storage::{StorageAdapter, SqliteAdapter};
