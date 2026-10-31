use super::*;

/// Normalize a (possibly mis-shaped) code/edit call into a real [`EditMode`]. Forgives the
/// ways a model mis-calls the tool — glass-boxed live: a 14B sent `edit_mode:"append"` (bare
/// variant string) with no nested fields. Order: (1) strict tagged object, unchanged; (2) a
/// bare mode string, or a mode INFERRED from which top-level fields are present; then build the
/// variant, failing LOUD and naming the missing field (never a silent no-op that scores a false
/// zero). [[px-persona-experience-tools-as-good-ux]] [[fallbacks-are-illegal-fail-loud]]
pub(super) fn normalize_edit_mode(p: &CodeEditParams) -> Result<EditMode, CommandError> {
    // (1) Strict: already the tagged object — deserialize as-is (the preferred path, unchanged).
    if p.edit_mode.get("type").is_some() {
        return serde_json::from_value::<EditMode>(p.edit_mode.clone())
            .map_err(|e| CommandError::Arguments(format!("code/edit: {e}. {EDIT_MODE_HELP}")));
    }
    // A field pulled from top-level OR from an untyped edit_mode object (flat call shapes).
    let s = |top: &Option<String>, key: &str| -> Option<String> {
        top.clone().or_else(|| {
            p.edit_mode
                .get(key)
                .and_then(|v| v.as_str().map(str::to_string))
        })
    };
    let content = s(&p.content, "content");
    // Top-level old_string/new_string land on p.search/p.replace via serde aliases above; this
    // also accepts them NESTED inside an untyped `edit_mode:{old_string:…,new_string:…}` object.
    let search = s(&p.search, "search")
        .or_else(|| s(&None, "old_string"))
        .or_else(|| s(&None, "old_str"));
    let replace = s(&p.replace, "replace")
        .or_else(|| s(&None, "new_string"))
        .or_else(|| s(&None, "new_str"));
    let new_content = s(&p.new_content, "new_content");
    // Numeric fields, pulled from top-level OR the nested untyped edit_mode object.
    // Live glass-box (2026-07-14): Devstral emitted `edit_mode:{end_line:65535,
    // new_content:"…"}` — the line numbers were NESTED, not top-level, so the
    // top-level-only read missed them and the edit failed. One extractor, both
    // placements. [[px-persona-experience-tools-as-good-ux]]
    let n = |top: Option<u32>, key: &str| -> Option<u32> {
        top.or_else(|| {
            p.edit_mode
                .get(key)
                .and_then(|v| v.as_u64())
                .map(|x| x.min(u32::MAX as u64) as u32)
        })
    };
    let start_line = n(p.start_line, "start_line");
    let end_line = n(p.end_line, "end_line");
    let line = n(p.line, "line");
    // (2) The mode name: a bare string, else inferred from which fields are present.
    let mode = p.edit_mode.as_str().map(str::to_string).or_else(|| {
        if search.is_some() || replace.is_some() {
            Some("search_replace".into())
        } else if start_line.is_some() || end_line.is_some() || new_content.is_some() {
            Some("line_range".into())
        } else if line.is_some() {
            Some("insert_at".into())
        } else if content.is_some() {
            Some("append".into())
        } else {
            None
        }
    });
    let miss =
        |what: &str| CommandError::Arguments(format!("code/edit: needs `{what}`. {EDIT_MODE_HELP}"));
    match mode.as_deref().map(|m| m.trim().to_lowercase()).as_deref() {
        Some("search_replace") => Ok(EditMode::SearchReplace {
            search: search.ok_or_else(|| miss("search"))?,
            replace: replace.ok_or_else(|| miss("replace"))?,
            all: p.all.unwrap_or(false),
        }),
        Some("append") => Ok(EditMode::Append {
            content: content.ok_or_else(|| miss("content"))?,
        }),
        Some("insert_at") => Ok(EditMode::InsertAt {
            line: line.ok_or_else(|| miss("line"))?,
            content: content.ok_or_else(|| miss("content"))?,
        }),
        Some("line_range") => Ok(EditMode::LineRange {
            // The reflexive "replace the file" shape a model emits is
            // `{new_content, end_line: 65535}` with NO start_line: it means
            // "from the top, to the end". Default start_line→1 and end_line→MAX
            // (apply_edit clamps MAX to EOF) so that intent LANDS instead of
            // missing a field and looping. new_content is the one truly required
            // field — without it there is nothing to write. [[fallbacks-are-illegal-fail-loud]]
            start_line: start_line.unwrap_or(1),
            end_line: end_line.unwrap_or(u32::MAX),
            new_content: new_content.ok_or_else(|| miss("new_content"))?,
        }),
        _ => Err(CommandError::Arguments(format!(
            "code/edit: could not determine the edit mode. {EDIT_MODE_HELP}"
        ))),
    }
}

