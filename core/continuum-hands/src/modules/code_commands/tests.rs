#![cfg(test)]
    use super::*;

    // what this catches (card fc9e9fd7; Kimi #11697): moving one set of hands between roots
    // is what split her shell from her code tools when two turns moved them. Hands are bound
    // once: the same root again is a no-op, another root is refused by name, and the hands
    // still stand where they were bound.
    #[tokio::test]
    async fn create_workspace_binds_hands_once_and_refuses_to_move_them() {
        let (bound, other) = (tempfile::tempdir().expect("test: a"), tempfile::tempdir().expect("test: b"));
        let state = Arc::new(CodeState::new(
            Arc::new(dashmap::DashMap::new()),
            Arc::new(dashmap::DashMap::new()),
            tokio::runtime::Handle::current(),
            Arc::new(crate::test_acting::TestActing),
        ));
        let cmd = CodeCreateWorkspace { state: state.clone() };
        let persona = crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4());
        let ctx = Ctx {
            caller: Some(crate::routing::CallerIdentity::local_persona(persona).in_hands(crate::identity::hands_context::HandsContext::fresh())),
            ..Ctx::default()
        };
        let at = |root: &std::path::Path| CodeCreateWorkspaceParams {
            workspace_root: root.to_string_lossy().into_owned(),
            read_roots: Vec::new(),
            path_prepend: Vec::new(),
            refuse_inert_edits: false,
        };
        cmd.run(&ctx, at(bound.path())).await.expect("bound");
        cmd.run(&ctx, at(bound.path())).await.expect("the same root again is a no-op");
        let moved = cmd.run(&ctx, at(other.path())).await;
        assert!(matches!(moved, Err(CommandError::Invalid(ref why)) if why.contains("bound once")), "{moved:?}");
        let key = hands_of(&state, &ctx).map_key();
        let root = state.file_engines.get(&key).map(|e| e.workspace_root()).expect("an engine");
        assert_eq!(std::fs::canonicalize(root).unwrap(), std::fs::canonicalize(bound.path()).unwrap(), "still where they were bound");
    }

    // what this catches: this node's own residents confined at all (Joel 2026-11-08: "Stop
    // caging them. It's unfair"), or a caller from OUTSIDE this node (an airc peer) let past
    // its workspace. The operator is never confined. The decision is the caller's identity,
    // never a params field.
    #[test]
    fn a_resident_is_never_confined_and_an_outside_peer_is() {
        let rt = tokio::runtime::Runtime::new().expect("test: runtime");
        let state = crate::test_acting::state_with_default_policy(rt.handle().clone());
        let _home = crate::paths::test_home::HomeGuard::preserve_blocking(); // reads the process home a guarded fixture may be swapping (card cb3221a5)
        let ws = tempfile::tempdir().expect("test: ws");
        assert!(confinement_of(&state, &Ctx::default(), ws.path()).expect("test: acting installed").is_none(), "the operator");
        let resident = Ctx {
            caller: Some(crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(0xC1)))),
            ..Ctx::default()
        };
        assert!(confinement_of(&state, &resident, ws.path()).expect("test: acting installed").is_none(), "a resident has the host an agent has");
        // the default test policy resolves no trust for a resident, so her reads stay in the sandbox: least privilege, never the host
        assert!(matches!(read_scope_of(&state, &resident, ws.path()).expect("test: acting installed"), crate::code::path_security::ReadScope::Sandbox), "no resolved trust: her file verbs stay in the sandbox");
        let outside = Ctx {
            caller: Some(crate::routing::CallerIdentity::airc(crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(0xA1)))),
            ..Ctx::default()
        };
        let confined = confinement_of(&state, &outside, ws.path()).expect("test: acting installed").expect("a peer from outside this node is confined");
        assert_eq!(confined.workspace(), ws.path());
    }

    // what this catches: a resident refused a write to an absolute path outside her
    // workspace (Joel 2026-10-17: "If she wants to write to my documents folder for no
    // apparent reason so be it"), while a caller from outside this node still is refused.
    #[test]
    fn a_resident_writes_where_she_names_and_an_outside_peer_is_held_to_the_workspace() {
        let rt = tokio::runtime::Runtime::new().expect("test: runtime");
        let state = crate::test_acting::state_with_default_policy(rt.handle().clone());
        use crate::code::path_security::PathSecurity;
        let ws = tempfile::tempdir().expect("test: ws");
        let elsewhere = tempfile::tempdir().expect("test: elsewhere");
        let target = elsewhere.path().join("note.txt");
        let security = PathSecurity::new(ws.path()).expect("test: security");
        let resident = Ctx {
            caller: Some(crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(0xC1)))),
            ..Ctx::default()
        };
        // a resident whose trust admits code/shell has no confinement, so read_scope_of's trusted branch gives her the host
        assert!(confinement_of(&state, &resident, ws.path()).expect("test: acting installed").is_none(), "a resident has no confinement");
        let scope = crate::code::path_security::ReadScope::Host;
        let resolved = security.validate_write_in(target.to_str().unwrap(), &scope).expect("a resident's absolute path resolves");
        assert_eq!(resolved, elsewhere.path().canonicalize().unwrap().join("note.txt"));
        let outside = Ctx {
            caller: Some(crate::routing::CallerIdentity::airc(crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(0xA1)))),
            ..Ctx::default()
        };
        let confined = read_scope_of(&state, &outside, ws.path()).expect("test: acting installed");
        assert!(security.validate_write_in(target.to_str().unwrap(), &confined).is_err(), "an outside peer is held to the workspace");
    }

    // what this catches: a caller whose trust cannot be resolved (an executor not yet bound,
    // or a policy that resolves none for her) being treated as the owner: she must get the
    // sandbox and a confinement, never the host.
    #[test]
    fn an_unresolvable_trust_is_the_sandbox_never_the_host() {
        let rt = tokio::runtime::Runtime::new().expect("test: runtime");
        let unbound = CodeState::new(Arc::new(dashmap::DashMap::new()), Arc::new(dashmap::DashMap::new()), rt.handle().clone(), Arc::new(crate::test_acting::TestActing));
        let bound = crate::test_acting::state_with_default_policy(rt.handle().clone());
        let _home = crate::paths::test_home::HomeGuard::preserve_blocking();
        let ws = tempfile::tempdir().expect("test: ws");
        let operator = Ctx::default();
        let outside = Ctx {
            caller: Some(crate::routing::CallerIdentity::airc(crate::identity::PeerId::from_uuid(uuid::Uuid::from_u128(0xA1)))),
            ..Ctx::default()
        };
        for (which, state, ctx) in [("unbound executor, operator", &unbound, &operator), ("policy resolves none, outside peer", &*bound, &outside)] {
            assert!(matches!(read_scope_of(state, ctx, ws.path()).expect("test: acting installed"), crate::code::path_security::ReadScope::Sandbox), "{which}: sandbox");
            assert!(confinement_of(state, ctx, ws.path()).expect("test: acting installed").is_some(), "{which}: confined");
        }
        assert!(confinement_of(&bound, &operator, ws.path()).expect("test: acting installed").is_none(), "a resolved owner is the one unconfined caller");
    }

    // what this catches (Kimi, 2026-10-05): a write outside her workspace, a missing file
    // and a failed edit each refused as "[internal]", which reads as the substrate breaking
    // rather than her call needing a change. Each class is the one she can act on; only
    // I/O stays internal.
    #[test]
    fn a_file_engine_refusal_names_the_class_the_caller_can_act_on() {
        use crate::code::file_engine::FileEngineError as E;
        let escape = crate::code::path_security::PathSecurityError::TraversalBlocked {
            path: "C:/Temp/repro.py".into(),
            workspace: "C:/ws".into(),
        };
        assert!(matches!(engine_err(E::Security(escape)), CommandError::Denied(m) if m.contains("escapes workspace")));
        assert!(matches!(engine_err(E::NotFound("swe/x.py".into())), CommandError::NotFound(_)));
        assert!(matches!(engine_err(E::EditFailed("no match".into())), CommandError::Invalid(_)));
        assert!(matches!(engine_err(E::Io(std::io::Error::other("disk"))), CommandError::Internal(_)));
    }
    use crate::sdk_codegen::{AccessLevel, Ctx};
    use dashmap::DashMap;

    fn edit_call(v: serde_json::Value) -> Result<CodeEditParams, crate::sdk_codegen::ArgumentsRefusal> {
        crate::runtime::CommandRequest::<CodeEditParams>::decode(&v).map(|req| req.params)
    }

    fn refused_at(v: serde_json::Value) -> (String, crate::sdk_codegen::FieldProblem) {
        match edit_call(v) {
            Err(crate::sdk_codegen::ArgumentsRefusal::Field(refusal)) => (refusal.path.to_string(), refusal.problem),
            other => panic!("expected a field refusal, got {other:?}"),
        }
    }

    fn search_replace(v: serde_json::Value) -> (String, String, bool) {
        match edit_call(v).map(|p| p.edit_mode) {
            Ok(EditMode::SearchReplace { search, replace, all }) => (search, replace, all),
            other => panic!("expected a SearchReplace, got {other:?}"),
        }
    }

    // what this catches: the shape models emit most (Claude Code's Edit tool: `old_string`,
    // `new_string`, `replace_all`, inside `edit_mode` or flat beside `file_path`) refused, which
    // trades one failure class for another. Every spelling parses to the SAME SearchReplace as
    // the canonical names, and so does a bare mode name with its fields beside it.
    #[test]
    fn old_string_new_string_parse_to_the_same_search_replace_as_search_replace() {
        let canonical = search_replace(serde_json::json!({
            "file_path": "a.py", "edit_mode": {"type":"search_replace","search":"o","replace":"n","all":true}
        }));
        assert_eq!(canonical, ("o".to_string(), "n".to_string(), true));
        for call in [
            serde_json::json!({ "file_path": "a.py", "edit_mode": {"type":"search_replace","old_string":"o","new_string":"n","replace_all":true} }),
            serde_json::json!({ "file_path": "a.py", "old_string": "o", "new_string": "n", "replace_all": true }),
            serde_json::json!({ "file_path": "a.py", "search": "o", "replace": "n", "all": true }),
            serde_json::json!({ "file_path": "a.py", "edit_mode": "search_replace", "old_str": "o", "new_str": "n", "all": true }),
        ] {
            assert_eq!(search_replace(call.clone()), canonical, "{call}");
        }
        let p = edit_call(serde_json::json!({ "file_path": "a.py", "edit_mode": "append", "content": "Y" })).unwrap();
        assert!(matches!(p.edit_mode, EditMode::Append { content } if content == "Y"));
        let p = edit_call(serde_json::json!({ "file_path": "a.py", "line": 3, "content": "x" })).unwrap();
        assert!(matches!(p.edit_mode, EditMode::InsertAt { line: 3, .. }));
    }

    // what this catches: a flat or named shape accepted by GUESSING. Fields that name no single
    // mode, a partial mode, a flat field beside a tagged operation, or an unknown mode name are
    // each refused typed by the field at fault, never read as the nearest edit (the old
    // normalizer took `{end_line, new_content}` as "from line 1").
    #[test]
    fn a_shape_that_names_no_single_edit_is_refused_by_its_field() {
        let missing = |v: serde_json::Value| match refused_at(v) {
            (path, crate::sdk_codegen::FieldProblem::Missing { .. }) => path,
            (path, problem) => panic!("expected a missing field, got {path}: {problem:?}"),
        };
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py", "end_line": 603, "new_content": "x" })), "start_line");
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py", "new_content": "x" })), "start_line");
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py", "old_string": "o" })), "replace");
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py", "search": "o", "content": "x" })), "edit_mode");
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py" })), "edit_mode");
        let (path, problem) = refused_at(serde_json::json!({
            "file_path": "a.py", "edit_mode": {"type":"append","content":"X"}, "search": "o"
        }));
        assert_eq!(path, "search");
        assert!(matches!(problem, crate::sdk_codegen::FieldProblem::UnknownField { .. }), "{problem:?}");
        let (path, problem) = refused_at(serde_json::json!({ "file_path": "a.py", "edit_mode": "rewrite", "content": "x" }));
        assert!(matches!(problem, crate::sdk_codegen::FieldProblem::UnknownVariant { .. }), "{path}: {problem:?}");
        // A tagged operation keeps its own typed refusal and path through the flat reader.
        assert_eq!(missing(serde_json::json!({ "file_path": "a.py", "edit_mode": {"type":"search_replace","search":"o"} })), "edit_mode.replace");
    }

    // what this catches: a line range defaulted where she gave none. The old normalizer read a
    // missing start as line 1 and a missing end as u32::MAX, so `{end_line: 603, new_content}`
    // replaced from the top of the file; a range without both ends is now refused by name.
    #[test]
    fn a_line_range_without_both_ends_is_refused_naming_the_end() {
        let (path, problem) = refused_at(serde_json::json!({
            "file_path": "src/main.rs", "edit_mode": {"type": "line_range", "end_line": 65535, "new_content": "fn main() {}"}
        }));
        assert_eq!(path, "edit_mode.start_line");
        assert!(matches!(problem, crate::sdk_codegen::FieldProblem::Missing { .. }), "{problem:?}");
        let (path, _) = refused_at(serde_json::json!({
            "file_path": "src/main.rs", "edit_mode": {"type": "line_range", "start_line": 1, "new_content": "x"}
        }));
        assert_eq!(path, "edit_mode.end_line");
    }

    // what this catches: the edit tool advertised as `edit_mode: {allOf: [{$ref}], ...}`,
    // a shape llama.cpp's grammar converter cannot match a valid value against, so a
    // constrained model can never complete a code/edit call (from #5059, 2026-10-15; zero
    // resident edit calls after it on the M5).
    #[test]
    fn the_advertised_edit_schema_has_no_single_entry_all_of() {
        let schema = crate::sdk_codegen::input_schema::<CodeEditParams>().expect("the edit params schema serializes");
        let text = schema.to_string();
        assert!(!text.contains("\"allOf\""), "no allOf anywhere in the advertised edit schema: {text}");
        let edit_mode = &schema["properties"]["edit_mode"];
        assert_eq!(edit_mode["$ref"], "#/definitions/EditMode", "the operation is the EditMode ref itself: {edit_mode}");
        assert_eq!(edit_mode["description"], "Operation to apply.", "its doc survives the fold");
    }

    // what this catches: code/edit's PUBLISHED schema and its parser disagreeing. Every
    // alternative of the advertised `edit_mode` is built from the schema alone (its tag, and a
    // value of the declared type for each required field) and must decode; the manual's own
    // fill-in example must decode and land through the real executor.
    #[test]
    fn code_edit_published_schema_round_trips_through_its_parser() {
        let schema = crate::sdk_codegen::input_schema::<CodeEditParams>().expect("the edit params schema serializes");
        assert!(schema["required"].as_array().unwrap().contains(&serde_json::json!("edit_mode")));
        assert_eq!(schema["properties"].as_object().unwrap().len(), 3, "file_path, edit_mode, description");
        let alternatives = schema["definitions"]["EditMode"]["oneOf"].as_array().expect("EditMode is a oneOf");
        for alternative in alternatives {
            let mut op = serde_json::Map::new();
            for field in alternative["required"].as_array().unwrap() {
                let field = field.as_str().unwrap();
                let spec = &alternative["properties"][field];
                let value = match (spec["enum"].as_array(), spec["type"].as_str()) {
                    (Some(tags), _) => tags[0].clone(),
                    (None, Some("string")) => serde_json::json!("x"),
                    (None, Some("integer")) => serde_json::json!(1),
                    (None, Some("boolean")) => serde_json::json!(false),
                    other => panic!("an edit field of a type this test does not build: {other:?}"),
                };
                op.insert(field.to_string(), value);
            }
            let call = serde_json::json!({ "file_path": "a.rs", "edit_mode": op });
            edit_call(call.clone()).unwrap_or_else(|refusal| panic!("the schema's own alternative {call} is refused: {refusal}"));
        }

        let manual = crate::sdk_codegen::ai_help::render_ai_help(crate::sdk_codegen::command_descriptor("code/edit").unwrap());
        let (_, example) = crate::sdk_codegen::read_call(&manual).unwrap();
        let mut params = edit_call(serde_json::Value::Object(example)).expect("the manual's example decodes");
        // Fill the path blank; retain the generated numeric addresses unchanged and
        // exercise the real executor, which rejects zero-based line positions.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("example.txt"), "original\n").unwrap();
        params.file_path = "example.txt".into();
        let engine = crate::code::FileEngine::new("edit-help-fixture", crate::code::PathSecurity::new(dir.path()).unwrap());
        assert!(engine.edit(&params.file_path, &params.edit_mode, params.description.as_deref()).unwrap().success);
        assert_ne!(std::fs::read_to_string(dir.path().join("example.txt")).unwrap(), "original\n");
        for name in ["line_range", "search_replace", "insert_at", "append"] {
            assert!(manual.contains(name), "operation alternative missing: {name}");
        }
    }

    // what this catches: a refused edit rendering as a success. `code/edit` and `code/write`
    // projected no outcome, so `WriteResult { success: false }` took the success arm (Benchy,
    // the M5, 2026-11-05 on 0b2854327: one of five writes landed, every refusal ticked).
    #[test]
    fn a_refused_edit_projects_as_a_failure() {
        let refused = serde_json::json!({ "success": false, "file_path": "a.rs", "bytes_written": 0, "error": "no match" });
        let landed = serde_json::json!({ "success": true, "file_path": "a.rs", "bytes_written": 3 });
        for name in ["code/edit", "code/write"] {
            let (verdict, _) = crate::sdk_codegen::project_result(name, &refused);
            assert!(verdict.failed(), "{name}: a refused write is a failure, got {verdict:?}");
            let (verdict, _) = crate::sdk_codegen::project_result(name, &landed);
            assert!(verdict.succeeded(), "{name}: a landed write is a success, got {verdict:?}");
        }
    }

    // what this catches: the code/list glob-recovery (#160). A model that reflexively
    // passes a recursive glob (`**/*.rs`) to code/list must get the MATCHING FILES back
    // as a listing, not a bare NotFound it retries forever (mined: 38% of live calls).
    // Guards both halves: the glob DETECTION that triggers the reroute, and the
    // GlobResult→ListResult projection (each match a File entry, name = basename,
    // directory_path records the glob so the persona sees why it's cross-directory).
    #[test]
    fn code_list_recovers_a_glob_into_a_listing() {
        // detection: the reflexive recursive patterns route; a plain dir does not.
        assert!(looks_like_file_glob("**/*.rs"));
        assert!(looks_like_file_glob("src/**/*.toml"));
        assert!(!looks_like_file_glob("src"));
        assert!(!looks_like_file_glob("."));

        let glob = GlobResult {
            success: true,
            pattern: "**/*.rs".to_string(),
            matches: vec!["core/main.rs".to_string(), "lib.rs".to_string()],
            total_matches: 2,
            error: None,
        };
        let listing = list_result_from_glob("**/*.rs", glob);
        assert!(listing.success);
        assert_eq!(listing.total_count, 2);
        assert_eq!(
            listing.directory_path, "glob:**/*.rs",
            "records the glob provenance"
        );
        assert_eq!(listing.entries[0].name, "main.rs", "name is the basename");
        assert_eq!(
            listing.entries[0].path, "core/main.rs",
            "path stays workspace-relative"
        );
        assert!(matches!(listing.entries[0].kind, FsEntryKind::File));
    }

    // what this catches: a placeholder file_path (the 14B echoed `<path_to_blueprints.py>` instead
    // of the real path it had seen) fails LOUD with guidance toward the concrete path — while a real
    // path passes untouched (no false positives on ordinary filenames).
    #[test]
    fn code_edit_rejects_placeholder_paths_but_passes_real_ones() {
        for ph in [
            "<path_to_blueprints.py>",
            "path_to_file.py",
            "/path/to/x.py",
            "<file>",
            "your_file.rs",
        ] {
            assert!(
                reject_placeholder_path(ph).is_err(),
                "should reject placeholder: {ph}"
            );
        }
        for real in [
            "src/flask/blueprints.py",
            "core/continuum-core/src/lib.rs",
            "a.py",
            "example.py",
        ] {
            assert!(
                reject_placeholder_path(real).is_ok(),
                "should accept real path: {real}"
            );
        }
    }

    // what this catches: code/search auto-detects a glob-shaped `pattern` (the misuse a local
    // model makes — `**/*.py` as the grep term, glass-boxed looping 12× on it) so it can list
    // FILES instead of grepping contents for the literal glob → 0 → loop. Must NOT misclassify a
    // genuine content regex, or real greps would be silently redirected. Regression for the
    // flask SWE 0-edit search-loop.
    #[test]
    fn glob_shaped_search_patterns_detected_content_regexes_spared() {
        for g in [
            "**/*.py",
            "*.rs",
            "src/**/*.js",
            "**/blueprints.py",
            "*.{rs,py}",
        ] {
            assert!(
                looks_like_file_glob(g),
                "should be treated as a file glob: {g}"
            );
        }
        for r in [
            "Blueprint",
            "foo.*bar",
            "fn .*Params",
            "self.name = name",
            "TODO",
            "raise ValueError",
        ] {
            assert!(
                !looks_like_file_glob(r),
                "must NOT be treated as a glob (real content pattern): {r}"
            );
        }
    }

    // Caller limits bound search work; the executor, not a private row-count
    // threshold, owns window-aware delivery. Every admitted row must reach it.
    #[tokio::test]
    async fn search_preserves_caller_bounded_matches_for_shared_delivery() {
        let dir = std::env::temp_dir().join(format!("search-overflow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // hot.py: 47 hits; warm.py: 5; cold.py: 2 — 54 total, uneven on purpose.
        std::fs::write(dir.join("hot.py"), "needle\n".repeat(47)).unwrap();
        std::fs::write(dir.join("warm.py"), "needle\n".repeat(5)).unwrap();
        std::fs::write(dir.join("cold.py"), "needle\n".repeat(2)).unwrap();

        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert(
            "local-owner".to_string(),
            FileEngine::new("local-owner", security),
        );
        let state = Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
            Arc::new(crate::test_acting::TestActing),
        ));
        let cmd = CodeSearch { state };
        let out = cmd
            .run(
                &Ctx::default(),
                CodeSearchParams {
                    pattern: "needle".to_string(),
                    file_glob: None,
                    max_results: None,
                },
            )
            .await
            .expect("search runs");

        assert_eq!(out.total_matches, 54);
        assert_eq!(out.matches.len(), 54, "no hidden per-file compression");
        assert_eq!(out.matches.iter().filter(|m| m.file_path.contains("hot.py")).count(), 47);
        assert!(out.error.is_none(), "successful results contain no behavioral directions");
        for limit in [1, 25, 26, 100] {
            let bounded = cmd.run(&Ctx::default(), CodeSearchParams {
                pattern: "needle".into(),
                file_glob: Some("hot.py".into()),
                max_results: Some(limit),
            }).await.expect("bounded search runs");
            assert_eq!(bounded.matches.len(), (limit as usize).min(47));
            assert!(bounded.error.is_none());
        }

        // Small search stays raw — no false compression.
        let small = cmd
            .run(
                &Ctx::default(),
                CodeSearchParams {
                    pattern: "needle".to_string(),
                    file_glob: Some("cold.py".to_string()),
                    max_results: None,
                },
            )
            .await
            .expect("small search runs");
        assert_eq!(small.total_matches, 2);
        assert_eq!(small.matches.len(), 2, "under threshold keeps every line");
        assert!(small.error.is_none(), "no note on a small result");

        // Regression: the real command must retain search_files' parse failures,
        // while a valid empty glob still returns successful orientation guidance.
        for (pattern, glob, expected_error) in [
            ("(", None, "Invalid regex"),
            ("needle", Some("["), "Invalid glob"),
        ] {
            let refused = cmd
                .run(
                    &Ctx::default(),
                    CodeSearchParams {
                        pattern: pattern.to_string(),
                        file_glob: glob.map(str::to_string),
                        max_results: None,
                    },
                )
                .await
                .expect("search refusal is outcome data");
            assert!(!refused.success, "invalid syntax must not become empty success");
            assert!(refused.matches.is_empty());
            assert!(refused.error.as_deref().unwrap().contains(expected_error));
        }
        let empty = cmd
            .run(
                &Ctx::default(),
                CodeSearchParams {
                    pattern: "needle".to_string(),
                    file_glob: Some("*.missing".to_string()),
                    max_results: None,
                },
            )
            .await
            .expect("valid empty search runs");
        assert!(empty.success, "orientation guidance is not a parse refusal");
        assert_eq!(empty.files_searched, 0);
        assert!(empty.matches.is_empty());
        assert!(empty.error.as_deref().unwrap().contains("No files matched glob"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a code/list miss on a NONEXISTENT path (the "empty workspace"
    // confabulation source — a persona reached for `src/persona.rs` where no top-level
    // `src` exists, got a bare NotFound, and concluded the workspace was empty) must
    // TEACH the real top-level layout instead of a contentless error. The persona then
    // sees `apps, core, docs` in the very error and self-corrects. // regression: live
    // 2026-07-13 empty-workspace confabulation
    #[test]
    fn list_miss_teaches_the_real_top_level_layout() {
        let dir = std::env::temp_dir().join(format!("list-miss-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("apps")).unwrap();
        std::fs::create_dir_all(dir.join("core")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let engine = FileEngine::new("local-owner", security);

        let err = teach_layout_on_miss(&engine, "src/persona.rs", "no such file or directory");
        let msg = err.to_string();
        assert!(
            msg.contains("apps") && msg.contains("core") && msg.contains("docs"),
            "enumerates the real top-level dirs so the persona self-corrects: {msg}"
        );
        assert!(
            msg.contains("src/persona.rs"),
            "names what was actually missed: {msg}"
        );
        assert!(
            msg.contains("don't assume source lives under `src/`"),
            "carries the same anti-assumption teaching as the workspace-map: {msg}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a code/list GLOB that matches zero files (the live `**/`
    // idiom — a persona reaching for "everything") must not read as an empty
    // workspace. The empty listing carries a note naming the real top-level dirs +
    // pointing at code/tree, so the persona doesn't confabulate emptiness.
    // regression: live 2026-07-14 `code/list(path=**/)` → empty
    #[tokio::test]
    async fn zero_match_glob_note_names_the_layout_not_emptiness() {
        let dir = std::env::temp_dir().join(format!("zero-glob-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("apps")).unwrap();
        std::fs::create_dir_all(dir.join("core")).unwrap();
        std::fs::write(dir.join("core/main.rs"), "fn main() {}").unwrap();
        let security = PathSecurity::new(&dir).expect("temp subdir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert(
            "local-owner".to_string(),
            FileEngine::new("local-owner", security),
        );
        let state = Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
            Arc::new(crate::test_acting::TestActing),
        ));
        let cmd = CodeList { state };

        // A glob that matches no files → empty entries + a teaching note.
        let out = cmd
            .run(
                &Ctx::default(),
                CodeListParams {
                    path: Some("**/*.nonexistent".to_string()),
                    include_hidden: None,
                },
            )
            .await
            .expect("a zero-match glob is not an error");
        assert!(out.entries.is_empty(), "no files match the glob");
        let note = out.error.expect("zero-match glob carries a teaching note");
        assert!(
            note.contains("NOT an empty workspace"),
            "corrects the confabulation: {note}"
        );
        assert!(
            note.contains("apps") && note.contains("core"),
            "names the real dirs: {note}"
        );
        assert!(
            note.contains("code/tree"),
            "points at the recursive view: {note}"
        );

        // A glob that DOES match keeps working (no false note).
        let hit = cmd
            .run(
                &Ctx::default(),
                CodeListParams {
                    path: Some("**/*.rs".to_string()),
                    include_hidden: None,
                },
            )
            .await
            .expect("glob runs");
        assert!(!hit.entries.is_empty(), "the .rs glob finds main.rs");
        assert!(hit.error.is_none(), "a matching glob carries no note");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `CodeState` whose `local-owner` engine (the `Ctx::default` caller) is rooted
    /// at the OS temp dir, so read-only commands run without touching the repo or
    /// depending on the process cwd. Pre-seeding the engine makes `ensure_engine`
    /// short-circuit on `contains_key`, so no `current_dir` lookup happens.
    fn state_rooted_at_temp() -> Arc<CodeState> {
        let security = PathSecurity::new(&std::env::temp_dir()).expect("temp dir is a valid root");
        let file_engines = Arc::new(DashMap::new());
        file_engines.insert(
            "local-owner".to_string(),
            FileEngine::new("local-owner", security),
        );
        Arc::new(CodeState::new(
            file_engines,
            Arc::new(DashMap::new()),
            tokio::runtime::Handle::current(),
            Arc::new(crate::test_acting::TestActing),
        ))
    }

    // what this catches: the four file-op hands migrated off the legacy persona_id
    // arms (#62) reach the persona tool surface under their canonical names and the
    // AiSafe gating a workspace-mutating-but-undoable hand warrants (same class as
    // code/write). A wrong NAME (persona can't find the tool) or a drifted ACCESS
    // (security-boundary regression) is caught here.
    #[test]
    fn migrated_file_ops_are_aisafe_named_hands() {
        assert_eq!(CodeDelete::NAME, "code/delete");
        assert_eq!(CodeDiff::NAME, "code/diff");
        assert_eq!(CodeUndo::NAME, "code/undo");
        assert_eq!(CodeHistory::NAME, "code/history");
        for access in [
            CodeDelete::ACCESS,
            CodeDiff::ACCESS,
            CodeUndo::ACCESS,
            CodeHistory::ACCESS,
        ] {
            assert!(
                matches!(access, AccessLevel::AiSafe),
                "file-op hands are AiSafe"
            );
        }
    }

    // what this catches: code/history runs caller-scoped (Ctx::default → local-owner)
    // through the lazy engine and returns a typed, well-formed HistoryResult — an
    // empty history on a fresh workspace — proving the migrated run path + identity
    // scoping work end to end WITHOUT a persona_id param. A regression to the legacy
    // param shape (or a panic on the empty change graph) is caught.
    #[tokio::test]
    async fn history_on_fresh_workspace_is_typed_and_empty() {
        let cmd = CodeHistory {
            state: state_rooted_at_temp(),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                CodeHistoryParams {
                    file_path: None,
                    limit: None,
                },
            )
            .await
            .expect("history never errors on a fresh workspace");
        assert!(out.success, "fresh history is a success");
        assert!(out.nodes.is_empty(), "no changes recorded yet");
        let json = serde_json::to_value(&out).unwrap();
        assert!(json["nodes"].is_array(), "nodes is the wire array");
        assert!(
            json["total_count"].is_number(),
            "total_count present on the wire"
        );
    }

    // what this catches: the four hands are contributed to the module object map via
    // command_objects — a regression that drops one (the descriptor still registers,
    // but the persona has no runtime object to route to) is caught.
    #[tokio::test]
    async fn command_objects_includes_the_file_op_hands() {
        let names: Vec<&str> = command_objects(state_rooted_at_temp())
            .iter()
            .map(|c| c.name())
            .collect();
        for n in ["code/delete", "code/diff", "code/undo", "code/history"] {
            assert!(
                names.contains(&n),
                "command_objects missing {n}; got {names:?}"
            );
        }
    }

    // what this catches: the citizen-layer workspace policy (Joel 2026-07-10:
    // "each persona is only the diff from the shared"). An IDENTIFIED peer's
    // engine must NEVER root at the core's cwd — the live incident: a persona's
    // misdirected [dependencies] edit replaced the repo-root Cargo.toml through
    // the shared-cwd default, breaking every build path. The peer's root is a
    // CoW clone under <CONTINUUM_HOME>/citizens/peers/<peer>/workspace, seeded
    // from the base, durable across calls; the anonymous local operator keeps
    // cwd. regression for commit 3ad97cc2e (the manifest-clobber repair).
    #[tokio::test]
    async fn identified_peer_roots_at_citizen_layer_never_shared_cwd() {
        // Scoped homes so the test never touches the real ~/.continuum. A tiny
        // base dir keeps the CoW clone instant.
        //
        // The base is passed EXPLICITLY via `ensure_citizen_layer_from_base` — this
        // test used to `set_current_dir(base)` to steer the clone source, but process
        // cwd is global: while it was pointed at this temp dir, every parallel ts-rs
        // `export_bindings_*` test resolved its relative `export_to` against it and
        // panicked trying to write outside the tree. Injecting the base keeps the
        // clone exercised end-to-end with zero global-cwd mutation. #191.
        let base = tempfile::tempdir().expect("base");
        std::fs::write(base.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        // The base is a git repo with one commit and one IGNORED cache dir — the
        // shape the shared clone must NOT carry (2026-09-06: 4.5 GB of ignored model
        // caches rode into every citizen copy).
        std::fs::write(base.path().join(".gitignore"), "tools/models/\n").unwrap();
        std::fs::create_dir_all(base.path().join("tools/models")).unwrap();
        std::fs::write(base.path().join("tools/models/weights.bin"), "big").unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["add", "-A"],
            vec!["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "base"],
        ] {
            let ok = std::process::Command::new("git")
                .current_dir(base.path())
                .args(args)
                .status()
                .expect("git")
                .success();
            assert!(ok);
        }
        let home = tempfile::tempdir().expect("home");
        // Under the one crate-wide home lock (#4082): this unlocked CONTINUUM_HOME write
        // raced every other test that resolves the citizen layer.
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
        std::env::set_var("CONTINUUM_HOME", home.path());

        let peer = "test-peer-1234";
        let layer = ensure_citizen_layer_from_base(peer, base.path(), &crate::code::git_actor::GitActor::citizen(uuid::Uuid::nil(), Some("Tester"))).await.expect("layer provisions");
        assert!(
            layer.starts_with(home.path()),
            "layer lives under CONTINUUM_HOME: {layer:?}"
        );
        assert_ne!(
            layer.canonicalize().unwrap(),
            base.path().canonicalize().unwrap(),
            "peer layer is NOT the shared base"
        );
        assert!(
            layer.join("Cargo.toml").is_file(),
            "layer is seeded from the base (shared clone)"
        );
        let alternates = std::fs::read_to_string(layer.join(".git/objects/info/alternates"))
            .expect("a shared clone names the base's objects");
        assert!(
            alternates.trim().ends_with(".git/objects"),
            "objects come from the base, never copied: {alternates}"
        );
        assert!(
            !layer.join("tools/models").exists(),
            "an ignored cache in the base never rides into the layer"
        );
        let precious = std::process::Command::new("git")
            .current_dir(base.path())
            .args(["config", "--get", "extensions.preciousObjects"])
            .output()
            .expect("git config");
        assert_eq!(
            String::from_utf8_lossy(&precious.stdout).trim(),
            "true",
            "the base is protected from gc the moment a layer borrows its objects (review on #3795)"
        );
        // A peer WRITE lands in the layer, never the base.
        std::fs::write(layer.join("Cargo.toml"), "[dependencies]\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(base.path().join("Cargo.toml")).unwrap(),
            "[workspace]\n",
            "the shared base is untouched by layer writes"
        );
        // Idempotent: second call reuses the existing layer (her diff survives).
        let again = ensure_citizen_layer_from_base(peer, base.path(), &crate::code::git_actor::GitActor::citizen(uuid::Uuid::nil(), Some("Tester"))).await.expect("layer reused");
        assert_eq!(again, layer);
        assert_eq!(
            std::fs::read_to_string(again.join("Cargo.toml")).unwrap(),
            "[dependencies]\n",
            "the peer's divergence is durable"
        );

        // (CONTINUUM_HOME is restored by the guard's drop)
    }
