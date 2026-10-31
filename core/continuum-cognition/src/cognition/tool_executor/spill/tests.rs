#![cfg(test)]
    use super::*;

    // what this catches (Codex on #4982): the restore retain loop untested, a restored
    // handle that does not resolve to the exact bytes the mind saw, a failed save that drops
    // the text or leaves a pin, and a new file visible to eviction before it is pinned.
    #[test]
    fn a_restore_saves_handleless_results_exactly_and_pins_them_until_published() {
        let tmp = std::env::temp_dir().join(format!("spill-restore-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let mut results = vec![
            ("first result\nline two".to_string(), None),
            ("already saved".to_string(), Some("feedfacefeedface".to_string())),
            ("third result".to_string(), None),
        ];
        let pin = LivePin::new();
        retain_handleless_with(|t| retain_in(&tmp, persona, t, &pin), results.iter_mut().map(|(t, h)| (t.as_str(), h)));
        for (text, handle) in [&results[0], &results[2]] {
            let handle = handle.as_deref().expect("a handle-less result is saved");
            assert_eq!(std::fs::read_to_string(resolve_in(&tmp, persona, handle).unwrap()).unwrap(), *text, "byte-identical");
        }
        assert_eq!(results[1].1.as_deref(), Some("feedfacefeedface"), "a saved result keeps its handle");
        let live: std::collections::HashSet<String> = live_handles().lock().get(&pin.0).cloned().unwrap_or_default();
        assert!(results.iter().filter(|(t, _)| t != "already saved").all(|(_, h)| live.contains(h.as_ref().unwrap())), "each is pinned as it is saved");
        let holder = pin.0;
        drop(pin);
        assert!(!live_handles().lock().contains_key(&holder), "the pin withdraws once the memory has published");

        // A root that is a FILE: every save fails, and the text stays whole with no handle.
        let blocked = std::env::temp_dir().join(format!("spill-blocked-{}", Uuid::new_v4()));
        std::fs::write(&blocked, b"not a directory").unwrap();
        let mut failed = vec![("kept whole".to_string(), None::<String>)];
        let failing = LivePin::new();
        retain_handleless_with(|t| retain_in(&blocked, persona, t, &failing), failed.iter_mut().map(|(t, h)| (t.as_str(), h)));
        assert_eq!(failed[0], ("kept whole".to_string(), None), "a failed save keeps its text and claims no handle");
        assert!(live_handles().lock().get(&failing.0).is_none_or(|set| set.is_empty()), "and pins nothing");

        // THE ORDER (Codex on #4982): the handle is live before its file exists, under the lock
        // eviction reads, so no evictor can see the new file unpinned.
        let ordered = LivePin::new();
        let handle = handle_of("pinned first");
        assert!(ordered.hold(&handle), "entered");
        let path = persona_dir(&tmp, persona).join(format!("{handle}.log"));
        assert!(!path.exists() && live_handles().lock().values().any(|set| set.contains(&handle)), "live before written");
        assert!(matches!(retain_in(&tmp, persona, "pinned first", &ordered), ResultCustody::Whole { .. }) && path.exists());

        // THE INTERLEAVING (Codex on #4982): a content-addressed file that already exists is
        // never deleted while pinned. Eviction decides and deletes in one critical section, so a
        // pin cannot enter between them: inside it, the lock a pin needs is held.
        use crate::paging::filesystem_tier_pool::Removal;
        assert!(matches!(evict_unless_live(&path), Removal::Protected) && path.exists(), "pinned: protected");
        drop(ordered);
        let removal = evict_unless_live_with(&path, || {
            assert!(live_handles().try_lock().is_none(), "no pin can enter between the check and the delete");
        });
        assert!(matches!(removal, Removal::Removed) && !path.exists(), "unpinned: removed");
        let again = LivePin::new();
        assert!(matches!(retain_in(&tmp, persona, "pinned first", &again), ResultCustody::Whole { .. }) && path.exists(), "a pin after the delete rewrites the file");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_file(&blocked);
    }

    // what this catches: any fragment at execution (the head+tail fold, the head cut). A small
    // and a large result both come back Complete with the whole text, saved byte-identical
    // under a resolvable handle; a failed save still carries the whole with the honest notice.
    #[test]
    fn a_result_is_saved_whole_and_handed_back_whole() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let large = format!("HEAD-MARK\n{}TAIL-MARK error: boom\n", "compiling module\n".repeat(5000));
        for body in ["ok: 2 passed".to_string(), large] {
            let Fitted::Complete { text, handle } = whole_or_handle_in(&tmp, body.clone(), persona, &LivePin::new())
            else {
                panic!("a saved result is complete")
            };
            assert_eq!(text, body, "handed back byte-identical");
            let path = resolve_in(&tmp, persona, &handle).expect("resolve");
            assert_eq!(std::fs::read_to_string(path).unwrap(), body, "and saved byte-identical");
        }

        // A failed save keeps the WHOLE result and says why: nothing else holds those bytes.
        let blocked = tmp.join("not-a-dir");
        std::fs::write(&blocked, b"file where the persona dir must go").unwrap();
        let unsaved_body = "UNSAVED-HEAD ".repeat(100);
        let Fitted::RetentionFailed { shown } =
            whole_or_handle_in(&blocked, unsaved_body.clone(), persona, &LivePin::new())
        else {
            panic!("a failed save is its own state")
        };
        assert!(shown.contains("saving this") && shown.ends_with(&unsaved_body), "kept whole: {shown}");
        assert!(shown.contains("cannot be reopened"), "{shown}");
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: a multi-call batch that one call's handle claimed to cover (Astra
    // on #4963). The batch's rendered text, two distinct results inside it, is saved as one
    // unit and resolves byte-identical, both results included.
    #[test]
    fn a_batch_is_retained_whole_both_results_byte_identical() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let batch = "code/read(a.rs)\nResult:\nfn a() {}\n\ncode/run(t)\nResult:\nerror[E0601]\n";
        let ResultCustody::Whole { handle } = retain_in(&tmp, persona, batch, &LivePin::new()) else {
            panic!("the batch is saved")
        };
        let saved = std::fs::read_to_string(resolve_in(&tmp, persona, &handle).unwrap()).unwrap();
        assert_eq!(saved, batch, "both results, byte-identical");
        let blocked = tmp.join("not-a-dir");
        std::fs::write(&blocked, b"x").unwrap();
        assert_eq!(retain_in(&blocked, persona, batch, &LivePin::new()), ResultCustody::Unsaved);
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: eviction erasing evidence a mind still points at (Astra on #4963).
    // A published handle is live; withdrawing the holder releases it; a path that is not a
    // spill file is never mistaken for one.
    #[test]
    fn a_referenced_handle_is_live_until_its_holder_withdraws() {
        let holder = u64::MAX - 7;
        let handle = format!("{:016x}", Uuid::new_v4().as_u128() as u64);
        let path = std::path::PathBuf::from(format!("/spill/p/{handle}.log"));
        assert!(!is_live(&path));
        publish_live_handles(holder, [handle.clone()].into_iter().collect());
        assert!(is_live(&path));
        withdraw_live_handles(holder);
        assert!(!is_live(&path));
        assert!(!is_live(std::path::Path::new("/")));
    }

    // what this catches: a round-trip through the root-injected core — spill
    // writes a hex-handled file, resolve points back at it, and the content is
    // byte-identical. Uses a tempdir so it never touches ~/.continuum (task #7).
    #[test]
    fn spill_then_resolve_round_trips() {
        let tmp = std::env::temp_dir().join(format!("spill-test-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let body = "line one\nline two\nthe error is here\n";
        let r = LivePin::new().save_in(&tmp, persona, body).expect("spill");
        assert_eq!(r.handle.len(), 16);
        assert!(r.handle.chars().all(|c| c.is_ascii_hexdigit()));
        let path = resolve_in(&tmp, persona, &r.handle).expect("resolve");
        assert_eq!(path, r.path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
        std::fs::remove_dir_all(&tmp).ok();
    }

    // what this catches: the path-traversal guard. A handle with a slash, a
    // `..`, or a non-hex char must be rejected so a persona can never escape its
    // own spill directory by crafting a handle.
    #[test]
    fn resolve_rejects_non_hex_handles() {
        let tmp = std::env::temp_dir().join("spill-guard");
        let persona = Uuid::new_v4();
        for bad in ["../secret", "a/b", "..", "deadbeefZZ", ""] {
            assert!(
                resolve_in(&tmp, persona, bad).is_err(),
                "must reject handle {bad:?}"
            );
        }
        // a well-formed hex handle is accepted (existence is the reader's concern)
        assert!(resolve_in(&tmp, persona, "deadbeefcafe0000").is_ok());
    }

    // what this catches: grep finds the failure line and carries context around
    // it — the core "find the error in the flood" affordance. The matched line
    // gets the `>` gutter; a neighbor rides along as context.
    #[test]
    fn investigate_greps_the_error_with_context() {
        let body = (1..=200)
            .map(|n| {
                if n == 137 {
                    "error[E0432]: unresolved import".to_string()
                } else {
                    format!("noise line {n}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, Some("error\\["), 1, None, None).expect("ok");
        assert_eq!(inv.total_matches, 1);
        assert!(inv.rendered.contains("error[E0432]"));
        assert!(
            inv.rendered.contains(">   137"),
            "matched line is gutter-marked"
        );
        assert!(
            inv.rendered.contains("136"),
            "context line above is present"
        );
        assert_eq!(inv.total_lines, 200);
    }

    // what this catches: a bad regex fails LOUD naming the pattern, never a
    // silent empty result that reads as "no error found".
    #[test]
    fn investigate_bad_regex_is_a_loud_error() {
        let err = investigate("x", Some("("), 0, None, None).unwrap_err();
        assert!(err.contains("invalid search pattern"));
    }

    // what this catches: with neither pattern nor range, a private tail size (the old 80
    // lines) deciding what she sees. The whole output returns; her view sizes it, with the
    // handle that pages the rest (HER-TOOLS: the view is the only sizer).
    #[test]
    fn investigate_without_a_selector_returns_the_whole_output() {
        let body = (1..=500)
            .map(|n| format!("row {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, None, 0, None, None).expect("ok");
        assert!(inv.rendered.contains("     1 row 1\n") && inv.rendered.contains("row 500"));
        assert!(!inv.result_truncated && inv.next_byte_offset.is_none());
    }

    // what this catches: an explicit line range reads exactly that slice,
    // 1-based and clamped to the file.
    #[test]
    fn investigate_reads_an_explicit_range() {
        let body = (1..=100)
            .map(|n| format!("L{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, None, 0, Some((10, 12)), None).expect("ok");
        assert!(inv.rendered.contains("    10 L10"));
        assert!(inv.rendered.contains("    12 L12"));
        assert!(!inv.rendered.contains("L13"));
        assert!(!inv.rendered.contains("L9\n"));
    }

    // what this catches: her `max_matches` dropping the whole-file count or the place to
    // continue from; the cap is her selection, never a default.
    #[test]
    fn max_matches_selects_without_losing_the_count_or_the_continuation() {
        let body = (1..=10000)
            .map(|n| format!("matchme {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inv = investigate(&body, Some("matchme"), 0, None, Some(3)).expect("ok");
        assert_eq!((inv.total_matches, inv.rendered.lines().count()), (10000, 3));
        assert!(inv.result_truncated);
        assert_eq!(inv.next_byte_offset, body.find("matchme 4\n").map(|at| at as u64));
    }
    // what this catches: independent cursors retain one opened source across
    // publication at the same path, and all continue from their own byte offsets.
    #[test]
    fn positioned_investigation_keeps_one_source_across_reuse() {
        use std::io::Read;
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let handle = "deadbeef";
        let path = write_in(dir.path(), persona, handle, "original\n").unwrap();
        let file = fs::File::open(&path).unwrap();
        let mut first = ExtentReader {
            file: &file,
            offset: 0,
            end: 9,
        };
        let mut second = ExtentReader {
            file: &file,
            offset: 0,
            end: 9,
        };
        let mut bytes = [0; 3];
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ori");
        write_in(dir.path(), persona, handle, "original\n").unwrap();
        assert_eq!(
            write_in(dir.path(), persona, handle, "replacement\n")
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
        second.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"ori");
        first.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"gin");
        assert_eq!(fs::read_to_string(&path).unwrap(), "original\n");
    }

    // what this catches: a huge line blocking a small range, or a regex refused on it: every
    // line is searched whole.
    #[test]
    fn long_unselected_lines_are_counted_without_materialization() {
        let body = format!("{}\n{}", "x".repeat(10000), "short\n".repeat(100));
        let range = investigate(&body, None, 0, Some((2, 3)), None).unwrap();
        assert_eq!(range.total_lines, 101);
        assert_eq!(range.total_bytes, body.len());
        assert!(range.rendered.contains("     2 short"));
        let whole = investigate(&body, None, 0, None, None).unwrap();
        assert!(whole.rendered.contains("short"));
        assert_eq!(investigate(&body, Some("x"), 0, None, None).unwrap().total_matches, 1);
    }

    // what this catches: the existing pin/remover contract protects staging as
    // well as published bytes, including publication followed by a holder release.
    #[test]
    fn staging_and_published_objects_share_the_existing_pin() {
        use crate::paging::filesystem_tier_pool::Removal;
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let parent = persona_dir(dir.path(), persona);
        fs::create_dir_all(&parent).unwrap();
        let handle = "deadbeef";
        let pin = LivePin::new();
        pin.hold(handle);
        let staging = parent.join(format!("{handle}.{}", Uuid::new_v4()));
        fs::write(&staging, "retained").unwrap();
        assert!(matches!(evict_unless_live(&staging), Removal::Protected));
        let published = parent.join(format!("{handle}.log"));
        continuum_host::durable::publish_new(&staging, &published).unwrap();
        let _ = fs::remove_file(&staging);
        assert!(matches!(evict_unless_live(&published), Removal::Protected));
        drop(pin);
        let _ = evict_unless_live(&published);
        assert!(!published.exists());
    }

    // Regression for #5036: a failed publisher leaves visible bytes, not a
    // durability receipt. Actual spill reuse must independently complete the fence.
    #[test]
    fn visible_publication_requires_a_successful_reuse_fence() {
        let dir = tempfile::tempdir().unwrap();
        let persona = Uuid::new_v4();
        let body = "retained output\n";
        let handle = handle_of(body);
        let pin = LivePin::new();
        pin.hold(&handle);
        let path = persona_dir(dir.path(), persona).join(format!("{handle}.log"));
        continuum_host::durable::fault_injection::with_failed_fence(|| {
            assert!(write_in(dir.path(), persona, &handle, body).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), body);
            // The object is already visible; byte equality must not bypass the fence.
            assert!(write_in(dir.path(), persona, &handle, body).is_err());
        });
        assert_eq!(write_in(dir.path(), persona, &handle, body).unwrap(), path);
    }

    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // Competing writers must converge on the same complete immutable object,
        // and neither publisher may leave a staging object behind.
        #[test]
        fn concurrent_duplicate_publishers_reuse_one_object() {
            let dir = tempfile::tempdir().unwrap();
            let persona = Uuid::new_v4();
            let content = "shared result\n".repeat(8192);
            let handle = handle_of(&content);
            let pin = LivePin::new();
            pin.hold(&handle);
            let ready = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let publish = || {
                    ready.wait();
                    write_in(dir.path(), persona, &handle, &content).unwrap()
                };
                let first = scope.spawn(publish);
                let second = scope.spawn(publish);
                let path = first.join().unwrap();
                assert_eq!(path, second.join().unwrap());
                assert_eq!(fs::read_to_string(path).unwrap(), content);
            });
            assert_eq!(fs::read_dir(persona_dir(dir.path(), persona)).unwrap().count(), 1);
        }
    }