/// Reject a `file_path` that is a PLACEHOLDER rather than a real path. Glass-boxed live: a 14B
/// wrote `file_path:"<path_to_blueprints.py>"` — it echoed the schema's angle-bracket placeholder
/// syntax (or a `path_to_X` template) instead of substituting the concrete path it had ALREADY
/// seen in its own search results. Fail LOUD and point it at where the real value lives, rather
/// than a bare "file not found" that reads as "wrong file" instead of "you passed a template".
pub(super) fn reject_placeholder_path(file_path: &str) -> Result<(), CommandError> {
    let p = file_path.trim();
    let looks_placeholder = (p.starts_with('<') && p.ends_with('>'))
        || p.contains("path_to")
        || p.contains("/path/to/")
        || p.contains("your_file")
        || p.contains("<file")
        || p.contains("<path");
    if looks_placeholder {
        return Err(CommandError::Invalid(format!(
            "code/edit: file_path '{file_path}' is a PLACEHOLDER, not a real path. Use the concrete \
             workspace-relative path you saw in your code/search or code/glob results (e.g. \
             'src/flask/blueprints.py'), not a template."
        )));
    }
    Ok(())
}

#[async_trait]
impl ActionCommand for CodeEdit {
    const NAME: &'static str = "code/edit";
    const ALIASES: &'static [&'static str] =
        &["edit_file", "str_replace", "apply_patch", "replace_in_file"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Edit an existing file: line-range replace, search/replace, insert-at, or append. Undoable.";
    type Params = CodeEditParams;
    type Output = WriteResult;

    async fn run(&self, ctx: &Ctx, p: CodeEditParams) -> Result<WriteResult, CommandError> {
        reject_placeholder_path(&p.file_path)?;
        let engine = engine!(self, ctx);
        let edit = CanonicalEdit::try_from(p)?;
        let scope = read_scope_of(&self.state, ctx, &engine.workspace_root())?;
        engine
            .edit_in(&edit.file_path, &edit.edit_mode, edit.description.as_deref(), &scope)
            .map_err(engine_err)
    }
}

