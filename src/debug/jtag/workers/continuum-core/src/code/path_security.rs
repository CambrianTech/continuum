//! Path Security — workspace-scoped path validation and traversal guard.
//!
//! Each persona gets a workspace root directory they cannot escape.
//! All file paths are canonicalized and validated before any I/O operation.
//!
//! Security guarantees:
//! - No directory traversal (../ sequences resolved and blocked)
//! - Extension allowlist enforced on write operations
//! - File size limits enforced on writes
//! - Symlinks resolved before validation (no symlink-based escapes)

use std::path::{Path, PathBuf};

use super::types::{ALLOWED_EXTENSIONS, MAX_WRITE_SIZE};

/// Workspace-scoped path security validator.
///
/// Validates that all file operations stay within the workspace boundary.
/// Created per-persona with their assigned workspace root.
#[derive(Debug, Clone)]
pub struct PathSecurity {
    /// The absolute, canonicalized workspace root.
    workspace_root: PathBuf,
    /// Optional read-only roots (e.g., the main codebase for discovery).
    read_roots: Vec<PathBuf>,
}

/// Errors that can occur during path validation.
#[derive(Debug, Clone, PartialEq)]
pub enum PathSecurityError {
    /// Path escapes the workspace boundary.
    TraversalBlocked { path: String, workspace: String },
    /// File extension not in allowlist.
    ExtensionBlocked { path: String, extension: String },
    /// File exceeds maximum write size.
    FileTooLarge { path: String, size: u64, max: u64 },
    /// Path is not valid UTF-8.
    InvalidPath { path: String },
    /// Workspace root does not exist or is not a directory.
    InvalidWorkspace { path: String },
}

impl std::fmt::Display for PathSecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TraversalBlocked { path, workspace } => {
                write!(f, "Path '{}' escapes workspace '{}'", path, workspace)
            }
            Self::ExtensionBlocked { path, extension } => {
                write!(f, "Extension '.{}' not allowed for '{}'", extension, path)
            }
            Self::FileTooLarge { path, size, max } => {
                write!(f, "File '{}' is {} bytes (max: {})", path, size, max)
            }
            Self::InvalidPath { path } => {
                write!(f, "Invalid path: '{}'", path)
            }
            Self::InvalidWorkspace { path } => {
                write!(f, "Invalid workspace root: '{}'", path)
            }
        }
    }
}

impl std::error::Error for PathSecurityError {}

impl PathSecurity {
    /// Create a new PathSecurity validator for a workspace.
    ///
    /// The workspace_root must exist and be a directory.
    /// It will be canonicalized (resolving symlinks).
    pub fn new(workspace_root: &Path) -> Result<Self, PathSecurityError> {
        let canonical = workspace_root.canonicalize().map_err(|_| {
            PathSecurityError::InvalidWorkspace {
                path: workspace_root.display().to_string(),
            }
        })?;

        if !canonical.is_dir() {
            return Err(PathSecurityError::InvalidWorkspace {
                path: canonical.display().to_string(),
            });
        }

        Ok(Self {
            workspace_root: canonical,
            read_roots: Vec::new(),
        })
    }

    /// Add a read-only root (e.g., the main codebase for code discovery).
    ///
    /// Paths within read_roots can be read but not written.
    pub fn add_read_root(&mut self, root: &Path) -> Result<(), PathSecurityError> {
        let canonical = root.canonicalize().map_err(|_| {
            PathSecurityError::InvalidWorkspace {
                path: root.display().to_string(),
            }
        })?;
        self.read_roots.push(canonical);
        Ok(())
    }

    /// Validate and resolve a path for read operations.
    ///
    /// The path must be within the workspace root OR any read-only root.
    /// Returns the absolute, canonicalized path.
    pub fn validate_read(&self, relative_path: &str) -> Result<PathBuf, PathSecurityError> {
        // Try workspace root first
        if let Ok(path) = self.resolve_within(&self.workspace_root, relative_path) {
            return Ok(path);
        }

        // Try read-only roots
        for root in &self.read_roots {
            if let Ok(path) = self.resolve_within(root, relative_path) {
                return Ok(path);
            }
        }

        Err(PathSecurityError::TraversalBlocked {
            path: relative_path.to_string(),
            workspace: self.workspace_root.display().to_string(),
        })
    }

    /// Validate and resolve a path for write operations.
    ///
    /// The path must be within the workspace root (not read-only roots).
    /// Also validates the file extension against the allowlist.
    /// Returns the absolute path (parent dir must exist).
    pub fn validate_write(&self, relative_path: &str) -> Result<PathBuf, PathSecurityError> {
        let resolved = self.resolve_for_write(relative_path)?;
        self.check_extension(relative_path)?;
        Ok(resolved)
    }

