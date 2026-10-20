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

    // what this catches: a path argument wearing markdown — backticks, quotes, or the next
    // line of a rendered receipt — resolving to a DIFFERENT address than the bare path
    // (card 1daffbaf: `utils.py\`` created a second file beside utils.py on the 5090).
    #[test]
    fn a_path_argument_wearing_markdown_resolves_to_the_bare_path_and_names_what_went() {
        use StrippedIdiom::*;
        let cases: &[(&str, &str, &[StrippedIdiom])] = &[
            ("src/main.ts", "src/main.ts", &[]),
            ("  src/main.ts ", "src/main.ts", &[]),
            ("`src/main.ts`", "src/main.ts", &[Backticks]),
            ("src/main.ts`", "src/main.ts", &[Backticks]),
            ("\"src/main.ts\"", "src/main.ts", &[Quotes]),
            ("'src/main.ts'", "src/main.ts", &[Quotes]),
            ("`\"src/main.ts\"`", "src/main.ts", &[Backticks, Quotes]),
            // The 5090 shape verbatim: fence, then the receipt line she was reading.
            (
                "src/main.ts`\nsuccess: ✗",
                "src/main.ts",
                &[TrailingLines, Backticks],
            ),
            ("src/main.ts\r\n", "src/main.ts", &[]),
            ("`", "", &[Backticks]),
        ];
        for (raw, want, stripped) in cases {
            let got = clean_path_arg(raw);
            assert_eq!(got.path, *want, "raw {raw:?}");
            assert_eq!(got.stripped, stripped.to_vec(), "raw {raw:?}");
            // Idempotent: the engine cleans first and the resolver cleans again.
            let again = clean_path_arg(&got.path);
            assert_eq!(again.path, got.path);
            assert!(
                again.stripped.is_empty(),
                "second pass over {raw:?} stripped again"
            );
        }
        assert!(clean_path_arg("src/main.ts").note().is_empty());
        let note = clean_path_arg("src/main.ts`\nsuccess: ✗").note();
        assert!(note.contains("the text after the line break") && note.contains("backticks"));
        assert!(
            note.contains("'src/main.ts'"),
            "the note names where it landed: {note}"
        );

        // And through the gate itself: the fenced spelling and the bare one are ONE address,
        // for a read of an existing file and for a write of a new one.
        let (_dir, security) = setup_workspace();
        assert_eq!(
            security.validate_read("`src/main.ts`").unwrap(),
            security.validate_read("src/main.ts").unwrap()
        );
        assert_eq!(
            security.validate_write("src/new.ts`\nsuccess: ✗").unwrap(),
            security.validate_write("src/new.ts").unwrap()
        );
    }

    #[test]
    fn test_valid_read() {
        let (_dir, security) = setup_workspace();
        let result = security.validate_read("src/main.ts");
        assert!(result.is_ok());
    }

    // what this catches (Kimi, 2026-10-06): a shell-trusted caller refused a read of a file
    // outside her workspace that she could `cat`. Host scope reads an existing absolute path
    // outside every root; Sandbox still refuses it; neither widens a relative traversal.
    #[test]
    fn host_scope_reads_outside_the_sandbox_and_only_absolute_paths() {
        let (_dir, security) = setup_workspace();
        let outside = tempfile::tempdir().unwrap();
        let file = outside.path().join("pr.diff");
        fs::write(&file, "diff").unwrap();
        let abs = file.to_string_lossy().to_string();
        assert!(security.validate_read_in(&abs, ReadScope::Sandbox).is_err());
        assert_eq!(security.validate_read_in(&abs, ReadScope::Host { deny: Vec::new() }).unwrap(), file.canonicalize().unwrap());
        assert!(security.validate_read_in("../../etc/passwd", ReadScope::Host { deny: Vec::new() }).is_err());
        assert!(security.validate_write(&abs).is_err(), "writes never widen");
    }

    // what this catches (Joel 2026-10-18 "keep creds off-limits"): a resident's file verbs,
    // which reach the whole host, reading or writing a credential root: directly, through a
    // link she names, or a scope's signing key inside her own workspace.
    #[cfg(unix)]
    #[test]
    fn host_scope_refuses_every_credential_path_however_it_is_named() {
        let (dir, security) = setup_workspace();
        let outside = tempfile::tempdir().unwrap();
        let ssh = outside.path().join(".ssh");
        fs::create_dir_all(&ssh).unwrap();
        fs::write(ssh.join("id"), "secret").unwrap();
        fs::write(outside.path().join("notes.txt"), "hers").unwrap();
        std::os::unix::fs::symlink(ssh.join("id"), outside.path().join("innocent")).unwrap();
        fs::create_dir_all(dir.path().join(".airc")).unwrap();
        fs::write(dir.path().join(".airc/identity.key"), "key").unwrap();
        let scope = || ReadScope::Host { deny: vec![ssh.canonicalize().unwrap()] };
        let at = |p: &std::path::Path| p.to_string_lossy().to_string();
        assert!(security.validate_read_in(&at(&outside.path().join("notes.txt")), scope()).is_ok(), "the rest of the host is hers");
        assert!(security.validate_read_in(&at(&ssh.join("id")), scope()).is_err(), "a credential root");
        assert!(security.validate_read_in(&at(&outside.path().join("innocent")), scope()).is_err(), "a link to one");
        assert!(security.validate_read_in(".airc/identity.key", scope()).is_err(), "a scope key inside her own workspace");
        assert!(security.validate_write_in(&at(&outside.path().join("new.txt")), &scope()).is_ok(), "she writes where she names");
        assert!(security.validate_write_in(&at(&ssh.join("authorized_keys")), &scope()).is_err(), "never into a credential root");
        assert!(security.validate_write_in(&at(&outside.path().join("innocent")), &scope()).is_err(), "nor through a link into one");
    }

    // what this catches (card d598c806): a citizen whose shell is confined reading, by
    // path, what her shell cannot: Confined scope reads an absolute path inside her roots
    // and refuses one outside them BY NAME (a security refusal naming the workspace, not
    // an ENOENT), while a relative traversal never widens.
    #[test]
    fn confined_scope_reads_her_roots_and_refuses_the_operators_by_name() {
        use crate::code::confinement::AllowList;
        let (_dir, security) = setup_workspace();
        let hers = tempfile::tempdir().unwrap();
        let theirs = tempfile::tempdir().unwrap();
        let mine = hers.path().join("notes.txt");
        let yours = theirs.path().join("resume.txt");
        fs::write(&mine, "mine").unwrap();
        fs::write(&yours, "yours").unwrap();
        let list = AllowList {
            write: vec![hers.path().canonicalize().unwrap()],
            read: vec![],
            deny: vec![],
            listable: vec![],
        };
        let scope = ReadScope::Confined(list);
        let mine_s = mine.to_string_lossy().to_string();
        let yours_s = yours.to_string_lossy().to_string();
        assert_eq!(security.validate_read_in(&mine_s, scope.clone()).unwrap(), mine.canonicalize().unwrap());
        match security.validate_read_in(&yours_s, scope.clone()) {
            Err(PathSecurityError::TraversalBlocked { path, .. }) => assert_eq!(path, yours_s),
            other => panic!("the operator's file must be refused by name, got {other:?}"),
        }
        assert!(security.validate_read_in("../../etc/passwd", scope).is_err(), "a traversal never widens");
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
        for ext in &[
            "swift", "kt", "cpp", "m", "go", "java", "ts", "rs", "py", "plist",
        ] {
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
    // what this catches: the absolute-path double-join (2026-08-23, fluid-sim exam):
    // a persona addressing her own file by the absolute path the system showed her
    // had it silently slash-stripped into a deep RELATIVE path and written to
    // <root>/var/…/<root>/index.html — graded "she never wrote it". Inside-root
    // absolutes rebase to relative; outside-root absolutes refuse LOUD; the
    // "/index.html" workspace-relative idiom keeps working.
    fn absolute_paths_rebase_inside_refuse_outside_keep_the_slash_idiom() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let sec = PathSecurity::new(root).expect("security over tempdir");
        let inside = format!("{}/index.html", root.display());
        let resolved = sec.resolve_for_write(&inside).expect("inside-root absolute resolves");
        assert!(resolved.ends_with("index.html"));
        let canon_root = root.canonicalize().expect("canon");
        assert!(resolved.starts_with(&canon_root) || resolved.starts_with(root),
            "must land under the root, never double-joined: {}", resolved.display());
        assert_eq!(resolved.to_string_lossy().matches("index.html").count(), 1, "double-join would repeat the filename");
        // deep subdir form too (the exam's actual shape)
        let deep = format!("{}/assets/app.js", root.display());
        let r2 = sec.resolve_for_write(&deep).expect("inside-root deep absolute resolves");
        assert!(r2.ends_with("assets/app.js"));
        // outside-root genuine absolute → loud refusal, never a silent deep write
        let err = sec.resolve_for_write("/var/folders/somewhere/else.html");
        assert!(matches!(err, Err(PathSecurityError::TraversalBlocked { .. })),
            "outside-root absolute must refuse, got {err:?}");
        // the forgiving idiom survives: "/index.html" = workspace-relative
        let idiom = sec.resolve_for_write("/index.html").expect("slash idiom resolves");
        assert!(idiom.ends_with("index.html"));
    }

    // what this catches: shell /c paths failing native tools, without granting
    // access to sibling workspaces or allowing writes through traversal.
    #[cfg(windows)]
    #[test]
    fn windows_shell_paths_keep_workspace_boundaries() {
        let (dir, security) = setup_workspace();
        let root = dir.path().canonicalize().expect("root");
        let native = root.to_string_lossy().replace('\\', "/");
        let native = native.strip_prefix("//?/").unwrap_or(&native);
        let shell = format!("/{}{}", native[..1].to_ascii_lowercase(), &native[2..]);
        std::fs::write(root.join("shell.txt"), "receipt").expect("file");
        assert_eq!(security.validate_read(&format!("{shell}/shell.txt")).expect("shell read"), root.join("shell.txt"));
        assert_eq!(security.resolve_for_write(&format!("{shell}/new.txt")).expect("shell write"), root.join("new.txt"));
        assert!(security.resolve_for_write(&format!("{shell}/../escape.txt")).is_err());
        let outside = tempfile::tempdir().expect("outside");
        let outside_file = outside.path().join("outside.txt");
        std::fs::write(&outside_file, "outside").expect("outside file");
        assert!(security.validate_read(&outside_file.to_string_lossy()).is_err());
        assert!(security.resolve_for_write(&outside_file.to_string_lossy()).is_err());
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