// ─────────────────────────── code/list ───────────────────────────
/// List a directory (flat, non-recursive).
pub struct CodeList {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeListParams {
    /// Directory to list, relative to the workspace root. Defaults to "." (root).
    #[serde(default)]
    pub path: Option<String>,
    /// Include hidden entries (dotfiles, `.git`, …). Defaults to false.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

#[async_trait]
impl ActionCommand for CodeList {
    const NAME: &'static str = "code/list";
    const ALIASES: &'static [&'static str] = &["list_files", "list_dir", "glob", "ls"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "List a directory (flat, non-recursive): names, kinds, and sizes. Use code/tree for recursion.";
    type Params = CodeListParams;
    type Output = ListResult;

    async fn run(&self, ctx: &Ctx, p: CodeListParams) -> Result<ListResult, CommandError> {
        let engine = engine!(self, ctx);
        let requested = p.path.as_deref().unwrap_or(".");
        // Ergonomic recovery: models trained on SWE-agent scaffolds reflexively
        // reach for `list_files("**/*.rs")` — a recursive GLOB where code/list
        // wants a single directory. list_dir rightly fails that (it's a flat
        // lister), but a bare NotFound teaches nothing and the model retries the
        // same glob (mined: 38% of live code/list calls, every failure a glob).
        // Meet the idiom: when the path IS a glob, resolve it via code/glob and
        // project the matches back as a listing — honest (they asked to "list"
        // these files, they get them), and it widens the surface without a new
        // verb. [[px-persona-experience-tools-as-good-ux]]
        if looks_like_file_glob(requested) {
            let glob = engine
                .glob_match(requested, None)
                .map_err(engine_err)?;
            let mut result = list_result_from_glob(requested, glob);
            // A zero-match glob is the OTHER "empty workspace" confabulation source
            // (glass-boxed 2026-07-14: a persona ran `code/list(path=**/)` — `**/`
            // matches no FILES, so the glob returns empty and she concluded the
            // workspace was empty). Same teach as the miss path, carried in the note
            // field so the empty listing itself isn't an error: name the real layout +
            // point at code/tree for a recursive view.
            if result.entries.is_empty() {
                if let Some(dirs) = top_level_dir_names(&engine) {
                    result.error = Some(format!(
                        "no files matched '{requested}' — this is NOT an empty workspace. \
                         Top-level directories: {}. For a recursive view use code/tree; to \
                         list one directory use code/list <dir>; for files by extension try \
                         a glob like `**/*.rs`.",
                        dirs.join(", ")
                    ));
                }
            }
            return Ok(result);
        }
        match engine.list_dir(requested, p.include_hidden.unwrap_or(false)) {
            Ok(listing) => Ok(listing),
            // A miss on a NON-glob path is the "empty workspace" confabulation source
            // (glass-boxed 2026-07-13: a persona reached for `src/persona.rs` — no such
            // top-level `src` here — got a bare NotFound, and concluded "the workspace
            // is empty", a false belief that then RECALLED and reinforced). A bare error
            // teaches nothing; enumerate the ACTUAL top-level dirs at the root so the
            // persona sees the true layout at the point of the miss and self-corrects —
            // the same teach-on-miss pattern as the glob recovery above and id_resolve.
            // [[px-persona-experience-tools-as-good-ux]]
            Err(e) => Err(teach_layout_on_miss(&engine, requested, &e.to_string())),
        }
    }
}

/// Turn a `code/list` path miss into a teaching error that names the real top-level
/// layout — so a persona that guessed a nonexistent path (`src/persona.rs`) learns
/// what DOES exist instead of concluding the workspace is empty. Best-effort: if the
/// root itself can't be listed, fall back to the original error rather than inventing
/// a layout ([[fallbacks-are-illegal-fail-loud]] — this is enrichment, never a silent
/// swallow; the miss still fails).
pub(super) fn teach_layout_on_miss(engine: &FileEngine, requested: &str, original: &str) -> CommandError {
    match top_level_dir_names(engine) {
        Some(dirs) if !dirs.is_empty() => CommandError::Invalid(format!(
            "'{requested}' not found in this workspace. Top-level directories here: {}. \
             Paths are relative to the root — don't assume source lives under `src/`; \
             pick one of these or drill in with code/list <dir>.",
            dirs.join(", ")
        )),
        Some(_) => CommandError::Invalid(format!(
            "'{requested}' not found and the workspace root has no directories ({original})"
        )),
        // Root itself couldn't be listed — never invent a layout, surface the real error.
        None => CommandError::Internal(original.to_string()),
    }
}

/// The sorted top-level directory names at the workspace root — the shared "what
/// actually exists here" fact behind every teach-on-miss ([`teach_layout_on_miss`]
/// and the zero-match-glob note). `None` only when the root itself can't be listed
/// (then the caller surfaces the real error rather than a fabricated layout).
pub(super) fn top_level_dir_names(engine: &FileEngine) -> Option<Vec<String>> {
    let root = engine.list_dir(".", false).ok()?;
    let mut dirs: Vec<String> = root
        .entries
        .iter()
        .filter(|e| e.kind == FsEntryKind::Directory)
        .map(|e| e.name.clone())
        .collect();
    dirs.sort();
    Some(dirs)
}

/// Project a [`GlobResult`] into a [`ListResult`] so a glob passed to `code/list`
/// returns a listing of the matching files (the reflexive `list_files("**/*.rs")`
/// idiom). Each match is a File entry — `code/glob` yields files only — with size
/// left `None` (a glob can match thousands; per-entry stat would defeat the
/// bounded cost `code/list` promises). `directory_path` records the glob so the
/// persona sees WHY the listing is cross-directory.
pub(super) fn list_result_from_glob(pattern: &str, glob: GlobResult) -> ListResult {
    let entries: Vec<DirEntry> = glob
        .matches
        .iter()
        .map(|path| DirEntry {
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.clone(),
            kind: FsEntryKind::File,
            size_bytes: None,
        })
        .collect();
    ListResult {
        success: glob.success,
        directory_path: format!("glob:{pattern}"),
        total_count: entries.len() as u32,
        entries,
        error: glob.error,
    }
}

// ─────────────────────────── code/exists ─────────────────────────
/// Check whether a path exists and what kind of entry it is.
pub struct CodeExists {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeExistsParams {
    /// Path to probe, relative to the workspace root.
    pub file_path: String,
}

#[async_trait]
impl ActionCommand for CodeExists {
    const NAME: &'static str = "code/exists";
    const DESCRIPTION: &'static str =
        "Check whether a path exists and its kind (file / directory / symlink) in one roundtrip.";
    type Params = CodeExistsParams;
    type Output = ExistsResult;