    /// Validate file size for a write operation.
    pub fn validate_size(&self, path: &str, size: u64) -> Result<(), PathSecurityError> {
        if size > MAX_WRITE_SIZE {
            return Err(PathSecurityError::FileTooLarge {
                path: path.to_string(),
                size,
                max: MAX_WRITE_SIZE,
            });
        }
        Ok(())
    }

    /// Resolve a relative path within a root, ensuring it doesn't escape.
    ///
    /// For existing files, uses canonicalize() to resolve symlinks.
    /// For new files, manually resolves the path and checks the prefix.
    fn resolve_within(&self, root: &Path, relative_path: &str) -> Result<PathBuf, PathSecurityError> {
        let joined = root.join(relative_path);

        // For existing paths, canonicalize resolves symlinks
        if joined.exists() {
            let canonical = joined.canonicalize().map_err(|_| {
                PathSecurityError::InvalidPath {
                    path: relative_path.to_string(),
                }
            })?;

            if canonical.starts_with(root) {
                return Ok(canonical);
            }

            return Err(PathSecurityError::TraversalBlocked {
                path: relative_path.to_string(),
                workspace: root.display().to_string(),
            });
        }

        // For non-existing paths, resolve parent and check
        Err(PathSecurityError::TraversalBlocked {
            path: relative_path.to_string(),
            workspace: root.display().to_string(),
        })
    }

    /// Resolve a relative path for write operations (file may not exist yet).
    ///
    /// The parent directory must exist and be within the workspace root.
    fn resolve_for_write(&self, relative_path: &str) -> Result<PathBuf, PathSecurityError> {
        // Check for obvious traversal attempts before any I/O
        let normalized = self.normalize_path(relative_path);
        if normalized.starts_with("..") || normalized.contains("/../") {
            return Err(PathSecurityError::TraversalBlocked {
                path: relative_path.to_string(),
                workspace: self.workspace_root.display().to_string(),
            });
        }

        let joined = self.workspace_root.join(&normalized);

        // If the file exists, canonicalize it
        if joined.exists() {
            let canonical = joined.canonicalize().map_err(|_| {
                PathSecurityError::InvalidPath {
                    path: relative_path.to_string(),
                }
            })?;

            if !canonical.starts_with(&self.workspace_root) {
                return Err(PathSecurityError::TraversalBlocked {
                    path: relative_path.to_string(),
                    workspace: self.workspace_root.display().to_string(),
                });
            }

            return Ok(canonical);
        }

        // For new files: walk up the parent chain to find the nearest existing
        // ancestor, canonicalize it, and verify it's within the workspace.
        // This handles creating files in not-yet-existing subdirectories
        // (e.g., "shared/format-utils.ts" when "shared/" doesn't exist yet).
        {
            let mut ancestor = joined.clone();
            // Walk up until we find an existing directory
            while let Some(parent) = ancestor.parent() {
                if parent.exists() {
                    let canonical_ancestor = parent.canonicalize().map_err(|_| {
                        PathSecurityError::InvalidPath {
                            path: relative_path.to_string(),
                        }
                    })?;

                    if !canonical_ancestor.starts_with(&self.workspace_root) {
                        return Err(PathSecurityError::TraversalBlocked {
                            path: relative_path.to_string(),
                            workspace: self.workspace_root.display().to_string(),
                        });
                    }

                    // Reconstruct: canonical ancestor + remaining relative components
                    let remaining = joined.strip_prefix(parent).map_err(|_| {
                        PathSecurityError::InvalidPath {
                            path: relative_path.to_string(),
                        }
                    })?;
                    return Ok(canonical_ancestor.join(remaining));
                }
                ancestor = parent.to_path_buf();
            }
        }

        Err(PathSecurityError::TraversalBlocked {
            path: relative_path.to_string(),
            workspace: self.workspace_root.display().to_string(),
        })
    }

    /// Check that a file's extension is in the allowlist.
    fn check_extension(&self, path: &str) -> Result<(), PathSecurityError> {
        let path = Path::new(path);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if extension.is_empty() || !ALLOWED_EXTENSIONS.contains(&extension) {
            return Err(PathSecurityError::ExtensionBlocked {
                path: path.display().to_string(),
                extension: extension.to_string(),
            });
        }

        Ok(())
    }

