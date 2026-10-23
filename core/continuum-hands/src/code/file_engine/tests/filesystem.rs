#![cfg(test)]
use super::*;


    // what this catches: the engine acting on, recording, or ECHOING the raw fenced argument
    // instead of the bare path (card 1daffbaf: a `code/write` on `utils.py\`` succeeded and
    // created a second file; the result said success and named the fenced path, so she
    // read it as landed). The receipt must lead with what was stripped and where it landed,
    // and an argument that is nothing but fencing is refused, never written at the root.
    #[test]
    fn a_fenced_path_argument_lands_at_the_bare_file_and_the_receipt_says_so() {
        let (dir, engine) = setup_engine();
        let r = engine.write("src/utils.py`", "x = 1\n", None).unwrap();
        assert!(r.success);
        assert_eq!(r.file_path, "src/utils.py", "the echo is the bare path");
        assert!(dir.path().join("src/utils.py").is_file());
        assert!(
            !dir.path().join("src/utils.py`").exists(),
            "no file wears the fence"
        );
        let ctx = r.applied_context.expect("a cleaning receipt");
        assert!(
            ctx.starts_with("PATH ARGUMENT CLEANED"),
            "the receipt leads: {ctx}"
        );
        assert!(ctx.contains("backticks") && ctx.contains("'src/utils.py'"));

        // The change graph is keyed by the bare path too: a read of the bare path sees the
        // write, and the 5090 shape (fence + attached receipt line) edits the SAME file.
        let read = engine.read("`src/utils.py`", None, None).unwrap();
        assert_eq!(read.total_lines, 1);
        assert!(read.content.as_deref().is_some_and(|c| c.contains("x = 1")), "{read:?}");
        let e = engine
            .edit(
                "src/utils.py`\nsuccess: ✗",
                &EditMode::Append {
                    content: "y = 2\n".into(),
                },
                None,
            )
            .unwrap();
        assert_eq!(e.file_path, "src/utils.py");
        assert!(e
            .applied_context
            .unwrap()
            .starts_with("PATH ARGUMENT CLEANED"));
        assert_eq!(
            fs::read_to_string(dir.path().join("src/utils.py")).unwrap(),
            "x = 1\ny = 2\n"
        );
        let d = engine.delete("'src/utils.py'", None).unwrap();
        assert!(d
            .applied_context
            .unwrap()
            .starts_with("PATH ARGUMENT CLEANED"));
        assert!(!dir.path().join("src/utils.py").exists());

        // A bare path costs nothing: no receipt, no note.
        let plain = engine.write("src/plain.py", "z = 3\n", None).unwrap();
        assert!(plain.applied_context.is_none());

        // Nothing but fencing is a refusal by name, not a write at the workspace root.
        let refused = engine.write("`", "oops", None).unwrap_err().to_string();
        assert!(
            refused.contains("empty once its fencing is stripped"),
            "{refused}"
        );
        assert!(!dir.path().join("`").exists());
    }

    #[test]
    fn test_read_full_file() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/main.ts", None, None).unwrap();
        assert!(result.success);
        assert_eq!(result.total_lines, 3);
        assert!(result.content.unwrap().contains("line 1"));
    }

    #[test]
    fn test_read_line_range() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/main.ts", Some(2), Some(2)).unwrap();
        assert!(result.success);
        assert_eq!(result.lines_returned, 1);
        // Numbered by ABSOLUTE file position — a read must show the coordinate system
        // code/edit addresses, and a windowed read must not renumber from 1.
        assert_eq!(result.content.unwrap(), "     2 | line 2");
    }

    #[test]
    fn test_read_nonexistent() {
        let (_dir, engine) = setup_engine();
        let result = engine.read("src/nonexistent.ts", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_new_file() {
        let (_dir, engine) = setup_engine();
        let result = engine
            .write(
                "src/new.ts",
                "export const x = 1;\n",
                Some("Create new file"),
            )
            .unwrap();
        assert!(result.success);
        assert!(result.change_id.is_some());
        assert_eq!(result.bytes_written, 20);

        // Verify content
        let read = engine.read("src/new.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("export const x = 1;"));
    }

    #[test]
    fn test_write_overwrite_creates_diff() {
        let (_dir, engine) = setup_engine();

        // Overwrite existing file
        let result = engine
            .write("src/main.ts", "modified\n", Some("Overwrite"))
            .unwrap();
        assert!(result.success);

        // Check history
        let history = engine.file_history("src/main.ts", 10);
        assert_eq!(history.nodes.len(), 1);
        assert!(history.nodes[0].forward_diff.unified.contains("-line 1"));
        assert!(history.nodes[0].forward_diff.unified.contains("+modified"));
    }

    #[test]
    fn test_edit_search_replace() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "line 2 modified".to_string(),
                    all: false,
                },
                Some("Modify line 2"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("line 2 modified"));
    }

    #[test]
    fn test_edit_line_range() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 2,
                    new_content: "replaced line".to_string(),
                },
                Some("Replace line 2"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        let content = read.content.unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("replaced line"));
        assert!(content.contains("line 3"));
        assert!(!content.contains("line 2\n"));
    }

    // what this catches: the live stall (2026-07-14) — Devstral personas emitted
    // edit_file with end_line: 65535 (the u16 "to EOF" sentinel) to replace a
    // whole file; the engine rejected it as "out of range", they re-emitted the
    // identical broken edit turn after turn, never compiled/ran, and confabulated
    // success. A past-EOF end_line must CLAMP to the last line and land the edit.
    #[test]
    fn line_range_end_past_eof_clamps_and_replaces_to_end() {
        let (_dir, engine) = setup_engine(); // 3-line file: "line 1\nline 2\nline 3\n"

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 1,
                    end_line: 65535, // reflexive "replace whole file" sentinel
                    new_content: "brand new body".to_string(),
                },
                Some("whole-file replace via to-EOF sentinel"),
            )
            .unwrap();
        assert!(result.success);

        let content = engine
            .read("src/main.ts", None, None)
            .unwrap()
            .content
            .unwrap();
        assert!(content.contains("brand new body"));
        assert!(!content.contains("line 1"));
        assert!(!content.contains("line 3"));
    }

    // what this catches: a partial to-EOF edit — start mid-file, end past EOF —
    // keeps the head and replaces the tail, rather than erroring.
    #[test]
    fn line_range_from_middle_to_past_eof_keeps_head_replaces_tail() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 9999,
                    new_content: "tail rewritten".to_string(),
                },
                Some("replace from line 2 to end"),
            )
            .unwrap();
        assert!(result.success);

        let content = engine
            .read("src/main.ts", None, None)
            .unwrap()
            .content
            .unwrap();
        assert!(content.contains("line 1"));
        assert!(content.contains("tail rewritten"));
        assert!(!content.contains("line 2"));
        assert!(!content.contains("line 3"));
    }

    // what this catches: a genuinely inverted range (end below start-1) is still
    // an error — the clamp widens intent, it doesn't swallow contradictions.
    #[test]
    fn line_range_inverted_still_errors() {
        let (_dir, engine) = setup_engine();

        let result = engine.edit(
            "src/main.ts",
            &EditMode::LineRange {
                start_line: 3,
                end_line: 1,
                new_content: "x".to_string(),
            },
            Some("inverted range"),
        );
        assert!(result.is_err());
    }

    // what this catches: InsertAt past EOF is the "append" idiom — clamp to
    // total+1 and append instead of rejecting.
    #[test]
    fn insert_at_past_eof_appends() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 65535,
                    content: "appended tail".to_string(),
                },
                Some("append via past-EOF insert"),
            )
            .unwrap();
        assert!(result.success);

        // Assert on the FILE, not on code/read's rendering — this is an edit test, and
        // coupling it to how the read tool displays lines is what made it fail when the
        // display gained line numbers.
        let content = fs::read_to_string(_dir.path().join("src/main.ts")).unwrap();
        assert!(content.contains("line 3\nappended tail"));
    }

    #[test]
    fn test_edit_insert_at() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 2,
                    content: "inserted line".to_string(),
                },
                Some("Insert before line 2"),
            )
            .unwrap();
        assert!(result.success);

        // Assert on the FILE, not on code/read's rendering — see the sibling note in
        // insert_at_past_eof_appends.
        let content = fs::read_to_string(_dir.path().join("src/main.ts")).unwrap();
        assert!(content.contains("line 1\ninserted line\nline 2"));
    }

    #[test]
    fn test_edit_append() {
        let (_dir, engine) = setup_engine();

        let result = engine
            .edit(
                "src/main.ts",
                &EditMode::Append {
                    content: "line 4".to_string(),
                },
                Some("Append line 4"),
            )
            .unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None).unwrap();
        assert!(read.content.unwrap().contains("line 4"));
    }

    #[test]
    fn test_delete_file() {
        let (_dir, engine) = setup_engine();

        let result = engine.delete("src/main.ts", Some("Remove main")).unwrap();
        assert!(result.success);

        let read = engine.read("src/main.ts", None, None);
        assert!(read.is_err()); // File should not exist

        // what this catches: the most destructive verb reporting the least. Delete returned
        // `success: true, bytes_written: 0` and an empty applied_context — indistinguishable
        // from a no-op in a receipt. Same reasoning error that left code/write silent about
        // overwrites (4e74d93ce). Pins that a delete states the size of what it removed and
        // carries the undo handle.
        let ctx = result
            .applied_context
            .expect("a delete reports what it destroyed");
        let change_id = result.change_id.expect("a delete records a change");
        assert!(
            ctx.contains("DELETED") && ctx.contains("line(s)") && ctx.contains(&change_id),
            "a delete must name the file, its size, and the undo handle, got:\n{ctx}"
        );
    }

    // what this catches: the write boundary is the SANDBOX, not a file-extension
    // allowlist — a822d6337 (#1908) deleted the extension list deliberately, and
    // this pins the new contract so the list doesn't creep back. (This test
    // previously asserted `.exe` writes fail; it was stale against that commit.)
    #[test]
    fn test_write_any_extension_inside_sandbox() {
        let (_dir, engine) = setup_engine();
        let result = engine.write("src/tool.exe", "bytes", None);
        assert!(
            result.is_ok(),
            "extension is not a boundary; the sandbox is: {result:?}"
        );
    }

    #[test]
    fn test_preview_diff() {
        let (_dir, engine) = setup_engine();
        let diff = engine
            .preview_diff(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "LINE TWO".to_string(),
                    all: false,
                },
            )
            .unwrap();
        assert!(diff.unified.contains("-line 2"));
        assert!(diff.unified.contains("+LINE TWO"));
    }

    #[test]
    fn test_workspace_history() {
        let (_dir, engine) = setup_engine();

        engine.write("src/a.ts", "a", Some("Write a")).unwrap();
        engine.write("src/b.ts", "b", Some("Write b")).unwrap();

        let history = engine.workspace_history(10);
        assert_eq!(history.nodes.len(), 2);
        assert_eq!(history.nodes[0].description.as_deref(), Some("Write b"));
        assert_eq!(history.nodes[1].description.as_deref(), Some("Write a"));
    }

    #[test]
    fn test_edit_search_not_found() {
        let (_dir, engine) = setup_engine();
        let result = engine.edit(
            "src/main.ts",
            &EditMode::SearchReplace {
                search: "nonexistent text".to_string(),
                replace: "replacement".to_string(),
                all: false,
            },
            None,
        );
        assert!(result.is_err());
    }

    // ════════════════════════════════════════════════════════════════
    // Filesystem introspection — persona-as-developer cluster
    // ════════════════════════════════════════════════════════════════
    //
    // Tests for exists / list_dir / glob_match per
    // docs/planning/PERSONA-AS-DEVELOPER-GAP.md priority 1 (the
    // safe-self-scaffolding seam).

    fn setup_engine_with_tree() -> (tempfile::TempDir, FileEngine) {
        let dir = tempfile::tempdir().unwrap();
        // Mini tree:
        //   src/main.ts                              file
        //   src/utils/helpers.ts                     file
        //   src/utils/.private.ts                    hidden file
        //   src/empty_dir/                           empty dir
        //   docs/README.md                           file in sibling
        fs::create_dir_all(dir.path().join("src/utils")).unwrap();
        fs::create_dir_all(dir.path().join("src/empty_dir")).unwrap();
        fs::create_dir_all(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "x").unwrap();
        fs::write(dir.path().join("src/utils/helpers.ts"), "y").unwrap();
        fs::write(dir.path().join("src/utils/.private.ts"), "z").unwrap();
        fs::write(dir.path().join("docs/README.md"), "w").unwrap();
        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = FileEngine::new("test-persona", security);
        (dir, engine)
    }

    // ── exists ──────────────────────────────────────────────────────

    #[test]
    fn exists_reports_file_with_size() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.exists("src/main.ts").expect("exists must succeed");
        assert!(r.exists);
        assert_eq!(r.kind, Some(FsEntryKind::File));
        assert_eq!(r.size_bytes, Some(1));
        assert!(r.error.is_none());
    }

    #[test]
    fn exists_reports_directory_without_size() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.exists("src/utils").expect("exists must succeed");
        assert!(r.exists);
        assert_eq!(r.kind, Some(FsEntryKind::Directory));
        assert_eq!(r.size_bytes, None, "directories don't report size");
    }

    #[test]
    fn exists_reports_false_for_missing_with_no_error() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .exists("src/nonexistent.ts")
            .expect("missing path is NOT an error — exists=false");
        assert!(!r.exists);
        assert_eq!(r.kind, None);
        assert_eq!(r.size_bytes, None);
        assert!(r.error.is_none(), "missing != error per the contract");
    }

    #[test]
    fn exists_rejects_path_outside_workspace_via_path_security() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .exists("../escape.ts")
            .expect_err("workspace escape must fail loud via PathSecurity");
        let msg = err.to_string();
        assert!(
            msg.contains("Security") || msg.contains("escape"),
            "error must surface PathSecurity layer: {msg}"
        );
    }

    // ── list_dir ────────────────────────────────────────────────────

    #[test]
    fn list_dir_returns_flat_listing_directories_first() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.list_dir("src", false).expect("list must succeed");
        assert!(r.success);
        // src has: main.ts (file), utils (dir), empty_dir (dir)
        // Sorted: directories first (alphabetical: empty_dir, utils),
        // then files (main.ts).
        let names: Vec<&str> = r.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["empty_dir", "utils", "main.ts"],
            "directories must come before files; each group alphabetical"
        );
        assert_eq!(r.total_count, 3);
    }

    #[test]
    fn list_dir_excludes_hidden_by_default_includes_when_asked() {
        let (_dir, engine) = setup_engine_with_tree();

        let default = engine.list_dir("src/utils", false).expect("default");
        let names: Vec<&str> = default.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["helpers.ts"],
            ".private.ts must be excluded by default"
        );

        let with_hidden = engine
            .list_dir("src/utils", true)
            .expect("include_hidden=true");
        let names: Vec<&str> = with_hidden
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![".private.ts", "helpers.ts"],
            "include_hidden=true surfaces dotfiles, still alphabetical"
        );
    }

    #[test]
    fn list_dir_reports_file_size_only_for_files() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine.list_dir("src", false).expect("list");
        for entry in &r.entries {
            match entry.kind {
                FsEntryKind::File => assert!(
                    entry.size_bytes.is_some(),
                    "{}: file must report size_bytes",
                    entry.name
                ),
                FsEntryKind::Directory => assert!(
                    entry.size_bytes.is_none(),
                    "{}: directory must NOT report size_bytes",
                    entry.name
                ),
                _ => {}
            }
        }
    }

    #[test]
    fn list_dir_rejects_non_directory_path_loud() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .list_dir("src/main.ts", false)
            .expect_err("listing a file (not a dir) must fail loud");
        assert!(err.to_string().contains("not a directory"));
    }

    #[test]
    fn list_dir_for_missing_path_returns_not_found() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .list_dir("src/nonexistent", false)
            .expect_err("missing directory must fail loud");
        assert!(err.to_string().contains("not found"));
    }

    // what this catches: resolve_dir is the ONE directory resolver — idiom-forgiveness
    // + honest errors + sandbox boundary in one place (2026-07-14 consolidation, and
    // the fix for Anwen's "workspace/ prefix → workspace is not a directory → give up").
    #[test]
    fn resolve_dir_is_the_one_forgiving_honest_secure_resolver() {
        let (_dir, engine) = setup_engine_with_tree(); // has src/ (dir), src/main.ts (file)

        // plain dir resolves
        assert!(engine.resolve_dir("src").unwrap().is_dir());
        // "." / empty → workspace root
        assert_eq!(engine.resolve_dir(".").unwrap(), engine.workspace_root());
        assert_eq!(engine.resolve_dir("").unwrap(), engine.workspace_root());
        // FORGIVENESS: a redundant leading "workspace/" is stripped (the root already
        // IS the workspace) — this is the exact live give-up shape.
        assert_eq!(
            engine.resolve_dir("workspace/src").unwrap(),
            engine.resolve_dir("src").unwrap()
        );
        // a leading slash is tolerated
        assert_eq!(
            engine.resolve_dir("/src").unwrap(),
            engine.resolve_dir("src").unwrap()
        );
        // HONEST: a FILE says so and points to code/read
        let file_err = engine.resolve_dir("src/main.ts").unwrap_err().to_string();
        assert!(file_err.contains("is a FILE"), "{file_err}");
        assert!(file_err.contains("code/read"), "{file_err}");
        // HONEST: a miss re-orients instead of reading as terminal
        let miss = engine
            .resolve_dir("does/not/exist")
            .unwrap_err()
            .to_string();
        assert!(miss.contains("path not found"), "{miss}");
        assert!(
            miss.contains("workspace root itself IS explorable"),
            "{miss}"
        );
        // SECURE: `..` traversal is blocked (the bypass the old handler joins skipped)
        assert!(engine.resolve_dir("../../etc").is_err());
        assert_eq!(
            engine.resolve_dir(&engine.workspace_root().join("src").to_string_lossy()).unwrap(),
            engine.resolve_dir("src").unwrap()
        );
        let outside = tempfile::tempdir().unwrap();
        assert!(engine.resolve_dir(&outside.path().to_string_lossy()).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), engine.workspace_root().join("outside-link")).unwrap();
            assert!(engine.resolve_dir("outside-link").is_err());
        }
    }

    #[test]
    fn list_dir_handles_empty_directory_cleanly() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .list_dir("src/empty_dir", false)
            .expect("empty dir lists cleanly");
        assert_eq!(r.entries.len(), 0);
        assert_eq!(r.total_count, 0);
    }

    // ── glob_match ──────────────────────────────────────────────────

    #[test]
    fn glob_matches_files_by_extension_recursively() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.ts", None)
            .expect("glob must succeed");
        assert!(r.success);
        // Should match main.ts + helpers.ts (NOT .private.ts —
        // hidden files excluded by ignore's standard filters).
        assert!(
            r.matches.iter().any(|p| p == "src/main.ts"),
            "expected src/main.ts in matches: {:?}",
            r.matches
        );
        assert!(
            r.matches.iter().any(|p| p == "src/utils/helpers.ts"),
            "expected src/utils/helpers.ts in matches: {:?}",
            r.matches
        );
        // Matches are sorted for determinism.
        let mut sorted = r.matches.clone();
        sorted.sort();
        assert_eq!(r.matches, sorted, "matches must be sorted alphabetically");
        assert!(!r.truncated);
    }

    #[test]
    fn glob_scoped_to_subdirectory_via_root_param() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.ts", Some("src/utils"))
            .expect("scoped glob must succeed");
        // Only helpers.ts should match — main.ts is outside src/utils.
        assert_eq!(
            r.matches,
            vec!["src/utils/helpers.ts".to_string()],
            "root param must scope the walk: {:?}",
            r.matches
        );
    }

    #[test]
    fn glob_with_no_matches_returns_empty_not_error() {
        let (_dir, engine) = setup_engine_with_tree();
        let r = engine
            .glob_match("**/*.nope", None)
            .expect("no matches != error");
        assert!(r.success);
        assert!(r.matches.is_empty());
        assert_eq!(r.total_matches, 0);
        assert!(!r.truncated);
    }

    #[test]
    fn glob_rejects_bad_pattern_loud() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .glob_match("[invalid", None)
            .expect_err("malformed glob must fail loud");
        assert!(err.to_string().contains("bad pattern"));
    }

    #[test]
    fn glob_rejects_root_outside_workspace_via_path_security() {
        let (_dir, engine) = setup_engine_with_tree();
        let err = engine
            .glob_match("**/*", Some("../escape"))
            .expect_err("workspace escape must fail loud");
        let msg = err.to_string();
        assert!(
            msg.contains("Security") || msg.contains("escape"),
            "PathSecurity layer must surface: {msg}"
        );
    }

    // ── concurrency stress test ─────────────────────────────────────
    //
    // Per [field manual §4.2](docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md):
    // multi-thread tokio for any handler that holds state across
    // calls. FileEngine is &self read-only here, but workspaces are
    // shared across personas — N concurrent reads must NOT interfere.
    //
    // The test fires 32 concurrent exists/list/glob ops and verifies
    // every result is internally consistent.

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn introspection_under_concurrent_load_returns_consistent_results() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        for i in 0..10 {
            fs::write(dir.path().join(format!("src/file_{i}.ts")), "x").unwrap();
        }
        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = std::sync::Arc::new(FileEngine::new("test-persona", security));

        const PARALLEL: usize = 32;
        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let engine = engine.clone();
            tasks.push(tokio::spawn(async move {
                // Each task does the trio: exists + list + glob.
                let target = format!("src/file_{}.ts", i % 10);
                let exists = engine.exists(&target).expect("exists");
                let list = engine.list_dir("src", false).expect("list");
                let glob = engine.glob_match("**/*.ts", None).expect("glob");
                (exists, list, glob)
            }));
        }
        let results: Vec<_> = futures::future::join_all(tasks)
            .await
            .into_iter()
            .map(|r| r.expect("task must not panic"))
            .collect();

        for (exists, list, glob) in &results {
            // exists: always finds something (we round-robin file_0..9)
            assert!(exists.exists);
            assert_eq!(exists.kind, Some(FsEntryKind::File));
            // list: always returns the 10 src files
            assert_eq!(
                list.total_count, 10,
                "list result must be stable across concurrent reads"
            );
            // glob: always returns the 10 src files
            assert_eq!(
                glob.total_matches, 10,
                "glob must return all 10 matches regardless of concurrent siblings"
            );
        }
    }
