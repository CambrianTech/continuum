//! Path resolution and workspace introspection: exists, list_dir, glob.
use super::*;

impl FileEngine {
    /// THE gate a persona's path ARGUMENT passes before it is an address (card 1daffbaf):
    /// markdown fencing and an attached receipt line are stripped once, here, and the
    /// cleaned string is what every file verb resolves, records in the change graph, and
    /// echoes back — never the raw argument. An argument that is NOTHING but fencing is
    /// refused by name rather than resolved to the workspace root.
    pub(super) fn path_arg(&self, raw: &str) -> Result<CleanedPathArg, FileEngineError> {
        let cleaned = clean_path_arg(raw);
        if cleaned.path.is_empty() {
            return Err(FileEngineError::NotFound(format!(
                "the path argument {raw:?} is empty once its fencing is stripped — send a \
                 bare path: no backticks, no quotes, one line"
            )));
        }
        Ok(cleaned)
    }

    /// Get the underlying PathSecurity (for search/tree operations that need it).
    pub fn security(&self) -> &PathSecurity {
        &self.security
    }

    /// Get the workspace root path.
    pub fn workspace_root(&self) -> PathBuf {
        self.security.workspace_root().to_path_buf()
    }

    /// Get all searchable roots: workspace root + read-only roots.
    /// Used by code/search and code/tree to search the full project, not just the worktree.
    pub fn searchable_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.security.workspace_root().to_path_buf()];
        roots.extend(self.security.read_roots().iter().cloned());
        roots
    }

    /// Resolve a workspace-relative path for INTROSPECTION queries
    /// (`exists`, `list_dir`, `glob_match`) where the path is allowed
    /// to NOT exist yet — `exists()` returning false isn't an error.
    ///
    /// `validate_read` rejects non-existent paths (TraversalBlocked)
    /// because it canonicalizes, which fails on missing entries.
    /// That's correct for read/write/edit which require the file —
    /// but wrong for introspection where the whole point is to
    /// answer "does this exist?". Hence this separate validator:
    /// string-level traversal check + join, no existence requirement.
    /// The ONE traversal-guarded join primitive: join `relative` under `root`,
    /// rejecting an absolute path or any `..` segment (the string-level escape
    /// vectors once existence isn't required). Every workspace-path resolver that
    /// does NOT require the target to already exist ([`validate_introspect_path`],
    /// [`resolve_dir`]) goes through here, so the sandbox boundary lives in ONE
    /// place instead of being hand-rolled per call site (a manual `root.join(rel)`
    /// in a command handler silently skips this check — the bypass this consolidates
    /// away). Symlink-escape on EXISTING paths is caught by PathSecurity's
    /// canonicalize check ([`validate_read`]); this is the floor for maybe-missing
    /// paths.
    pub(super) fn secure_join(&self, root: &Path, relative: &str) -> Result<PathBuf, FileEngineError> {
        let blocked = || {
            FileEngineError::Security(PathSecurityError::TraversalBlocked {
                path: relative.to_string(),
                workspace: root.display().to_string(),
            })
        };
        if relative.starts_with('/') || relative.starts_with('\\') {
            return Err(blocked());
        }
        if relative.split(['/', '\\']).any(|seg| seg == "..") {
            return Err(blocked());
        }
        Ok(root.join(relative))
    }

    /// Resolve a workspace-relative path for INTROSPECTION queries (`exists`,
    /// `list_dir`, `glob_match`) where the path is allowed to NOT exist yet.
    /// Thin wrapper over [`secure_join`] against the workspace root.
    pub(super) fn validate_introspect_path(&self, relative: &str) -> Result<PathBuf, FileEngineError> {
        self.secure_join(self.security.workspace_root(), relative)
    }

    /// Resolve a persona-supplied path to an existing DIRECTORY within the sandbox —
    /// the ONE resolver every directory-oriented command (`code/tree`, `code/list`,
    /// the `code/search`/`code/glob` root) shares. Path-idiom forgiveness and honest
    /// errors thus live in ONE place instead of drifting per handler (they used to:
    /// only `code/tree` tolerated the prefix; `code/list` and `code/search` hand-rolled
    /// `searchable_roots().join(rel)`, which ALSO skipped the sandbox check).
    ///
    /// Forgives the idioms a persona reaches for from what it sees rendered: a leading
    /// `/`, and a redundant leading `workspace/` segment (the root already IS the
    /// workspace, so that prefix doubles and misses — glass-boxed 2026-07-14). Tries
    /// each searchable root (workspace + read-only roots). HONEST, actionable errors:
    /// a FILE says "use code/read", a miss says "the workspace root itself IS
    /// explorable" — so a mind re-orients instead of concluding it is stuck.
    pub fn resolve_dir(&self, relative: &str) -> Result<PathBuf, FileEngineError> {
        // File and directory tools accept the same receipt paths. Resolve before
        // stripping the leading slash, which would turn an absolute address into
        // a different relative path. Canonical validation also checks symlinks.
        if let Ok(abs) = self.security.validate_read(relative) {
            if abs.is_dir() {
                return Ok(abs);
            }
        }
        let rel = relative.trim().trim_start_matches('/');
        if rel.is_empty() || rel == "." {
            return Ok(self.security.workspace_root().to_path_buf());
        }
        let mut candidates = vec![rel];
        if let Some(stripped) = rel.strip_prefix("workspace/") {
            candidates.push(stripped);
        }
        let mut file_hit: Option<PathBuf> = None;
        for root in self.searchable_roots() {
            for cand in &candidates {
                // `..`/absolute are rejected here; a rejected candidate is skipped, not
                // fatal, so a later candidate/root can still resolve. A genuinely
                // malicious `..` resolves to no directory and surfaces the miss error.
                if let Ok(abs) = self.secure_join(&root, cand) {
                    let Ok(abs) = self.security.validate_read(&abs.to_string_lossy()) else {
                        continue;
                    };
                    if abs.is_dir() {
                        return Ok(abs);
                    }
                    if abs.is_file() {
                        file_hit.get_or_insert(abs);
                    }
                }
            }
        }
        if let Some(f) = file_hit {
            return Err(FileEngineError::NotFound(format!(
                "{} is a FILE, not a directory — use code/read to read it, or run this on \
                 its parent directory.",
                f.display()
            )));
        }
        Err(FileEngineError::NotFound(format!(
            "path not found: {relative} — nothing exists at that path. This is about the \
             PATH, not the workspace: the workspace root itself IS explorable — call this \
             command with NO path (or \".\") for the whole tree, then use a path you see \
             in that output. Paths are relative to the workspace root; do not prefix them \
             with 'workspace/'."
        )))
    }

    /// Check whether a path exists, and if so what kind of entry it is.
    ///
    /// Closes the "is this path safe to write to / scaffold into?"
    /// question in one call. Per
    /// [PERSONA-AS-DEVELOPER-GAP.md](../../../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md),
    /// this is the top-priority filesystem-introspection seam: a
    /// persona running `generate/module` needs to probe before
    /// scaffolding to avoid clobbering.
    ///
    /// Uses `validate_introspect_path` so non-existent paths report
    /// `exists: false` rather than failing with a security error.
    /// Symlinks report as `Symlink` without following — callers that
    /// want follow-the-link semantics can `code/read` and observe the
    /// `NotFound` error if the target is broken.
    pub fn exists(&self, relative_path: &str) -> Result<ExistsResult, FileEngineError> {
        let abs_path = self.validate_introspect_path(relative_path)?;

        // symlink_metadata so we don't follow links transparently.
        let meta = fs::symlink_metadata(&abs_path);
        match meta {
            Ok(m) => {
                let kind = if m.is_symlink() {
                    FsEntryKind::Symlink
                } else if m.is_file() {
                    FsEntryKind::File
                } else if m.is_dir() {
                    FsEntryKind::Directory
                } else {
                    FsEntryKind::Other
                };
                let size_bytes = if matches!(kind, FsEntryKind::File) {
                    Some(m.len())
                } else {
                    None
                };
                Ok(ExistsResult {
                    success: true,
                    exists: true,
                    file_path: relative_path.to_string(),
                    kind: Some(kind),
                    size_bytes,
                    error: None,
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ExistsResult {
                success: true,
                exists: false,
                file_path: relative_path.to_string(),
                kind: None,
                size_bytes: None,
                error: None,
            }),
            Err(e) => Err(FileEngineError::Io(e)),
        }
    }

    /// Flat directory listing (no recursion). Hidden entries (names
    /// starting with `.`) excluded unless `include_hidden` is true.
    ///
    /// Sorted: directories first, then files, both alphabetical.
    /// Predictable order matters for persona reproducibility (a
    /// generator that picks "first available name" must get the
    /// same answer every run).
    ///
    /// For recursive output, callers use `code/tree` instead — this
    /// is intentionally O(N) in directory size, not O(N) in subtree
    /// size, so cheap-by-design.
    pub fn list_dir(
        &self,
        relative_path: &str,
        include_hidden: bool,
    ) -> Result<ListResult, FileEngineError> {
        // ONE resolver (resolve_dir): idiom-forgiveness + the not-found / is-a-FILE
        // honest errors live there, shared with code/tree — not re-checked here.
        let abs_path = self.resolve_dir(relative_path)?;

        let workspace_root = self.security.workspace_root();
        let mut entries: Vec<DirEntry> = Vec::new();

        for raw in fs::read_dir(&abs_path)? {
            let raw = match raw {
                Ok(e) => e,
                Err(_) => continue, // single bad entry shouldn't kill the listing
            };
            let name = raw.file_name().to_string_lossy().to_string();
            if !include_hidden && name.starts_with('.') {
                continue;
            }
            // Stat each entry so we can report kind + size. Errors on
            // individual entries surface as `Other` rather than
            // failing the whole listing — partial info beats none.
            let entry_meta = fs::symlink_metadata(raw.path()).ok();
            let kind = match entry_meta.as_ref() {
                Some(m) if m.is_symlink() => FsEntryKind::Symlink,
                Some(m) if m.is_file() => FsEntryKind::File,
                Some(m) if m.is_dir() => FsEntryKind::Directory,
                _ => FsEntryKind::Other,
            };
            let size_bytes = match (entry_meta.as_ref(), kind) {
                (Some(m), FsEntryKind::File) => Some(m.len()),
                _ => None,
            };
            let path = raw
                .path()
                .strip_prefix(workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| raw.path().to_string_lossy().to_string());
            entries.push(DirEntry {
                name,
                path,
                kind,
                size_bytes,
            });
        }

        // Directories first, then files; alphabetical within each.
        // Symlinks + Other sort as directories (uncommon enough that
        // their ordering doesn't justify a third bucket).
        entries.sort_by(|a, b| {
            let a_is_file = matches!(a.kind, FsEntryKind::File);
            let b_is_file = matches!(b.kind, FsEntryKind::File);
            a_is_file.cmp(&b_is_file).then(a.name.cmp(&b.name))
        });

        let total_count = entries.len() as u32;
        Ok(ListResult {
            success: true,
            directory_path: relative_path.to_string(),
            entries,
            total_count,
            error: None,
        })
    }

    /// Glob expansion scoped to the workspace (or a `root`
    /// subdirectory of it). Uses the `ignore` crate's overrides for
    /// `.gitignore`-respecting walks, same as `code/search`.
    ///
    /// Patterns are workspace-relative globs like `**/*.rs` or
    /// `core/**/Cargo.toml`. Output is workspace-relative
    /// paths, sorted alphabetically. Capped at `GLOB_MAX_MATCHES`
    /// (5000) so a runaway pattern doesn't OOM the caller —
    /// `truncated: true` flags the cap.
    pub fn glob_match(
        &self,
        pattern: &str,
        root: Option<&str>,
    ) -> Result<GlobResult, FileEngineError> {
        // Root may not exist; use introspect validator. For the actual
        // walk, the directory MUST exist — error if not.
        let scan_root = match root {
            Some(r) => {
                let p = self.validate_introspect_path(r)?;
                if !p.is_dir() {
                    return Err(FileEngineError::NotFound(format!(
                        "code/glob: root is not a directory: {r}"
                    )));
                }
                p
            }
            None => self.security.workspace_root().to_path_buf(),
        };

        // Build the override as a whitelist match for the pattern.
        // OverrideBuilder treats non-`!` patterns as whitelist; we
        // explicitly check `is_whitelist()` per entry so only matched
        // files are emitted.
        let mut overrides = ignore::overrides::OverrideBuilder::new(&scan_root);
        overrides
            .add(pattern)
            .map_err(|e| FileEngineError::EditFailed(format!("code/glob: bad pattern: {e}")))?;
        let overrides = overrides
            .build()
            .map_err(|e| FileEngineError::EditFailed(format!("code/glob: overrides build: {e}")))?;

        // standard_filters=true ⇒ respects .gitignore, .ignore, AND
        // hides hidden files by default. Persona-as-developer
        // contract: glob does NOT see dotfiles unless the pattern
        // explicitly starts with `.` (matches Unix shell intuition).
        let walker = ignore::WalkBuilder::new(&scan_root)
            .standard_filters(true)
            .hidden(true)
            .build();

        let workspace_root = self.security.workspace_root();
        let mut matches: Vec<String> = Vec::new();
        let mut truncated = false;

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();

            // Skip the scan root itself (the walker yields it).
            if path == scan_root {
                continue;
            }

            // FILES only — directories are not glob matches per the
            // contract. (A persona that wants to enumerate directories
            // uses `code/list`.) `file_type` returns Some when the
            // walker stat'd it; treat None as "skip" (rare).
            let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
            if !is_file {
                continue;
            }

            // Explicit whitelist check — only emit when the pattern
            // matched this specific path. `Override::matched(path,
            // is_dir)` returns Match::None / Ignore / Whitelist; we
            // want Whitelist only.
            let m = overrides.matched(path, false);
            if !m.is_whitelist() {
                continue;
            }

            let rel = path
                .strip_prefix(workspace_root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| path.to_string_lossy().to_string());

            if matches.len() >= GLOB_MAX_MATCHES {
                truncated = true;
                break;
            }
            matches.push(rel);
        }

        matches.sort();
        let total_matches = matches.len() as u32;

        Ok(GlobResult {
            success: true,
            pattern: pattern.to_string(),
            matches,
            total_matches,
            truncated,
            error: None,
        })
    }

    /// Get the latest parent ID for a file (for DAG edges).
    pub(super) fn latest_parent(&self, file_path: &str) -> Vec<Uuid> {
        self.graph
            .latest_for_file(file_path)
            .map(|n| vec![n.id])
            .unwrap_or_default()
    }
}