    /// Normalize a path by collapsing `.` and `..` components without I/O.
    ///
    /// This is a pre-check before any filesystem operations.
    /// Returns the normalized path. If `..` underflows (tries to go above root),
    /// the result will start with `..` to signal a traversal attempt.
    fn normalize_path(&self, path: &str) -> String {
        let mut components = Vec::new();
        let mut depth: i32 = 0; // Track depth relative to root

        for part in path.split('/') {
            match part {
                "" | "." => continue,
                ".." => {
                    if depth > 0 {
                        components.pop();
                        depth -= 1;
                    } else {
                        // Underflow: trying to go above workspace root
                        components.push("..");
                    }
                }
                other => {
                    components.push(other);
                    depth += 1;
                }
            }
        }

        components.join("/")
    }

    /// Get the workspace root path.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_workspace() -> (tempfile::TempDir, PathSecurity) {
        let dir = tempfile::tempdir().unwrap();
        // Create some subdirectories and files
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "console.log('hello');").unwrap();
        fs::write(dir.path().join("readme.md"), "# Hello").unwrap();

        let security = PathSecurity::new(dir.path()).unwrap();
        (dir, security)
    }

    #[test]
    fn test_valid_read() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_read("src/main.ts");
        assert!(result.is_ok());
    }

    #[test]
    fn test_traversal_blocked() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_read("../../etc/passwd");
        assert!(matches!(result, Err(PathSecurityError::TraversalBlocked { .. })));
    }

    #[test]
    fn test_dot_dot_traversal() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_write("src/../../etc/passwd.ts");
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_write_existing() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_write("src/main.ts");
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_write_new_file() {
        let (_dir, security) = setup_workspace();
        // New file in existing directory
        let result = security.validate_write("src/new_file.ts");
        assert!(result.is_ok());
    }

    #[test]
    fn test_extension_blocked() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_write("src/malware.exe");
        assert!(matches!(result, Err(PathSecurityError::ExtensionBlocked { .. })));
    }

    #[test]
    fn test_allowed_extensions() {
        let (_dir, security) = setup_workspace();
        // All these should pass extension check
        for ext in &["ts", "tsx", "js", "jsx", "json", "md", "css", "html", "rs", "toml", "yaml", "yml", "txt", "sh", "py"] {
            let path = format!("src/test.{}", ext);
            let result = security.check_extension(&path);
            assert!(result.is_ok(), "Extension '{}' should be allowed", ext);
        }
    }

    #[test]
    fn test_file_too_large() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_size("test.ts", MAX_WRITE_SIZE + 1);
        assert!(matches!(result, Err(PathSecurityError::FileTooLarge { .. })));
    }

    #[test]
    fn test_file_within_limit() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_size("test.ts", MAX_WRITE_SIZE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_root() {
        let dir = tempfile::tempdir().unwrap();
        let read_dir = tempfile::tempdir().unwrap();
        fs::write(read_dir.path().join("lib.ts"), "export {};").unwrap();

        let mut security = PathSecurity::new(dir.path()).unwrap();
        security.add_read_root(read_dir.path()).unwrap();

        // Can read from read root
        let result = security.validate_read("lib.ts");
        assert!(result.is_ok());
    }

    #[test]
    fn test_write_creates_in_workspace_not_read_root() {
        let dir = tempfile::tempdir().unwrap();
        let read_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(read_dir.path().join("libs")).unwrap();
        fs::write(read_dir.path().join("libs/external.ts"), "export {};").unwrap();

        let mut security = PathSecurity::new(dir.path()).unwrap();
        security.add_read_root(read_dir.path()).unwrap();

        // Can read from read root via relative path
        let read_result = security.validate_read("libs/external.ts");
        assert!(read_result.is_ok());

        // Writing "libs/external.ts" creates it in the WORKSPACE (writable),
        // not in the read root. This is valid — the file will be at
        // workspace/libs/external.ts. The read root is untouched.
        let write_result = security.validate_write("libs/external.ts");
        assert!(write_result.is_ok(), "Should be able to write new file in workspace subdirectory");
        let resolved = write_result.unwrap();
        let canonical_dir = dir.path().canonicalize().unwrap();
        assert!(resolved.starts_with(&canonical_dir), "Write should resolve within workspace, not read root");
    }

    #[test]
    fn test_normalize_path() {
        let (_dir, security) = setup_workspace();
        assert_eq!(security.normalize_path("src/../src/main.ts"), "src/main.ts");
        assert_eq!(security.normalize_path("./src/main.ts"), "src/main.ts");
        assert_eq!(security.normalize_path("src/./main.ts"), "src/main.ts");
        assert_eq!(security.normalize_path("a/b/c/../../d"), "a/d");
    }

    #[test]
    fn test_invalid_workspace() {
        let result = PathSecurity::new(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(matches!(result, Err(PathSecurityError::InvalidWorkspace { .. })));
    }
}
