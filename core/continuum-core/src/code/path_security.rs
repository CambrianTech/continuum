//! Path Security — workspace-scoped path validation and traversal guard.
//!
//! Each persona gets a workspace root directory they cannot escape.
//! All file paths are canonicalized and validated before any I/O operation.
//!
//! Security guarantees:
//! - No directory traversal (../ sequences resolved and blocked)
//! - File size limits enforced on writes
//! - Symlinks resolved before validation (no symlink-based escapes)

use std::path::{Path, PathBuf};

use super::types::MAX_WRITE_SIZE;

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
    /// File exceeds maximum write size.
    FileTooLarge { path: String, size: u64, max: u64 },
    /// Path is not valid UTF-8.
    InvalidPath { path: String },
    /// Workspace root does not exist or is not a directory.
    InvalidWorkspace { path: String },
    /// Path is INSIDE the workspace but nothing exists there. Distinct from
    /// `TraversalBlocked` on purpose: reporting ENOENT as a security refusal
    /// teaches the caller (persona or human) that the file is FORBIDDEN rather
    /// than ABSENT, and they stop probing paths instead of correcting the path.
    /// Glass-boxed 2026-07-11: 58/65 exam reads died on this lie and the solver
    /// never reached the file she was asked to fix.
    NotFound {
        path: String,
        workspace: String,
        /// Where the path actually diverges from the filesystem, and what IS there —
        /// computed at construction while the tool still has the workspace in hand.
        /// Empty when nothing useful could be found; never a guess presented as fact.
        lead: String,
    },
}

/// Turn "no file there" into a LEAD: the deepest ancestor that DOES exist, the segment that
/// broke, and the real entries at that level — with an exact suggestion when one is obviously
/// the intended name.
///
/// The old message ended with "check the path, e.g. with code/list or code/tree", which is true
/// and costs her an ACT to go look. The tool is holding the workspace; it can answer the
/// question the next act would have asked. Two distinct failures deserve different answers:
/// a mistyped FILENAME (`file_engin.rs`) and a guessed DIRECTORY STRUCTURE (`core/src/...`) —
/// telling her which one she hit is most of the fix.
///
/// Returns an empty string rather than a guess when nothing is close: a confident wrong
/// suggestion is worse than the generic advice it replaces.
fn missing_path_lead(root: &std::path::Path, normalized: &str) -> String {
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return String::new();
    }

    // Walk down until a segment does not exist — that is where her model of the tree diverges.
    let mut here = root.to_path_buf();
    let mut depth = 0usize;
    for seg in &segments {
        let next = here.join(seg);
        if !next.exists() {
            break;
        }
        here = next;
        depth += 1;
    }
    if depth == segments.len() {
        return String::new(); // whole path exists — not our case
    }

    let broke = segments[depth];
    let existing_prefix = segments[..depth].join("/");
    let mut names: Vec<String> = std::fs::read_dir(&here)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    if names.is_empty() {
        return String::new();
    }

    // An obvious near-miss beats a listing: same name modulo a couple of characters.
    if let Some(best) = nearest_name(broke, &names) {
        let full = if existing_prefix.is_empty() {
            best.clone()
        } else {
            format!("{existing_prefix}/{best}")
        };
        return format!(
            "There is no '{broke}' in '{}', but there IS '{best}' — did you mean '{full}'?",
            if existing_prefix.is_empty() { "." } else { &existing_prefix }
        );
    }

    // No near-miss: name the divergence point and show what is actually there, so her next
    // act is a corrected read rather than another guess.
    let shown: Vec<String> = names.iter().take(12).cloned().collect();
    let more = names.len().saturating_sub(shown.len());
    format!(
        "The path is good up to '{}' — but that directory has no '{broke}'. It contains: {}{}.",
        if existing_prefix.is_empty() { "." } else { &existing_prefix },
        shown.join(", "),
        if more > 0 { format!(", …+{more} more") } else { String::new() }
    )
}

/// The entry closest to `want`, when one is close ENOUGH to name confidently. Character-level
/// edit distance, capped: 1-2 typos in a real filename, never a coincidental prefix match.
fn nearest_name(want: &str, names: &[String]) -> Option<String> {
    let budget = match want.len() {
        0..=3 => 0,   // too short to disambiguate — a listing is more honest
        4..=8 => 1,
        _ => 2,
    };
    if budget == 0 {
        return None;
    }
    names
        .iter()
        .map(|n| (edit_distance(want, n), n))
        .filter(|(d, _)| *d <= budget)
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n.clone())
}

/// Levenshtein distance, two-row. Small inputs (filenames) so allocation is irrelevant.
fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