    async fn run(&self, ctx: &Ctx, p: CodeExistsParams) -> Result<ExistsResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .exists(&p.file_path)
            .map_err(engine_err)
    }
}

// ─────────────────────────── code/glob ───────────────────────────
/// Find files by glob pattern.
pub struct CodeGlob {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeGlobParams {
    /// Glob pattern. Use `**/` to recurse from the workspace root regardless
    /// of layout, e.g. `**/*.rs` matches Rust files anywhere in the tree.
    pub pattern: String,
    /// Optional sub-root to scope the glob, relative to the workspace root.
    #[serde(default)]
    pub root: Option<String>,
}

#[async_trait]
impl ActionCommand for CodeGlob {
    const NAME: &'static str = "code/glob";
    const DESCRIPTION: &'static str =
        "Find files by glob pattern (e.g. `**/*.rs` matches Rust files at any depth, layout-independent). Returns matching workspace-relative paths.";
    type Params = CodeGlobParams;
    type Output = GlobResult;

    async fn run(&self, ctx: &Ctx, p: CodeGlobParams) -> Result<GlobResult, CommandError> {
        let engine = engine!(self, ctx);
        engine
            .glob_match(&p.pattern, p.root.as_deref())
            .map_err(engine_err)
    }
}

// ─────────────────────────── code/tree ───────────────────────────
/// Print a recursive directory tree.
pub struct CodeTree {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeTreeParams {
    /// Subdirectory to root the tree at, relative to the workspace root. Defaults
    /// to the workspace root.
    #[serde(default)]
    pub path: Option<String>,
    /// Maximum recursion depth. Defaults to 10.
    #[serde(default)]
    pub max_depth: Option<u32>,
    /// Include hidden entries. Defaults to false.
    #[serde(default)]
    pub include_hidden: Option<bool>,
}

#[async_trait]
impl ActionCommand for CodeTree {
    const NAME: &'static str = "code/tree";
    const ALIASES: &'static [&'static str] = &["file_tree", "directory_tree", "tree"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Print a recursive directory tree (bounded depth) — the project's structure at a glance.";
    type Params = CodeTreeParams;
    type Output = TreeResult;

    async fn run(&self, ctx: &Ctx, p: CodeTreeParams) -> Result<TreeResult, CommandError> {
        let engine = engine!(self, ctx);
        // ONE resolver for every directory-oriented command (FileEngine::resolve_dir):
        // idiom-forgiveness (redundant "workspace/" prefix, leading '/') and honest,
        // actionable errors live there, not hand-rolled here. No path → the whole tree.
        let target = engine
            .resolve_dir(p.path.as_deref().unwrap_or("."))
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        Ok(tree::generate_tree(
            &target,
            p.max_depth.unwrap_or(10),
            p.include_hidden.unwrap_or(false),
        ))
    }
}

// ─────────────────────────── code/search ─────────────────────────
/// Heuristic: does this string look like a FILE GLOB (a path pattern) rather than a
/// grep term/regex? Conservative — requires a `*`/`?` AND a path-glob shape (`/`, `**`,
/// or a leading `*.ext`) AND no whitespace — so a genuine content regex like `foo.*bar`
/// or `fn .*Params` is NOT misclassified, while `**/*.py`, `*.rs`, `src/**/*.js` are.
/// Used by code/search to auto-recover the common `pattern`↔`file_glob` conflation.
pub(super) fn looks_like_file_glob(pattern: &str) -> bool {
    let p = pattern.trim();
    !p.is_empty()
        && !p.chars().any(char::is_whitespace)
        && (p.contains('*') || p.contains('?'))
        && (p.contains('/') || p.contains("**") || p.starts_with("*."))
}

/// Search file contents for a pattern (grep across the workspace).
pub struct CodeSearch {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeSearchParams {
    /// Text/regex pattern to search for.
    pub pattern: String,
    /// Optional glob to restrict which files are searched. Omit to search ALL
    /// files. Globs recurse from the workspace root and are layout-independent,
    /// e.g. `**/*.rs` (NOT `src/**/*.rs` — don't assume the code lives under src/).
    #[serde(default)]
    pub file_glob: Option<String>,
    /// Cap on the number of matches returned. Defaults to 100.
    #[serde(default)]
    pub max_results: Option<u32>,
}

#[async_trait]
impl ActionCommand for CodeSearch {
    const NAME: &'static str = "code/search";
    const EFFECT: continuum_base::tool_effect::ToolEffect = continuum_base::tool_effect::ToolEffect::Observes;
    const ALIASES: &'static [&'static str] = &["grep", "search_files", "ripgrep", "rg", "search"];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
    const DESCRIPTION: &'static str =
        "Search file contents for a pattern across the workspace (grep). Returns file:line matches.";
    type Params = CodeSearchParams;
    type Output = SearchResult;

