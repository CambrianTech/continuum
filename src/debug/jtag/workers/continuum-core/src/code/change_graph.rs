//! Change Graph — DAG of file operations with undo/redo capability.
//!
//! Every file operation (create, write, edit, delete) creates a ChangeNode
//! in the graph. Each node stores forward and reverse diffs, enabling
//! point-in-time undo of any operation.
//!
//! Uses DashMap for lock-free concurrent access — multiple personas can
//! operate on different workspaces simultaneously with zero contention.

use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use super::types::{ChangeNode, FileOperation, FileDiff};

/// Per-workspace DAG of change operations.
///
/// Thread-safe: DashMap for node storage, RwLock for ordered indices.
/// Each workspace gets its own ChangeGraph instance.
pub struct ChangeGraph {
    workspace_id: String,
    /// Primary storage: node ID → ChangeNode
    nodes: DashMap<Uuid, ChangeNode>,
    /// File index: file_path → ordered list of node IDs (append-only)
    file_index: DashMap<String, Vec<Uuid>>,
    /// Chronological order of all node IDs (most recent last)
    chronological: RwLock<Vec<Uuid>>,
}

impl ChangeGraph {
    /// Create a new empty change graph for a workspace.
    pub fn new(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            nodes: DashMap::new(),
            file_index: DashMap::new(),
            chronological: RwLock::new(Vec::new()),
        }
    }

    /// Record a new change node in the graph.
    ///
    /// Adds the node to primary storage, file index, and chronological order.
    /// The caller is responsible for constructing the ChangeNode with correct
    /// parent_ids, diffs, etc.
    pub fn record(&self, node: ChangeNode) {
        let id = node.id;
        let file_path = node.file_path.clone();

        // Insert into primary storage
        self.nodes.insert(id, node);

        // Update file index
        self.file_index
            .entry(file_path)
            .or_default()
            .push(id);

        // Append to chronological order
        self.chronological.write().push(id);
    }

    /// Get a specific change node by ID.
    pub fn get(&self, id: &Uuid) -> Option<ChangeNode> {
        self.nodes.get(id).map(|r| r.clone())
    }

    /// Get the reverse diff for a specific change (for undo).
    ///
    /// Returns `(reverse_diff, file_path)` so the caller can apply the
    /// reverse diff to restore the file. Returns None if node not found.
    pub fn reverse_diff_for(&self, id: &Uuid) -> Option<(FileDiff, String)> {
        self.nodes
            .get(id)
            .map(|node| (node.reverse_diff.clone(), node.file_path.clone()))
    }

    /// Record an undo operation.
    ///
    /// Creates a new ChangeNode that reverses the target node.
    /// The reverse node's forward_diff is the target's reverse_diff (and vice versa).
    pub fn record_undo(&self, target_id: Uuid, author_id: &str) -> Option<ChangeNode> {
        let target = self.nodes.get(&target_id)?;

        let undo_node = ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: vec![target_id],
            author_id: author_id.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            file_path: target.file_path.clone(),
            operation: FileOperation::Undo {
                reverted_id: target_id,
            },
            // Swap forward/reverse: undo's forward is the original's reverse
            forward_diff: target.reverse_diff.clone(),
            reverse_diff: target.forward_diff.clone(),
            description: Some(format!("Undo: {}", target.description.as_deref().unwrap_or("previous change"))),
            workspace_id: self.workspace_id.clone(),
        };

        drop(target); // Release DashMap ref before mutating
        let result = undo_node.clone();
        self.record(undo_node);
        Some(result)
    }

    /// Get the last N change node IDs in reverse chronological order (for undo_last).
    ///
    /// Skips nodes that are already undo operations to avoid undo-of-undo chains.
    pub fn last_n_undoable(&self, count: usize) -> Vec<Uuid> {
        let chrono = self.chronological.read();
        chrono
            .iter()
            .rev()
            .filter(|id| {
                self.nodes
                    .get(id)
                    .map(|n| !matches!(n.operation, FileOperation::Undo { .. }))
                    .unwrap_or(false)
            })
            .take(count)
            .copied()
            .collect()
    }

    /// Get change history for a specific file, most recent first.
    pub fn file_history(&self, file_path: &str, limit: usize) -> Vec<ChangeNode> {
        let ids = match self.file_index.get(file_path) {
            Some(ids) => ids.clone(),
            None => return Vec::new(),
        };

        ids.iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.nodes.get(id).map(|r| r.clone()))
            .collect()
    }

    /// Get the most recent change node for a file.
    pub fn latest_for_file(&self, file_path: &str) -> Option<ChangeNode> {
        let ids = self.file_index.get(file_path)?;
        let last_id = ids.last()?;
        self.nodes.get(last_id).map(|r| r.clone())
    }

    /// Get all change history for the workspace, most recent first.
    pub fn workspace_history(&self, limit: usize) -> Vec<ChangeNode> {
        let chrono = self.chronological.read();
        chrono
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.nodes.get(id).map(|r| r.clone()))
            .collect()
    }

    /// Walk the DAG backwards from a node, collecting all ancestors.
    ///
    /// Uses BFS to handle the DAG structure (nodes can have multiple parents).
    /// Returns ancestors in breadth-first order (immediate parents first).
    pub fn ancestors(&self, node_id: &Uuid) -> Vec<ChangeNode> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        // Seed with the starting node's parents
        if let Some(node) = self.nodes.get(node_id) {
            for parent_id in &node.parent_ids {
                if visited.insert(*parent_id) {
                    queue.push_back(*parent_id);
                }
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if let Some(node) = self.nodes.get(&current_id) {
                for parent_id in &node.parent_ids {
                    if visited.insert(*parent_id) {
                        queue.push_back(*parent_id);
                    }
                }
                result.push(node.clone());
            }
        }

        result
    }

    /// Total number of change nodes in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// List all files that have been modified in this workspace.
    pub fn modified_files(&self) -> Vec<String> {
        self.file_index
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get the workspace ID this graph belongs to.
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::types::{FileDiff, DiffHunk};

    fn make_diff(content: &str) -> FileDiff {
        FileDiff {
            unified: content.to_string(),
            hunks: vec![DiffHunk {
                old_start: 1,
                old_count: 1,
                new_start: 1,
                new_count: 1,
                content: content.to_string(),
            }],
        }
    }

    fn make_node(file_path: &str, description: &str) -> ChangeNode {
        ChangeNode {
            id: Uuid::new_v4(),
            parent_ids: Vec::new(),
            author_id: "test-persona".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            file_path: file_path.to_string(),
            operation: FileOperation::Write,
            forward_diff: make_diff("+new content"),
            reverse_diff: make_diff("-new content"),
            description: Some(description.to_string()),
            workspace_id: "test-workspace".to_string(),
        }
    }

    #[test]
    fn test_record_and_get() {
        let graph = ChangeGraph::new("test-workspace");
        let node = make_node("src/main.ts", "Initial write");
        let id = node.id;

        graph.record(node);

        let retrieved = graph.get(&id).unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.file_path, "src/main.ts");
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_file_history() {
        let graph = ChangeGraph::new("test-workspace");

        let node1 = make_node("src/main.ts", "First edit");
        let node2 = make_node("src/main.ts", "Second edit");
        let node3 = make_node("src/other.ts", "Other file edit");

        let id1 = node1.id;
        let id2 = node2.id;

        graph.record(node1);
        graph.record(node2);
        graph.record(node3);

        let history = graph.file_history("src/main.ts", 10);
        assert_eq!(history.len(), 2);
        // Most recent first
        assert_eq!(history[0].id, id2);
        assert_eq!(history[1].id, id1);
    }

    #[test]
    fn test_workspace_history() {
        let graph = ChangeGraph::new("test-workspace");

        let node1 = make_node("src/a.ts", "Edit a");
        let node2 = make_node("src/b.ts", "Edit b");
        let node3 = make_node("src/c.ts", "Edit c");

        let id1 = node1.id;
        let id3 = node3.id;

        graph.record(node1);
        graph.record(node2);
        graph.record(node3);

        let history = graph.workspace_history(2);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, id3); // Most recent
        assert_eq!(history[1].description.as_deref(), Some("Edit b"));

        let all = graph.workspace_history(100);
        assert_eq!(all.len(), 3);
        assert_eq!(all[2].id, id1); // Oldest
    }

    #[test]
    fn test_undo_creates_reverse_node() {
        let graph = ChangeGraph::new("test-workspace");

        let original = make_node("src/main.ts", "Original write");
        let original_id = original.id;
        let original_forward = original.forward_diff.unified.clone();
        let original_reverse = original.reverse_diff.unified.clone();

        graph.record(original);

        let undo_node = graph.record_undo(original_id, "undo-persona").unwrap();

        // Undo node's forward diff should be original's reverse diff
        assert_eq!(undo_node.forward_diff.unified, original_reverse);
        // Undo node's reverse diff should be original's forward diff
        assert_eq!(undo_node.reverse_diff.unified, original_forward);
        assert!(matches!(undo_node.operation, FileOperation::Undo { reverted_id } if reverted_id == original_id));
        assert_eq!(graph.len(), 2);
    }

    #[test]
    fn test_last_n_undoable_skips_undo_nodes() {
        let graph = ChangeGraph::new("test-workspace");

        let node1 = make_node("src/a.ts", "Edit 1");
        let node2 = make_node("src/b.ts", "Edit 2");
        let id1 = node1.id;
        let id2 = node2.id;

        graph.record(node1);
        graph.record(node2);

        // Create an undo (which adds a 3rd node)
        graph.record_undo(id1, "persona");

        // last_n_undoable should skip the undo node
        let undoable = graph.last_n_undoable(5);
        assert_eq!(undoable.len(), 2);
        assert_eq!(undoable[0], id2); // Most recent non-undo
        assert_eq!(undoable[1], id1); // Older non-undo
    }

    #[test]
    fn test_ancestors_walks_dag() {
        let graph = ChangeGraph::new("test-workspace");

        // Create a chain: root → child → grandchild
        let root = make_node("src/main.ts", "Root");
        let root_id = root.id;
        graph.record(root);

        let mut child = make_node("src/main.ts", "Child");
        child.parent_ids = vec![root_id];
        let child_id = child.id;
        graph.record(child);

        let mut grandchild = make_node("src/main.ts", "Grandchild");
        grandchild.parent_ids = vec![child_id];
        let grandchild_id = grandchild.id;
        graph.record(grandchild);

        let ancestors = graph.ancestors(&grandchild_id);
        assert_eq!(ancestors.len(), 2);
        // BFS: child first, then root
        assert_eq!(ancestors[0].id, child_id);
        assert_eq!(ancestors[1].id, root_id);
    }

    #[test]
    fn test_latest_for_file() {
        let graph = ChangeGraph::new("test-workspace");

        let node1 = make_node("src/main.ts", "First");
        let node2 = make_node("src/main.ts", "Second");
        let id2 = node2.id;

        graph.record(node1);
        graph.record(node2);

        let latest = graph.latest_for_file("src/main.ts").unwrap();
        assert_eq!(latest.id, id2);
    }

    #[test]
    fn test_modified_files() {
        let graph = ChangeGraph::new("test-workspace");

        graph.record(make_node("src/a.ts", "Edit a"));
        graph.record(make_node("src/b.ts", "Edit b"));
        graph.record(make_node("src/a.ts", "Edit a again"));

        let mut files = graph.modified_files();
        files.sort();
        assert_eq!(files, vec!["src/a.ts", "src/b.ts"]);
    }

    #[test]
    fn test_empty_graph() {
        let graph = ChangeGraph::new("test-workspace");
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
        assert!(graph.file_history("any.ts", 10).is_empty());
        assert!(graph.workspace_history(10).is_empty());
        assert!(graph.latest_for_file("any.ts").is_none());
        assert!(graph.last_n_undoable(5).is_empty());
    }

    #[test]
    fn test_undo_nonexistent_returns_none() {
        let graph = ChangeGraph::new("test-workspace");
        assert!(graph.record_undo(Uuid::new_v4(), "persona").is_none());
    }
}