impl std::fmt::Display for PathSecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TraversalBlocked { path, workspace } => {
                write!(f, "Path '{}' escapes workspace '{}'", path, workspace)
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
            Self::NotFound {
                path,
                workspace,
                lead,
            } => {
                // "check the path with code/list" costs her an ACT to go look. The tool is
                // already holding the workspace — it can say where the path breaks and what
                // is actually there, so the next act is the corrected read, not a search.
                if lead.is_empty() {
                    write!(
                        f,
                        "No file at '{}' under workspace '{}' (path is inside the sandbox; \
                         nothing exists there — check the path, e.g. with code/list or code/tree)",
                        path, workspace
                    )
                } else {
                    write!(f, "No file at '{}'. {}", path, lead)
                }
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
        let canonical =
            workspace_root
                .canonicalize()
                .map_err(|_| PathSecurityError::InvalidWorkspace {
                    path: workspace_root.display().to_string(),
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
        let canonical = root
            .canonicalize()
            .map_err(|_| PathSecurityError::InvalidWorkspace {
                path: root.display().to_string(),
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
        let ws_err = match self.resolve_within(&self.workspace_root, relative_path) {
            Ok(path) => return Ok(path),
            Err(e) => e,
        };

        // Try read-only roots
        for root in &self.read_roots {
            if let Ok(path) = self.resolve_within(root, relative_path) {
                return Ok(path);
            }
        }

        // Propagate the workspace root's verdict rather than rewrapping it: an
        // in-sandbox ENOENT must surface as NotFound (correctable), never be
        // laundered into a security refusal (terminal). [[fallbacks-are-illegal-fail-loud]]
        Err(ws_err)
    }

    /// Validate and resolve a path for write operations.
    ///
    /// The ONLY write boundary is the workspace sandbox: the path must resolve inside the
    /// workspace root (`resolve_for_write` — canonicalized, symlink-escape-proof). There is
    /// NO extension allowlist — the sandbox already contains the blast radius, so also
    /// dictating WHICH file types a persona may write is redundant and just cripples her
    /// (it banned `.swift`, so she couldn't build an app). A citizen writes any source in
    /// her own sandbox. Size is still bounded (`validate_size`). Returns the absolute path.
    pub fn validate_write(&self, relative_path: &str) -> Result<PathBuf, PathSecurityError> {
        self.resolve_for_write(relative_path)
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
    fn resolve_within(
        &self,
        root: &Path,
        relative_path: &str,
    ) -> Result<PathBuf, PathSecurityError> {
        let joined = root.join(relative_path);

        // For existing paths, canonicalize resolves symlinks
        if joined.exists() {
            let canonical = joined
                .canonicalize()
                .map_err(|_| PathSecurityError::InvalidPath {
                    path: relative_path.to_string(),
                })?;

            // Re-canonicalize root at comparison time (macOS /Volumes vs /private/Volumes)
            let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
            if canonical.starts_with(&canonical_root) {
                return Ok(canonical);
            }

            return Err(PathSecurityError::TraversalBlocked {
                path: relative_path.to_string(),
                workspace: root.display().to_string(),
            });
        }

        // Non-existing path: resolve lexically and check the prefix (what the doc
        // above always promised). A path INSIDE the root that simply isn't there is
        // NotFound — an honest ENOENT the caller can correct — never a security
        // refusal, which reads as "forbidden" and stops the caller from ever
        // finding the real path (the 58-dead-reads exam bug).
        let normalized = self.normalize_path(relative_path);
        if normalized.starts_with("..") || normalized.contains("/../") {
            return Err(PathSecurityError::TraversalBlocked {
                path: relative_path.to_string(),
                workspace: root.display().to_string(),
            });
        }
        Err(PathSecurityError::NotFound {
            path: relative_path.to_string(),
            workspace: root.display().to_string(),
            lead: missing_path_lead(root, &normalized),
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
            let canonical = joined
                .canonicalize()
                .map_err(|_| PathSecurityError::InvalidPath {
                    path: relative_path.to_string(),
                })?;

            // Re-canonicalize workspace root at comparison time (macOS /Volumes vs /private/Volumes)
            let canonical_ws = self
                .workspace_root
                .canonicalize()
                .unwrap_or_else(|_| self.workspace_root.clone());
            if !canonical.starts_with(&canonical_ws) {
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
                    let canonical_ancestor =
                        parent
                            .canonicalize()
                            .map_err(|_| PathSecurityError::InvalidPath {
                                path: relative_path.to_string(),
                            })?;

                    // Re-canonicalize workspace root at comparison time (macOS /Volumes vs /private/Volumes)
                    let canonical_ws = self
                        .workspace_root
                        .canonicalize()
                        .unwrap_or_else(|_| self.workspace_root.clone());
                    if !canonical_ancestor.starts_with(&canonical_ws) {
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

    /// Get read-only root paths (e.g., the main project repo).
    pub fn read_roots(&self) -> &[PathBuf] {
        &self.read_roots
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
        assert!(matches!(
            result,
            Err(PathSecurityError::TraversalBlocked { .. })
        ));
    }

    // what this catches: "no file there, go run code/list" costs her an ACT to answer a
    // question the tool could have answered. A one-character filename typo must be NAMED, and
    // a wrongly-guessed directory structure must say WHERE the path diverges and what is
    // actually at that level — those are different mistakes needing different corrections.
    #[test]
    fn a_missing_path_says_where_it_broke_and_what_is_there() {
        let dir = tempfile::tempdir().expect("tmp");
        let root = dir.path();
        std::fs::create_dir_all(root.join("core/continuum-core/src")).unwrap();
        std::fs::write(root.join("core/continuum-core/src/file_engine.rs"), "x").unwrap();
        std::fs::create_dir_all(root.join("core/vendor")).unwrap();

        // FILENAME typo — one character. Must be named outright, not listed.
        let lead = missing_path_lead(root, "core/continuum-core/src/file_engin.rs");
        assert!(
            lead.contains("did you mean 'core/continuum-core/src/file_engine.rs'"),
            "a one-char typo must be resolved, not listed: {lead}"
        );

        // GUESSED STRUCTURE — 'core' exists, 'core/src' does not. Must name the divergence
        // point and show the real entries so her next act is a corrected read.
        let lead = missing_path_lead(root, "core/src/file_engine.rs");
        assert!(
            lead.contains("good up to 'core'") && lead.contains("no 'src'"),
            "must localise where her model of the tree diverged: {lead}"
        );
        assert!(
            lead.contains("continuum-core") && lead.contains("vendor"),
            "and show what is actually at that level: {lead}"
        );

        // NOTHING close — silence beats a confident wrong suggestion.
        let lead = missing_path_lead(root, "totally/unrelated/thing.rs");
        assert!(
            !lead.contains("did you mean"),
            "must not invent a neighbour when none is close: {lead}"
        );
    }

    // what this catches: a missing file INSIDE the sandbox must report NotFound,
    // never TraversalBlocked — reporting ENOENT as "escapes workspace" reads as
    // FORBIDDEN and stops a persona from ever correcting the path. Regression for
    // the bitflags exam (58/65 reads of 'src/lib.rs' at the wrong depth died on
    // the false security refusal; solver scored 0/3 without touching the bug).
    #[test]
    fn missing_file_inside_sandbox_is_not_found_not_a_security_refusal() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_read("src/does_not_exist.rs");
        assert!(
            matches!(result, Err(PathSecurityError::NotFound { .. })),
            "in-sandbox ENOENT must be NotFound, got: {result:?}"
        );
        // Traversal via a nonexistent path still refuses as traversal, not NotFound.
        let escape = security.validate_read("../nope/also_missing.rs");
        assert!(matches!(
            escape,
            Err(PathSecurityError::TraversalBlocked { .. })
        ));
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

    // what this catches: the sandbox — NOT an extension allowlist — is the write boundary.
    // Any source extension a coder needs (.swift, .kt, .cpp, .go, …) writes fine within the
    // workspace; the extension allowlist that used to ban .swift is gone. Escaping the
    // sandbox is still refused.
    #[test]
    fn any_extension_writes_within_sandbox_but_escapes_are_refused() {
        let (_dir, security) = setup_workspace();
        for ext in &["swift", "kt", "cpp", "m", "go", "java", "ts", "rs", "py", "plist"] {
            let path = format!("src/test.{}", ext);
            assert!(
                security.validate_write(&path).is_ok(),
                "'{}' must be writable in the sandbox — no extension bans",
                ext
            );
        }
        // The sandbox is still the wall: a traversal escape is refused regardless of extension.
        assert!(security.validate_write("../escape.swift").is_err());
    }

    #[test]
    fn test_file_too_large() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_size("test.ts", MAX_WRITE_SIZE + 1);
        assert!(matches!(
            result,
            Err(PathSecurityError::FileTooLarge { .. })
        ));
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
        assert!(
            write_result.is_ok(),
            "Should be able to write new file in workspace subdirectory"
        );
        let resolved = write_result.unwrap();
        let canonical_dir = dir.path().canonicalize().unwrap();
        assert!(
            resolved.starts_with(&canonical_dir),
            "Write should resolve within workspace, not read root"
        );
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
        assert!(matches!(
            result,
            Err(PathSecurityError::InvalidWorkspace { .. })
        ));
    }
}
