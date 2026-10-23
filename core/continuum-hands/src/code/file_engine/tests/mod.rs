#![cfg(test)]
    mod path_recovery {
        use super::super::suggest_nearest_paths;
        use std::fs;

        // what this catches: a mistyped path returning a dead end. Glass-boxed
        // 2026-08-28 on the five no-candidate-patch solve runs — django-15252
        // burned 30 acts, found the RIGHT file on act 1, and still wrote nothing
        // because 3 of its 8 reads were paths that do not exist. Both real
        // misses are pinned here: a missing directory segment
        // (django/db/backends/creation.py -> .../base/creation.py) and a
        // singular/plural slip (migration/ -> migrations/). Also pins the
        // "operate on people's computers" bounds: heavy directories are never
        // walked, so an error path can never stall someone's machine.
        #[test]
        fn a_mistyped_path_gets_pointed_at_the_real_one() {
            let tmp = std::env::temp_dir().join(format!("fe-suggest-{}", uuid::Uuid::new_v4()));
            let deep = tmp.join("django/db/backends/base");
            fs::create_dir_all(&deep).unwrap();
            fs::write(deep.join("creation.py"), "x").unwrap();
            let mig = tmp.join("django/core/management/commands");
            fs::create_dir_all(&mig).unwrap();
            fs::write(mig.join("migrate.py"), "x").unwrap();

            // The real miss: she dropped the `base/` segment.
            let hits = suggest_nearest_paths(&tmp, "django/db/backends/creation.py");
            assert!(
                hits.iter().any(|h| h.ends_with("base/creation.py")),
                "same filename elsewhere in the tree must be offered, got {hits:?}"
            );

            // The other real miss: `migration/` for `migrations/`.
            let hits = suggest_nearest_paths(&tmp, "django/migration/commands/migrate.py");
            assert!(
                hits.iter().any(|h| h.ends_with("commands/migrate.py")),
                "the correct migrate.py must be offered, got {hits:?}"
            );

            // A name nothing resembles yields nothing — never noise.
            assert!(suggest_nearest_paths(&tmp, "totally/unrelated/zzzz.py").is_empty());

            // OPERATE ON PEOPLE'S COMPUTERS: heavy dirs are not walked, so a
            // file hiding in node_modules/.git is never even considered.
            let heavy = tmp.join("node_modules/pkg");
            fs::create_dir_all(&heavy).unwrap();
            fs::write(heavy.join("recorder.py"), "x").unwrap();
            let hits = suggest_nearest_paths(&tmp, "django/db/migrations/recorder.py");
            assert!(
                !hits.iter().any(|h| h.contains("node_modules")),
                "node_modules must never be walked for suggestions, got {hits:?}"
            );

            let _ = fs::remove_dir_all(&tmp);
        }
    }


    use super::*;
    use std::fs;

    // what this catches: the silent misplaced edit. A persona addressed a line she had
    // never seen numbered (`code/shell cat` has no line numbers) and dropped a guard clause
    // into a function's parameter list; the receipt said `success: true` and nothing else,
    // so her next turn had no way to know (M5 glass-box, 2026-08-04, SWE-bench flask-4045).
    // The receipt must carry the numbered landing site so a wrong line becomes a visible
    // fact in working memory. Pins: line-addressed edits report the neighborhood with the
    // anchor marked, and a whole-file write reports None (no landing site to speak of).
    #[test]
    fn a_line_addressed_edit_reports_where_it_actually_landed() {
        let (_dir, engine) = setup_engine();

        let inserted = engine
            .edit(
                "src/main.ts",
                &EditMode::InsertAt {
                    line: 2,
                    content: "INSERTED\n".to_string(),
                },
                None,
            )
            .expect("insert");
        let ctx = inserted
            .applied_context
            .expect("a line-addressed edit must report its landing site");
        assert!(
            ctx.contains("INSERTED"),
            "the receipt must show the text that landed, got:\n{ctx}"
        );
        assert!(
            ctx.contains(">    2 |"),
            "the anchor line must be marked so the landing site is unmissable, got:\n{ctx}"
        );
        assert!(
            ctx.contains("line 1") && ctx.contains("line 3"),
            "surrounding lines are the whole point — they are what reveals a guard clause \
             dropped into a parameter list, got:\n{ctx}"
        );

        // A whole-file write replaces everything; there is no landing site to report, and
        // inventing one would be noise in a small working-memory window.
        let written = engine.write("src/other.ts", "a\nb\n", None).expect("write");
        assert!(
            written.applied_context.is_none(),
            "creating a NEW file destroys nothing and needs no warning"
        );
    }

    // what this catches: THE measured failure, identical in three SWE-bench runs on the M5,
    // 2026-08-04 — a guard clause placed inside an open `def __init__(`, via insert_at and
    // via search/replace, six edits across twelve acts, with the file read back in between.
    // The numbered-line receipt (59efe1ecd) could not surface it: the closing paren is still
    // there so brackets stay balanced, and six lines of plausible Python say nothing about
    // the grammar. Only a parser does.
    //
    // Each assertion gets its OWN file. The first version of this test appended valid code
    // to the file the first assertion had just broken, then asserted no warning — it failed
    // for that reason and nearly cost a correct implementation.
    #[test]
    fn an_edit_that_would_break_the_parse_is_refused_and_changes_nothing() {
        let (dir, engine) = setup_engine();
        let signature = "def f(\n    a,\n    b,\n):\n    return a\n";
        fs::write(dir.path().join("src/broken.py"), signature).unwrap();
        fs::write(dir.path().join("src/clean.py"), signature).unwrap();

        // The exact shape from the runs: a statement inside the parameter list.
        let broken = engine
            .edit(
                "src/broken.py",
                &EditMode::InsertAt {
                    line: 2,
                    content: "    if '.' in a:\n        raise ValueError(\"no dots\")\n"
                        .to_string(),
                },
                None,
            )
            .expect("a refusal is a RESULT, not a transport error");
        assert!(
            !broken.success,
            "an edit that makes a parsing file unparseable must be refused, not applied"
        );
        let err = broken.error.expect("a refusal must say why");
        assert!(
            err.contains("EDIT REFUSED") && err.contains("UNCHANGED"),
            "the refusal must name itself and state the file was left alone, got:\n{err}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("src/broken.py")).unwrap(),
            signature,
            "REFUSED means byte-for-byte unchanged — the whole point is that no wreckage \
             is left behind for a later edit to stack on"
        );
        assert!(
            broken.change_id.is_none(),
            "nothing was written, so there must be no change to undo"
        );

        // A valid edit on an INTACT file must stay quiet, or the warning is noise.
        let clean = engine
            .edit(
                "src/clean.py",
                &EditMode::Append {
                    content: "\ndef g():\n    return 1\n".to_string(),
                },
                None,
            )
            .expect("clean edit");
        let ctx = clean.applied_context.expect("append reports its tail");
        assert!(
            !ctx.contains("SYNTAX ERROR"),
            "valid Python must not warn, got:\n{ctx}"
        );
    }

    // what this catches: repair-forward-on-rubble. flask-4045 run 5 (M5, 2026-08-04): an
    // early edit dropped a guard clause into `super().__init__(`'s argument list; four acts
    // later she worked out the right idiom and applied a CORRECT content-anchored fix — on
    // top of the still-present wreckage, so the file never parsed and the task failed. Across
    // five runs she repaired forward every time and never reverted once. Pins the two facts
    // she cannot compute from a SyntaxError alone: whether THIS edit is what broke a
    // previously-working file, and the change_id that reverses it.
    #[test]
    fn an_edit_on_an_already_broken_file_says_the_damage_predates_it() {
        let (dir, engine) = setup_engine();
        // Damage that did NOT come from an edit — the engine refuses those now. This is what a
        // whole-file `code/write` (deliberately ungated) or an outside change leaves behind.
        fs::write(
            dir.path().join("src/first.py"),
            "def f(\n    a,\n    if a:\n):\n    return a\n",
        )
        .unwrap();

        // An edit that neither causes nor fixes the break must say the damage PREDATES it.
        // Claiming otherwise would send her to undo her own innocent change and leave the real
        // damage in place — the trap run 5 fell into.
        let again = engine
            .edit(
                "src/first.py",
                &EditMode::Append {
                    content: "\ndef g():\n    return 1\n".to_string(),
                },
                None,
            )
            .expect("an edit on an already-broken file still applies");
        assert!(
            again.success,
            "the gate only fires when the file PARSED before"
        );
        let ctx = again.applied_context.expect("landing site");
        assert!(
            ctx.contains("STILL BROKEN") && ctx.contains("already here before"),
            "an edit on an already-broken file must say the damage predates it, got:\n{ctx}"
        );
    }

    // what this catches: THE upstream defect behind the whole "statement inside an open
    // paren" family (SWE-bench flask-4045 runs 1, 3, 5 on the M5, 2026-08-04). code/edit
    // addresses lines by NUMBER; code/read returned bare text. She read the whole 128-line
    // file, asked for insert_at line 35, and landed 4 lines off — inside
    // `super().__init__(`'s argument list — because she had to count by hand. Pins that a
    // what this catches: a dead-end error on the path the refusal STEERS her onto. The edit
    // gate tells her to "anchor on the text itself with a search_replace edit"; if that then
    // says only "Search text not found", the redirect lands nowhere. The dominant real cause is
    // invisible in a quoted string — leading whitespace — so the miss must NAME it.
    #[test]
    fn a_search_miss_names_the_indent_mismatch_and_the_line() {
        let content = "def f():\n    if len(univariate) == 1:\n        return 1\n";
        // She reproduced the line without its indent — the classic miss.
        let report = search_miss_report(content, "if len(univariate) == 1:");
        assert!(
            report.contains("INDENT mismatch") && report.contains("line 2"),
            "must localise the line and name the cause, got:\n{report}"
        );
        assert!(
            report.contains("4 space(s)"),
            "must state the actual indent so she can reproduce it, got:\n{report}"
        );
        assert!(
            report.contains("line_range edit on line 2"),
            "and offer the escape that does not depend on reproducing whitespace, got:\n{report}"
        );
    }

    // what this catches: a genuinely absent anchor must still orient her — pointing at the
    // nearest real line is what distinguishes "you mis-remembered" from "the file changed".
    #[test]
    fn a_search_miss_with_no_whitespace_match_points_at_the_nearest_line() {
        let content = "def solve(gens, basis):\n    if len(univariate) == 2:\n        pass\n";
        let report = search_miss_report(content, "if len(univariate) == 99:");
        assert!(
            report.contains("closest line is 2"),
            "must point at something she can look at, got:\n{report}"
        );
        // And an anchor sharing nothing must NOT invent a neighbour.
        let unrelated = search_miss_report(content, "completely unrelated banana text");
        assert!(
            unrelated.contains("Nothing in this file resembles it"),
            "a bogus 'closest' is worse than admitting no match, got:\n{unrelated}"
        );
    }

    // what this catches: the WHITESPACE-EXACT DEATH of a correct multi-line edit — the
    // live shape from benchy-sympy-22840-n4 (2026-08-08). She anchored the exact fix site
    // (3 lines, cse_main.py 674-676) but reproduced them with the indent dropped; the edit
    // died "Nothing in this file resembles it" and she burned her act budget recovering.
    // A UNIQUE trimmed-lines match is unambiguous intent: apply it, re-based onto the
    // file's real indentation, preserving the replacement's relative structure.
    #[test]
    fn a_unique_whitespace_normalized_multiline_match_applies_with_the_files_indent() {
        let content = "def tree_cse():\n\
                       \x20   for expr in exprs:\n\
                       \x20       if isinstance(orig_expr, MatrixExpr):\n\
                       \x20           sym = MatrixSymbol(sym.name, orig_expr.rows,\n\
                       \x20               orig_expr.cols)\n\
                       \x20       return sym\n";
        let edit = EditMode::SearchReplace {
            // Her live idiom: right text, no indent.
            search: "if isinstance(orig_expr, MatrixExpr):\n\
                     sym = MatrixSymbol(sym.name, orig_expr.rows,\n\
                     orig_expr.cols)"
                .to_string(),
            replace: "if isinstance(orig_expr, MatrixExpr):\n\
                      \x20   sym = sym"
                .to_string(),
            all: false,
        };
        let out = apply_edit(content, &edit).expect("unique normalized match must apply");
        assert!(
            out.contains("        if isinstance(orig_expr, MatrixExpr):\n            sym = sym\n"),
            "replacement must carry the FILE's indent (8) and its own relative +4, got:\n{out}"
        );
        assert!(
            !out.contains("MatrixSymbol"),
            "the matched block must be gone, got:\n{out}"
        );
        assert!(
            out.contains("        return sym"),
            "surrounding lines must survive untouched, got:\n{out}"
        );
    }

    // what this catches: the fallback must never GUESS between sites. Two normalized
    // matches → refuse with the line numbers, file untouched — a whitespace-blind edit
    // applied to the wrong one of two candidates is strictly worse than the old dead end.
    #[test]
    fn an_ambiguous_whitespace_normalized_match_refuses_with_line_numbers() {
        // Two same-shaped blocks at DIFFERENT indents — the multi-line search is not a
        // verbatim substring of either (so the exact path misses), and trimmed-lines
        // matching finds both.
        let content = "class A:\n    def f(self):\n        return 1\n\
                       class B:\n        def f(self):\n            return 1\n";
        let edit = EditMode::SearchReplace {
            search: "def f(self):\nreturn 1".to_string(),
            replace: "def f(self):\nreturn 2".to_string(),
            all: false,
        };
        let err = apply_edit(content, &edit).expect_err("two candidate sites must refuse");
        let msg = format!("{err}");
        assert!(
            msg.contains("2 locations") && msg.contains("lines 2, 5"),
            "must count and locate the candidates, got:\n{msg}"
        );
    }

    // what this catches (2026-09-21, Sahar on pydata__xarray-7393): an exact anchor that
    // occurs in two functions was applied to the FIRST by `replacen(.., 1)` — the hunk
    // landed in `reset_index` instead of `stack`. Verbatim ambiguity now refuses with the
    // line numbers, exactly as the whitespace-normalized path does; a unique anchor and
    // `all: true` are unchanged.
    #[test]
    fn an_ambiguous_exact_anchor_refuses_with_line_numbers_instead_of_taking_the_first() {
        let content = "def reset_index(self):\n    idx = self.idx\n    return idx\n\n\
                       def stack(self):\n    idx = self.idx\n    return idx\n";
        let ambiguous = EditMode::SearchReplace {
            search: "    idx = self.idx\n    return idx\n".to_string(),
            replace: "    idx = self.idx\n    product_vars = {}\n    return idx\n".to_string(),
            all: false,
        };
        let err = apply_edit(content, &ambiguous).expect_err("two verbatim sites must refuse");
        let msg = format!("{err}");
        assert!(msg.contains("2 locations") && msg.contains("lines 2, 6"), "count + lines, got:\n{msg}");
        // A unique anchor (one more line of context) applies to the site it names.
        let unique = EditMode::SearchReplace {
            search: "def stack(self):\n    idx = self.idx\n".to_string(),
            replace: "def stack(self):\n    product_vars = {}\n    idx = self.idx\n".to_string(),
            all: false,
        };
        let out = apply_edit(content, &unique).expect("a unique anchor applies");
        assert!(out.contains("def stack(self):\n    product_vars = {}"));
        assert!(out.starts_with("def reset_index(self):\n    idx = self.idx\n"), "reset_index untouched");
        // all=true is the explicit every-site edit and still applies everywhere.
        let every = EditMode::SearchReplace { search: "return idx".to_string(), replace: "return idx  # both".to_string(), all: true };
        assert_eq!(apply_edit(content, &every).expect("all=true").matches("# both").count(), 2);
    }

    // what this catches: the fallback must not swallow the genuinely-absent case — the
    // teaching miss report (nearest line / nothing resembles) is still the answer there.
    #[test]
    fn no_normalized_match_still_falls_through_to_the_miss_report() {
        let content = "def solve():\n    return 1\n";
        let edit = EditMode::SearchReplace {
            search: "completely unrelated banana text".to_string(),
            replace: "x".to_string(),
            all: false,
        };
        let err = apply_edit(content, &edit).expect_err("absent anchor still fails");
        assert!(
            format!("{err}").contains("SEARCH TEXT NOT FOUND"),
            "the miss report remains the terminal answer, got:\n{err}"
        );
    }

    // what this catches: the ORPHANED TAIL — a line_range whose end falls inside a construct
    // it does not close. sympy-22005 (M5, 2026-08-04, agent/solve): she asked for lines
    // 240..247 of `polysys.py`, but the block she meant ends at 249 because 248-249 are the
    // tail of a triple-quoted string. Her replacement carried its own `'''))`, so the
    // original's last two lines survived after it and the module stopped parsing. The FIX
    // itself was reasonable — semantics right, extent wrong, score zero.
    //
    // Bracket balance cannot catch this and was falsified for the insert case; only a parser
    // can. The gate is the parser, and refusing keeps the correct-ish attempt recoverable
    // instead of burying it under a syntax error she never reverts.
    // what this catches: the refusal pointing at the WRONG cause. Live on sympy-21379 —
    // she read basic.py, pasted the numbered output straight back as whole-file content, so
    // every line carried the `   12 | ` gutter and Python saw `IndentationError: unexpected
    // indent`. The gate correctly saved the file, then handed her the line-range advice
    // ("widen the range to cover the whole construct") for an edit that used no line range.
    // She spent the remaining 16 of 30 acts chasing that wrong lead and never landed an edit.
    // The gutter detector already existed; it was only wired to the write SUCCESS path.
    // A refusal must name the cause it can actually prove, and must not lie about damage it
    // prevented. [[a-probe-that-can-only-fail-is-worse-than-no-probe]]
    // what this catches: the missing import — valid syntax, guaranteed NameError, invisible
    // to the parse gate. Live on sympy-21379, her FIRST edit to real library source all
    // session, and the entire library change was two lines:
    //     def _subs(self, old, new, **hints):
    //   +     clear_cache()
    // `clear_cache` is imported nowhere in basic.py. Every substitution in sympy raised
    // NameError; passToPass went 40 -> 26, fourteen passing tests destroyed, and the edit
    // reported success: true.
    //
    // Also pins the SAFETY clause, which matters more than the catch: only names the edit
    // INTRODUCED are flagged. A name the old file already mentions is left alone — that is
    // what keeps star-imports and conditional definitions from producing false refusals.
    // what this catches: the docstring that silently stops being one. Same 2-line
    // sympy-21379 edit as the missing-import test — she inserted between `def` and the
    // docstring, so ast.get_docstring(_subs) went None. Valid syntax; the parse gate is blind
    // to it; help() loses the text and any doctests inside it stop being collected (in sympy
    // those run as part of the suite).
    //
    // WARNING not refusal, on purpose: a NameError is a guaranteed crash and earns a refusal,
    // a displaced docstring is degradation the code survives. Severity matches force — a gate
    // that cries wolf is a gate that gets ignored.
    #[test]
    fn an_insert_above_a_docstring_is_reported_as_displacing_it() {
        let dir = tempfile::tempdir().expect("tmp");
        let f = dir.path().join("m.py");
        let old = "def a():\n    \"\"\"Docs.\"\"\"\n    return 1\n";
        let displaced = "def a():\n    x = 1\n    \"\"\"Docs.\"\"\"\n    return 1\n";

        match displaced_docstrings(&f, old, displaced) {
            None => return, // no python3 — inconclusive is not a verdict
            Some(w) => {
                assert!(w.contains("a"), "names the function: {w}");
                assert!(w.contains("docstring"), "says what was lost: {w}");
                assert!(
                    w.contains("AFTER the closing quotes"),
                    "says exactly what to do about it: {w}"
                );
            }
        }

        // Inserting BELOW the docstring is fine — the common, correct edit must stay silent.
        assert!(
            displaced_docstrings(
                &f,
                old,
                "def a():\n    \"\"\"Docs.\"\"\"\n    x = 1\n    return 1\n"
            )
            .is_none(),
            "a correct insert must not warn"
        );
        // A function that never had a docstring is not 'displaced'.
        assert!(
            displaced_docstrings(
                &f,
                "def b():\n    return 1\n",
                "def b():\n    x=1\n    return 1\n"
            )
            .is_none(),
            "no docstring to lose"
        );
    }

    #[test]
    fn an_edit_that_calls_an_unimported_name_is_refused_not_silently_broken() {
        let dir = tempfile::tempdir().expect("tmp");
        let f = dir.path().join("m.py");
        let old = "def a():\n    return 1\n";

        // introduces a call to a name the file has never heard of
        let bad = "def a():\n    clear_cache()\n    return 1\n";
        let hit = introduced_undefined_calls(&f, old, bad);
        match hit {
            None => return, // no python3 here — an inconclusive probe is not a verdict
            Some(names) => assert!(
                names.iter().any(|n| n == "clear_cache"),
                "must name the undefined call: {names:?}"
            ),
        }

        // builtins are fine
        assert!(
            introduced_undefined_calls(&f, old, "def a():\n    return len([1])\n").is_none(),
            "builtins are not undefined names"
        );
        // imported in the same edit → fine
        assert!(
            introduced_undefined_calls(
                &f,
                old,
                "from x import clear_cache\ndef a():\n    clear_cache()\n"
            )
            .is_none(),
            "a name imported by the same edit is bound"
        );
        // ALREADY MENTIONED in the old file → never flagged, even if unbound in the new text
        assert!(
            introduced_undefined_calls(
                &f,
                "# clear_cache lives in sympy.core.cache\ndef a():\n    return 1\n",
                bad
            )
            .is_none(),
            "a name the old file already mentions is out of scope — this is the anti-false-refusal clause"
        );
    }

    #[test]
    fn a_refused_gutter_paste_is_told_about_the_gutter_not_a_line_range() {
        let pasted = "     1 | def f():\n     2 |     return 1\n     3 | \n     4 | x = f()\n";
        let hint = numbered_paste_refusal(pasted);
        assert!(hint.contains("NNN | "), "names the gutter itself: {hint}");
        assert!(hint.contains("strip"), "says what to DO about it: {hint}");
        assert!(
            !hint.contains("undo") && !hint.to_lowercase().contains("now corrupt"),
            "the refusal path SAVED the file — claiming damage would be a lie: {hint}"
        );
        // Ordinary code with a bit-or must not trip it.
        assert!(
            numbered_paste_refusal("fn a() {}\nlet t = 1 | 2;\nfn b() {}\n").is_empty(),
            "real code is not a numbered paste"
        );
    }

    #[test]
    fn a_line_range_ending_inside_a_triple_quoted_string_is_refused() {
        let (dir, engine) = setup_engine();
        // Lines 1..8, with the string closing on 8 — the sympy shape, minus the distance.
        let original = "def solve(gens, basis):\n\
                        \x20   univariate = [b for b in basis]\n\
                        \x20   if len(univariate) == 1:\n\
                        \x20       f = univariate.pop()\n\
                        \x20   else:\n\
                        \x20       raise NotImplementedError('''\n\
                        \x20           only zero-dimensional systems supported\n\
                        \x20           (finite number of solutions)\n\
                        \x20           ''')\n";
        fs::write(dir.path().join("src/polysys.py"), original).unwrap();

        // End at 7 — one line INTO the string. Her replacement closes the quote itself, so
        // lines 8-9 are left dangling exactly as they were on the real run.
        let refused = engine
            .edit(
                "src/polysys.py",
                &EditMode::LineRange {
                    start_line: 3,
                    end_line: 7,
                    new_content: "    if len(univariate) == 1 and len(gens) == 1:\n\
                                  \x20       f = univariate.pop()\n\
                                  \x20   else:\n\
                                  \x20       raise NotImplementedError('''\n\
                                  \x20           only zero-dimensional systems supported\n\
                                  \x20           ''')"
                        .to_string(),
                },
                None,
            )
            .expect("a refusal is a RESULT, not a transport error");

        assert!(!refused.success, "the orphaned tail must not be written");
        assert_eq!(
            fs::read_to_string(dir.path().join("src/polysys.py")).unwrap(),
            original,
            "the file must survive byte-for-byte so her next attempt starts from working code"
        );
        let err = refused.error.expect("a refusal must say why");
        // THE AIM ASSIST: not "widen the range" — the NUMBER. She already made this judgement
        // once and got it wrong; repeating the instruction is not help. The construct closes at
        // line 9, so the refusal must say end_line=9 and confirm the same content then parses.
        assert!(
            err.contains("THE FIX: use end_line=9 instead of 7"),
            "the refusal must compute and name the end line that works, got:\n{err}"
        );
        assert!(
            err.contains("stopped 2 line(s) short"),
            "and quantify how far off she was, got:\n{err}"
        );

        // And the SAME edit with the range widened to cover the whole construct must APPLY —
        // otherwise the gate would just be blocking the fix rather than correcting the aim.
        let ok = engine
            .edit(
                "src/polysys.py",
                &EditMode::LineRange {
                    start_line: 3,
                    end_line: 9,
                    new_content: "    if len(univariate) == 1 and len(gens) == 1:\n\
                                  \x20       f = univariate.pop()\n\
                                  \x20   else:\n\
                                  \x20       raise NotImplementedError('''\n\
                                  \x20           only zero-dimensional systems supported\n\
                                  \x20           ''')"
                        .to_string(),
                },
                None,
            )
            .expect("edit");
        assert!(
            ok.success,
            "a correctly-bounded edit must still apply — the gate rejects breakage, not intent"
        );
        assert!(
            fs::read_to_string(dir.path().join("src/polysys.py"))
                .unwrap()
                .contains("len(gens) == 1"),
            "the widened edit lands the fix"
        );
    }

    // read shows the coordinate system an edit requires, and that the gutter matches
    // `applied_context`'s (`{n:>6} | `), so one file reads the same before and after.
    #[test]
    fn a_read_shows_the_line_numbers_an_edit_addresses() {
        let (dir, engine) = setup_engine();
        fs::write(
            dir.path().join("src/counted.txt"),
            "alpha\nbravo\ncharlie\n",
        )
        .unwrap();

        let whole = engine.read("src/counted.txt", None, None).expect("read");
        let content = whole.content.expect("content");
        assert!(
            content.contains("     1 | alpha") && content.contains("     3 | charlie"),
            "a full read numbers every line, got:\n{content}"
        );

        // A WINDOWED read must number by ABSOLUTE file position, not 1..n of the slice —
        // a relative number is worse than none, because it looks addressable and is not.
        let window = engine
            .read("src/counted.txt", Some(2), Some(3))
            .expect("read");
        let content = window.content.expect("content");
        assert!(
            content.contains("     2 | bravo") && !content.contains("     1 | bravo"),
            "a windowed read numbers from the file's start, not the window's, got:\n{content}"
        );
    }

    // what this catches: the hazard the numbering itself creates. Read a file, hand the
    // numbered text back to code/write, and every line carries a gutter that was never in
    // the source — a corrupt file behind `success: true`. Pins that the write receipt names
    // it, and that ordinary code (which has no gutter) never trips the warning.
    #[test]
    fn writing_read_output_back_verbatim_is_named_in_the_receipt() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/round.py"), "a = 1\nb = 2\nc = 3\n").unwrap();
        let numbered = engine
            .read("src/round.py", None, None)
            .expect("read")
            .content
            .expect("content");

        let pasted = engine
            .write("src/round.py", &numbered, None)
            .expect("the write still applies — we report, never refuse");
        let ctx = pasted
            .applied_context
            .expect("an overwrite always reports something");
        assert!(
            ctx.contains("line-number gutter"),
            "pasting read output back must be named as such, got:\n{ctx}"
        );

        // Real source has no gutter — the warning must stay silent or it is noise.
        let clean = engine
            .write("src/round.py", "a = 1\nb = 2\nc = 3\n", None)
            .expect("clean write");
        let ctx = clean.applied_context.expect("overwrite reports magnitude");
        assert!(
            !ctx.contains("line-number gutter"),
            "ordinary source must not trip the gutter warning, got:\n{ctx}"
        );
    }

    // what this catches: the stale-line-number defect that produced the orphaned parameter
    // tail on SWE-bench flask-4045, run 4 (M5, 2026-08-04). She read the file, edited
    // lines 16..17 replacing 2 lines with 4, then addressed lines 14..23 using the numbers
    // from the READ — two short, because the first edit had pushed everything down by 2.
    // Her second block was correct; it just landed against a map that had moved and nothing
    // told her. Pins both directions: a growing edit says how far the lines below it slid,
    // and an edit that does NOT change the line count stays silent so the notice keeps
    // meaning something.
    #[test]
    fn an_edit_that_changes_the_line_count_says_the_numbers_below_it_moved() {
        let (dir, engine) = setup_engine();
        let five = "one\ntwo\nthree\nfour\nfive\n";
        fs::write(dir.path().join("src/grew.txt"), five).unwrap();
        fs::write(dir.path().join("src/same.txt"), five).unwrap();

        let grew = engine
            .edit(
                "src/grew.txt",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 3,
                    new_content: "a\nb\nc\nd".to_string(),
                },
                None,
            )
            .expect("edit applies");
        let ctx = grew
            .applied_context
            .expect("a line-addressed edit reports its landing site");
        assert!(
            ctx.contains("LINE NUMBERS SHIFTED") && ctx.contains("+2"),
            "replacing 2 lines with 4 must state the +2 slide of everything below, got:\n{ctx}"
        );

        // Same line count in, same line count out — nothing downstream moved, so a notice
        // here would be noise on every ordinary in-place edit.
        let same = engine
            .edit(
                "src/same.txt",
                &EditMode::LineRange {
                    start_line: 2,
                    end_line: 3,
                    new_content: "a\nb".to_string(),
                },
                None,
            )
            .expect("edit applies");
        let ctx = same.applied_context.expect("landing site");
        assert!(
            !ctx.contains("LINE NUMBERS SHIFTED"),
            "an edit that preserves the line count invalidates nothing, got:\n{ctx}"
        );
    }

    // what this catches: the destructive clobber. On the M5, 2026-08-04, a persona had a
    // CORRECT 2-line fix in src/flask/blueprints.py (128 lines), then called code/write
    // with a 5-line stub reconstructed from memory — imports gone, class hierarchy gone,
    // her own fix gone — and the receipt said `success: true, bytes_written: 214`. She read
    // the file afterward and never repaired it. Of every file verb, whole-file write can
    // destroy the most, and it was the one reporting the least. Pins: an overwrite states
    // what it replaced, a large shrink is called out by name, and creating a new file stays
    // quiet (asserted above) so the warning means something when it appears.
    #[test]
    fn overwriting_an_existing_file_reports_what_it_destroyed() {
        let (_dir, engine) = setup_engine();

        // 3 lines -> 1: the shape that erased a 128-line module.
        let clobbered = engine
            .write("src/main.ts", "just this\n", None)
            .expect("overwrite");
        let ctx = clobbered
            .applied_context
            .expect("an overwrite of real content must report the loss");
        assert!(
            ctx.contains("3 line(s) replaced by 1"),
            "the magnitude of the loss is the fact, got:\n{ctx}"
        );
        assert!(
            ctx.contains("REMOVED most of the file"),
            "a large shrink must be named, not left for the reader to compute, got:\n{ctx}"
        );
        assert!(
            ctx.contains("code/undo"),
            "the receipt must point at the recovery path, got:\n{ctx}"
        );
    }

    /// The flask-4045 shape, minimised: a class whose docstring is long enough that an edit
    /// anchored on prose lands inside the literal. `guard` is the fix she actually derived.
    fn blueprint_module() -> &'static str {
        "class Blueprint:\n    \"\"\"Represents a blueprint.\n\n    :param name: The name of the blueprint.\n    :param import_name: The package name.\n    \"\"\"\n\n    def __init__(self, name):\n        self.name = name\n"
    }

    fn guard() -> &'static str {
        "        if \".\" in name:\n            raise ValueError(\"names may not contain dots\")\n"
    }

    // what this catches (#317, measured 3× on pallets__flask-4045): the write that parses and
    // does nothing. The model derives the correct guard and anchors it on a docstring line, so
    // the code lands INSIDE the class docstring. `ast.parse` is happy, the test suite runs, the
    // guard is prose. The last measured run applied cleanly with all 51 PASS_TO_PASS green and
    // both FAIL_TO_PASS still failing — indistinguishable from the model simply being wrong.
    //
    // Pins the LIVE stance: the edit LANDS (a citizen writes code as text whenever she means to
    // — docstring examples, fixtures, quoted snippets — and that is first-class), and she is
    // TOLD it will not execute. Warning, never a block.
    #[test]
    fn code_written_into_a_docstring_lands_but_says_it_will_not_execute() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();

        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt {
                    line: 5,
                    content: guard().to_string(),
                },
                None,
            )
            .expect("a live citizen's edit is never refused for landing in a literal");
        assert!(r.success, "the live path writes it: {:?}", r.error);
        let ctx = r
            .applied_context
            .expect("an edit into a literal must carry a receipt");
        assert!(
            ctx.contains("INSIDE A STRING LITERAL") && ctx.contains("never executes"),
            "she must be told the text is inert, in those words, got:\n{ctx}"
        );
        // It must AFFIRM the legitimate case first — writing about code is first-class, and a
        // notice that reads as a scolding for something she may have meant is a PX defect.
        assert!(
            ctx.contains("nothing to fix"),
            "a citizen who MEANT to write a docstring example must be told she is already \
             correct, got:\n{ctx}"
        );
        // And it must teach the way out with real verbs, not "widen the range".
        for must in ["code/read", "code/edit", "method body"] {
            assert!(
                ctx.contains(must),
                "the notice must walk her through it — missing `{must}`, got:\n{ctx}"
            );
        }
        // And it really is on disk — this is a warning, not a silent refusal.
        assert!(fs::read_to_string(dir.path().join("src/bp.py"))
            .unwrap()
            .contains("raise ValueError"));
    }

    // what this catches: the same edit on a SCORED run, where the deliverable IS an executing
    // patch and she cannot recover from a file she believes she fixed. Refusal is what gives the
    // act→observe circuit something to retry. Pins that the file is left UNTOUCHED (a refusal
    // that half-wrote would be worse than the warning) and that the message AIMS her at the body
    // instead of only diagnosing — the lesson from the sympy-21379 refusal that sent her chasing
    // a line range she wasn't using.
    #[test]
    fn a_scored_run_refuses_the_same_edit_and_leaves_the_file_intact() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();
        let engine = engine.with_write_policy(WritePolicy::RefuseInert);

        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt {
                    line: 5,
                    content: guard().to_string(),
                },
                None,
            )
            .expect("a refusal is a result, not an Err");
        assert!(!r.success, "a scored run must not accept an inert patch");
        let err = r.error.expect("a refusal must say why");
        assert!(
            err.contains("EDIT REFUSED") && err.contains("UNCHANGED"),
            "the refusal must state that nothing was written, got:\n{err}"
        );
        // Diagnosing is not enough — it has to TEACH: name the recovery verbs and the anchor
        // that works. The sympy-21379 refusal was correct and still burned 16 of her 30 acts
        // because the advice ("widen the range") was something she had to act on blind.
        for must in [
            "code/read",
            "code/edit",
            "method body",
            "confirm the behavior",
        ] {
            assert!(
                err.contains(must),
                "the refusal must walk her through it — missing `{must}`, got:\n{err}"
            );
        }
        assert_eq!(
            fs::read_to_string(dir.path().join("src/bp.py")).unwrap(),
            blueprint_module(),
            "a refused edit must leave the file byte-identical"
        );
    }

    // what this catches: the refusal staying NARROW. Hardening a scored run must not turn into
    // "you may not edit Python" — a real fix anchored in the method body is exactly what the
    // benchmark wants, and it must sail through the same strict engine untouched.
    #[test]
    fn the_scored_gate_passes_the_same_guard_when_it_lands_in_the_body() {
        let (dir, engine) = setup_engine();
        fs::write(dir.path().join("src/bp.py"), blueprint_module()).unwrap();
        let engine = engine.with_write_policy(WritePolicy::RefuseInert);

        // Line 8 is `def __init__(self, name):` — inserting after it is the real fix.
        let r = engine
            .edit(
                "src/bp.py",
                &EditMode::InsertAt {
                    line: 9,
                    content: guard().to_string(),
                },
                None,
            )
            .expect("edit");
        assert!(
            r.success,
            "the correct fix must not be collateral damage: {:?}",
            r.error
        );
        assert!(fs::read_to_string(dir.path().join("src/bp.py"))
            .unwrap()
            .contains("raise ValueError"));
    }

    // what this catches: the content-anchored blind spot. A search/replace is the SAFER
    // idiom — the caller never has to know a line number — but the search text can still
    // match somewhere they did not mean, silently. Reporting only line-addressed edits
    // would leave exactly the idiom a careful model reaches for as the one with no
    // feedback. Pins: a replacement is located in the written content, and an append
    // reports the tail (proving it joined the file rather than fusing onto the last line).
    #[test]
    fn content_anchored_and_append_edits_report_their_landing_site_too() {
        let (_dir, engine) = setup_engine();

        let replaced = engine
            .edit(
                "src/main.ts",
                &EditMode::SearchReplace {
                    search: "line 2".to_string(),
                    replace: "REPLACED".to_string(),
                    all: false,
                },
                None,
            )
            .expect("search/replace");
        let ctx = replaced
            .applied_context
            .expect("a replacement has a location even without a line number");
        assert!(
            ctx.contains(">    2 | REPLACED"),
            "the replacement must be located and marked in the written content, got:\n{ctx}"
        );

        let appended = engine
            .edit(
                "src/main.ts",
                &EditMode::Append {
                    content: "TAIL\n".to_string(),
                },
                None,
            )
            .expect("append");
        let ctx = appended
            .applied_context
            .expect("an append lands at the end");
        assert!(
            ctx.contains("TAIL"),
            "an append must show the tail it produced, got:\n{ctx}"
        );
    }


    fn setup_engine() -> (tempfile::TempDir, FileEngine) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "line 1\nline 2\nline 3\n").unwrap();

        let security = PathSecurity::new(dir.path()).unwrap();
        let engine = FileEngine::new("test-persona", security);
        (dir, engine)
    }

mod filesystem;