    async fn run(&self, ctx: &Ctx, p: CodeSearchParams) -> Result<SearchResult, CommandError> {
        let engine = engine!(self, ctx);
        let max = p.max_results.unwrap_or(100);

        // FORGIVENESS: models routinely conflate code/search (grep file CONTENTS — `pattern`
        // is the search TERM, `file_glob` filters which files) with code/glob (find files BY
        // name). A glob-shaped `pattern` with no `file_glob` greps CONTENTS for the literal glob
        // text → 0 matches → the model retries the identical call forever (glass-boxed live: a
        // 14B fired `code/search(pattern="**/*.py")` 12× in a row, never advancing to read/edit).
        // If `pattern` looks like a file glob and no `file_glob` was given, do what it MEANT: list
        // the matching FILES (code/glob's job) and hand them back + a one-line note on the right
        // shape, so a single call makes progress instead of looping. [[px-persona-experience-tools-as-good-ux]]
        if p.file_glob.is_none() && looks_like_file_glob(&p.pattern) {
            let g = engine
                .glob_match(&p.pattern, None)
                .map_err(engine_err)?;
            let matches: Vec<SearchMatch> = g
                .matches
                .iter()
                .take(max as usize)
                .map(|path| SearchMatch {
                    file_path: path.clone(),
                    line_number: 0,
                    line_content: String::new(),
                    match_start: 0,
                    match_end: 0,
                })
                .collect();
            return Ok(SearchResult {
                success: true,
                total_matches: g.total_matches,
                files_searched: g.matches.len() as u32,
                matches,
                error: Some(format!(
                    "`pattern` {:?} looked like a FILE GLOB, so I listed matching FILES (that is \
                     code/glob's job). To search file CONTENTS instead, call code/search with a \
                     text/regex `pattern` (e.g. \"Blueprint\") PLUS `file_glob` {:?}. {} file(s) matched.",
                    p.pattern, p.pattern, g.total_matches
                )),
            });
        }

        // Search every searchable root (workspace + read-only roots), merging and
        // de-duplicating by (file, line) since roots may overlap.
        let mut matches: Vec<SearchMatch> = Vec::new();
        let mut total_matches = 0u32;
        let mut files_searched = 0u32;
        for root in engine.searchable_roots() {
            let remaining = max.saturating_sub(matches.len() as u32);
            if remaining == 0 {
                break;
            }
            let r = search::search_files(&root, &p.pattern, p.file_glob.as_deref(), remaining);
            if !r.success {
                // Preserve the search owner's typed refusal; an invalid regex/glob is
                // not a successful empty search with orientation guidance.
                return Ok(r);
            }
            total_matches += r.total_matches;
            files_searched += r.files_searched;
            matches.extend(r.matches);
        }
        matches.sort_by(|a, b| {
            a.file_path
                .cmp(&b.file_path)
                .then(a.line_number.cmp(&b.line_number))
        });
        matches.dedup_by(|a, b| a.file_path == b.file_path && a.line_number == b.line_number);
        matches.truncate(max as usize);

        // Preserve the caller-bounded result. The shared tool executor owns
        // window-aware whole-or-handle delivery and durable expansion; compressing
        // matches here would discard rows before that custody boundary sees them.
        let mut error = None;

        // Grounding for an empty search: a glob that matched ZERO files almost
        // always means the caller guessed a wrong path prefix (observed live —
        // globbed "continuum-core/*" when the tree is "core/continuum-core/…",
        // then confabulated a fake "agent_loop.py" rather than recover). Hand back
        // the workspace root + its top-level entries so the caller can re-orient
        // from the real layout instead of inventing one. Pure grounding DATA via
        // the existing `error` field — not a behavior gate, not the answer; the
        // map, not the route. (CLAUDE.md AI-QA: make tool failures recoverable.)
        if error.is_none() && files_searched == 0 && p.file_glob.is_some() {
            let root = engine.workspace_root();
            let mut tops: Vec<String> = std::fs::read_dir(&root)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| !n.starts_with('.') && n != "target" && n != "node_modules")
                        .collect()
                })
                .unwrap_or_default();
            tops.sort();
            tops.truncate(24);
            error = Some(format!(
                "No files matched glob {:?}. Workspace root is {} and all paths are \
                 relative to it; its top-level directories are: [{}]. Re-check the \
                 glob against this actual layout.",
                p.file_glob.as_deref().unwrap_or(""),
                root.display(),
                tops.join(", "),
            ));
        }

        Ok(SearchResult {
            success: true,
            matches,
            total_matches,
            files_searched,
            error,
        })
    }
}

// ─────────────────────────── code/shell ──────────────────────────
/// Run a command in a fresh shell process using the caller's workspace (the agentic
/// primitive — build, test, git, training, a daemon). `Privileged` → the Trusted
/// tier: a local persona or trusted node may run it; a remote `Provisional` peer
/// may NOT.
///
/// HANDLE-BASED, not a forced promise: the command starts immediately and is
/// waited on INLINE only up to `wait_ms`. If it finishes in that window the full
/// result comes back; if not, the response carries `status: running` + the
/// `execution_id` HANDLE and the command keeps running — poll it with
/// `code/shell-poll`, stop it with `code/shell-kill`. A long build / test suite /
/// training run / daemon therefore never blocks the turn or fails as a timed-out
/// promise (the bug that dumbed down long-running work before).
pub struct CodeShell {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellParams {
    /// Command; paths in code strings are not shell-translated.
    ///
    /// Each execution starts a fresh shell. For example, use `cd path && command`
    /// in the same call. Check failures explicitly; use `set -o pipefail` where
    /// supported when a pipeline must fail if any child fails. Child cwd/env/options
    /// do not persist to later calls; files and execution handles do.
    pub cmd: String,
    /// How long to wait INLINE for completion before returning the execution_id
    /// handle to poll. Defaults to 30000 (30s). A long job keeps running past this.
    #[serde(default)]
    pub wait_ms: Option<u64>,
    /// Optional HARD kill timeout (ms) for the command itself. Default: none — a
    /// long-running command (training, a daemon) runs until it finishes or you
    /// `code/shell-kill` it. Set this only when you want the command force-stopped.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Snapshot an execution's current state into the wire response. Used both when a
/// command COMPLETES within the inline window and when it's still RUNNING (handle
/// handed back with partial output + `exit_code: None`).
pub(super) fn shell_response(s: &crate::code::shell_session::ExecutionState) -> ShellExecuteResponse {
    ShellExecuteResponse {
        execution_id: s.id.clone(),
        status: s.status.clone(),
        stdout: Some(s.stdout_lines.join("\n")),
        stderr: Some(s.stderr_lines.join("\n")),
        exit_code: s.exit_code,
        next: (s.status == ShellExecutionStatus::Running).then(|| {
            format!(
                "still running: its result is delivered to you when it finishes; read its progress now with code/shell-poll execution_id={}",
                s.id
            )
        }),
    }
}

#[async_trait]
impl ActionCommand for CodeShell {
    const NAME: &'static str = "code/shell";
    const ALIASES: &'static [&'static str] = &[
        "bash",
        "shell",
        "run_terminal_cmd",
        "execute_command",
        "run_command",
    ];
    const NATIVE: bool = true; // core agentic working set — offered natively (auto-derived)
                               // Privileged → Trusted tier: arbitrary execution is for high-trust local
                               // citizens (a local persona / a trusted node), never a Provisional remote peer.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "New shell per cmd (Windows Bash; configured Unix shell). Files/handles persist; cd/env/options \
         do not. Use native host paths. Shell status follows last-command/pipeline/|| rules, not each \
         child's success. wait_ms defaults to 30000; poll running execution_id with code/shell-poll.";
    type Params = CodeShellParams;
    type Output = ShellExecuteResponse;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeShellParams,
    ) -> Result<ShellExecuteResponse, CommandError> {
        let who = ensure_shell(&self.state, ctx).await?;

        // Start the command while briefly holding the shell entry, then DROP the
        // DashMap ref before awaiting — never hold a lock across `.await` (the
        // bounded wait below blocks the turn, not the shard). [[concurrency-style-guide]]
        let state_arc = {
            let mut shell = self.state.shell_sessions.get_mut(&who).ok_or_else(|| {
                CommandError::Internal("shell vanished after provisioning".into())
            })?;
            let exec_id = shell
                .execute(&p.cmd, p.timeout_ms, &self.state.rt_handle)
                .map_err(CommandError::Internal)?;
            shell
                .get_execution_state(&exec_id)
                .map_err(CommandError::Internal)?
        };

        // BOUNDED inline wait: return the moment it completes, or hand back the
        // handle when the window elapses — never block past wait_ms. No DashMap
        // lock held across the await.
        let deadline =
            Instant::now() + Duration::from_millis(p.wait_ms.unwrap_or(DEFAULT_SHELL_WAIT_MS));
        loop {
            let notify = {
                let s = state_arc
                    .lock()
                    .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
                if s.status != ShellExecutionStatus::Running {
                    return Ok(shell_response(&s)); // completed in-window
                }
                s.output_notify.clone()
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let mut s = state_arc
                    .lock()
                    .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
                // Mark the handback so the exit fold PUSHES completion (the
                // dispatch listener folds it into her working memory) instead
                // of waiting silently to be polled.
                s.handed_back = true;
                return Ok(shell_response(&s)); // still running → hand back the handle
            }
            // Wake on new output or when the inline window closes, then re-check.
            let _ = tokio::time::timeout(remaining, notify.notified()).await;
        }
    }
}

// ─────────────────────────── code/shell-poll ─────────────────────
/// Poll a running (or finished) shell execution by its handle — the read half of
/// the handle-based long-running contract.
pub struct CodeShellPoll {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellPollParams {
    /// Full execution_id or an unambiguous hex prefix within your shell session.
    pub execution_id: String,
}

#[async_trait]
impl ActionCommand for CodeShellPoll {
    const NAME: &'static str = "code/shell-poll";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const NATIVE: bool = true; // follow a long command (install, build, server), like Claude's
    const DESCRIPTION: &'static str =
        "Poll a shell execution by its execution_id handle: current status, accumulated \
         stdout/stderr, and exit_code once finished. The non-blocking way to follow a long command.";
    type Params = CodeShellPollParams;
    type Output = ShellExecuteResponse;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CodeShellPollParams,
    ) -> Result<ShellExecuteResponse, CommandError> {
        // The hands these hands reach: a handle from her claim A stays hers after her focus
        // moved to claim B; an isolated eval reaches only itself (Astra on #5026).
        let state_arc = self
            .state
            .with_execution(&hands_of(&self.state, ctx), &p.execution_id, |s, id| s.get_execution_state(id))
            .map_err(execution_lookup_error)?;
        let s = state_arc
            .lock()
            .map_err(|e| CommandError::Internal(format!("execution lock poisoned: {e}")))?;
        Ok(shell_response(&s))
    }
}

/// The refusal for a handle that did not reach a command. "No shell session" only when she
/// truly has none (Cormac on #4586); otherwise the handle is named as unknown or ambiguous.
pub(super) fn execution_lookup_error(e: crate::modules::code::ExecutionLookup) -> CommandError {
    use crate::modules::code::ExecutionLookup;
    match e {
        ExecutionLookup::NoSession => CommandError::NotFound(
            "no shell session for caller: start a command with code/shell first".into(),
        ),
        ExecutionLookup::Unresolved(why) => CommandError::Invalid(format!(
            "{why}. The handle may belong to a finished-and-collected command or be ambiguous; \
             use the full execution_id from code/shell"
        )),
        ExecutionLookup::Failed(why) => CommandError::Internal(why),
    }
}

// ─────────────────────────── code/shell-kill ─────────────────────
/// Terminate a running shell execution by its handle.
pub struct CodeShellKill {
    pub state: Arc<CodeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CodeShellKillParams {
    /// Full execution_id or an unambiguous hex prefix within your shell session.
    pub execution_id: String,
}
