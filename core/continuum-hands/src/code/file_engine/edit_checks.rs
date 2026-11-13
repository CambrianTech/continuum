//! What an edit is checked for and told about: parse, landing site, notices, repair hints.
use super::*;

/// How many lines on each side of an edit the receipt shows. Enough to see whether the
/// insert landed inside a signature, a parameter list, or a docstring — the misplacements
/// actually observed — without turning every edit receipt into a file dump that crowds a
/// small working-memory window.
// context-budget-exempt: lines of surrounding source shown around an applied edit so she can SEE what landed — a diff display shape in lines, not a size budget
pub(super) const APPLIED_CONTEXT_RADIUS: u32 = 6;

/// A file's own language checker, if this file type has a cheap one. `None` means "no
/// opinion" — most extensions, and that is correct: silence must be the default so the
/// warning carries weight when it appears.
///
/// Deliberately NOT a build. `py_compile` parses one file in milliseconds and answers the
/// only question asked here — does this still parse. Running the project's real test suite
/// is the persona's own move via `code/shell`, not something a file write does behind her.
/// This path's in-process validator, if we have a parser for its language.
///
/// Was `syntax_checker_for`, which returned an argv for `python3 -m py_compile`. Core is
/// entirely Rust — see `code::syntax` for why the interpreter had to go. `None` is still
/// the honest default for a language we cannot parse.
pub(super) fn validator_for(
    path: &std::path::Path,
) -> Option<&'static dyn crate::code::syntax::SyntaxValidator> {
    crate::code::syntax::validator_for(path)
}

/// Does the file still parse after this edit? `Some(error)` only when a checker exists,
/// ran, and REJECTED the file. A missing interpreter, an unknown extension, or a spawn
/// failure all yield `None`: a tool that cannot verify must not render a verdict.
///
/// This exists because no lexical proxy works. Measured on the M5, 2026-08-04 — three
/// SWE-bench runs, every failure the same shape: a guard clause placed inside an open
/// `def __init__(`. The closing paren is still present, so delimiters stay balanced while
/// the file is unparseable (a bracket-balance check was built, falsified by its own test,
/// and thrown away). "Valid text at this location" and "valid program" are different
/// predicates. A real `SyntaxError: line 21` is unambiguous where six numbered lines of
/// plausible-looking Python are not.
///
/// NOTE the narrowed scope since the refusal gate landed. An edit that would break a file
/// that parsed is now rejected before it is written, so reaching this function means the file
/// was ALREADY broken and this edit did not fix it. There is no longer a "your edit caused
/// this" case to report here — the gate reports that, at the moment it refuses, and the file
/// never enters the broken state. The only remaining way in is damage from a whole-file
/// `code/write` (deliberately ungated) or a change made outside the engine.
pub(super) fn syntax_error_after_edit(abs_path: &std::path::Path) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let content = fs::read_to_string(abs_path).ok()?;
    let fault = validator.parse_check(&content).err()?;
    Some(format!(
        "STILL BROKEN — this file does not parse, and the damage was already here before \
         this edit (an edit that would BREAK a parsing file is refused outright, so it \
         cannot be from this one):\n{fault}\n\
         Fixing only the error above will leave the earlier damage in place. `code/undo` \
         walks changes back one at a time.\n"
    ))
}

/// Would `content` have parsed, at this path's language? Writes it to a sibling temp file so
/// the check never disturbs what is on disk. `false` whenever we cannot tell — the caller
/// uses this only to decide WHICH true statement to make, and "we could not verify" must
/// never masquerade as "it was already broken".
pub(super) fn parses_clean(abs_path: &std::path::Path, content: &str) -> bool {
    probe_parse(abs_path, content)
        .map(|o| o.is_none())
        .unwrap_or(false)
}

/// A `search_replace` whose text isn't in the file — reported as a LEAD, not a dead end.
///
/// "Search text not found: 'if len(univariate) == 1:'" is a true statement that helps with
/// nothing. The tool is holding the file; it can say WHY the match missed. And this error is
/// on the path the edit-refusal deliberately steers her onto ("anchor on the text itself with
/// a search_replace edit") — a redirect that lands on a dead end is worse than no redirect.
///
/// The dominant cause by far is invisible: leading whitespace. A model reproducing a line from
/// a numbered read routinely drops or normalises the indent, and the diff is unseeable in a
/// quoted string. So when the trimmed forms match, SAY that, and give the exact line with its
/// indent spelled out.
pub(super) fn search_miss_report(content: &str, search: &str) -> String {
    let head: String = if search.len() > 60 {
        format!("{}…", &search[..60])
    } else {
        search.to_string()
    };

    // Single-line searches are the common case and the one we can localise precisely.
    let needle = search.trim();
    if !needle.is_empty() && !needle.contains('\n') {
        for (i, line) in content.lines().enumerate() {
            if line.trim() == needle {
                let n = i + 1;
                let indent = line.len() - line.trim_start().len();
                return format!(
                    "SEARCH TEXT NOT FOUND — but line {n} matches once whitespace is ignored, so \
                     this is an INDENT mismatch, not a missing line.\n\
                     the file has : {line:?}\n\
                     you searched : {search:?}\n\
                     Line {n} starts with {indent} space(s). Copy it exactly — including leading \
                     whitespace — or use a line_range edit on line {n}, which does not depend on \
                     reproducing the indent."
                );
            }
        }
        // No whitespace-equal line: offer the nearest neighbour so she can see how far off she
        // is, rather than guessing whether the file changed under her.
        if let Some((n, line)) = nearest_line(content, needle) {
            return format!(
                "SEARCH TEXT NOT FOUND. The closest line is {n}:\n\
                 the file has : {line:?}\n\
                 you searched : {search:?}\n\
                 If that is the line you meant, copy it verbatim from a fresh `code/read` — the \
                 text you searched for does not appear anywhere in the file, so it is either \
                 mis-remembered or from a different file."
            );
        }
    }

    format!(
        "SEARCH TEXT NOT FOUND: '{head}'. Nothing in this file resembles it. Re-read the region \
         with `code/read` and copy the anchor verbatim, or address the lines by number with a \
         line_range edit."
    )
}

/// Indentation-tolerant `search_replace` fallback. `Ok(Some(new_content))` when the search
/// block matches exactly one contiguous run of file lines with per-line leading whitespace
/// ignored — the replacement is applied there, re-based onto the file's real indentation
/// (the replacement block's own common leading prefix is treated as its baseline, so its
/// RELATIVE indentation survives the shift). `Ok(None)` when nothing matches (caller falls
/// through to the miss report). `Err` when the normalized match is ambiguous: applying a
/// whitespace-blind edit to one of several candidate sites would be a silent guess.
pub(super) fn indent_tolerant_replace(
    content: &str,
    search: &str,
    replace: &str,
) -> Result<Option<String>, FileEngineError> {
    let search_trimmed: Vec<&str> = search.lines().map(str::trim).collect();
    // A whitespace-only search has no content identity to match on.
    if search_trimmed.is_empty() || search_trimmed.iter().all(|l| l.is_empty()) {
        return Ok(None);
    }
    let file_lines: Vec<&str> = content.lines().collect();
    let k = search_trimmed.len();
    if file_lines.len() < k {
        return Ok(None);
    }

    let mut matches: Vec<usize> = Vec::new();
    for start in 0..=(file_lines.len() - k) {
        if (0..k).all(|j| file_lines[start + j].trim() == search_trimmed[j]) {
            matches.push(start);
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => {
            let start = matches[0];
            let file_indent: String = file_lines[start]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();

            // The replacement block's own baseline: the longest whitespace prefix shared
            // by every non-empty line. Stripping it before prepending the file's indent
            // preserves the block's relative structure.
            let replace_lines: Vec<&str> = replace.lines().collect();
            let baseline: String = replace_lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| {
                    l.chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>()
                })
                .min_by_key(String::len)
                .unwrap_or_default();
            // A baseline only counts if every non-empty line actually starts with it
            // (mixed tab/space prefixes can produce a "shortest prefix" nothing shares).
            // An EMPTY baseline is fine: stripping it is a no-op and each line's own
            // indent IS its relative indent.
            let has_common = replace_lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .all(|l| l.starts_with(baseline.as_str()));

            let mut result = String::new();
            for line in &file_lines[..start] {
                result.push_str(line);
                result.push('\n');
            }
            for line in &replace_lines {
                if line.trim().is_empty() {
                    // Blank replacement lines stay blank — trailing indent is noise.
                } else if has_common {
                    result.push_str(&file_indent);
                    result.push_str(line.strip_prefix(baseline.as_str()).unwrap_or(line));
                } else {
                    // No shared prefix — flatten to the file indent. Cruder (relative
                    // structure is lost) but never ambiguous about WHERE.
                    result.push_str(&file_indent);
                    result.push_str(line.trim_start());
                }
                result.push('\n');
            }
            for line in &file_lines[start + k..] {
                result.push_str(line);
                result.push('\n');
            }
            if !content.ends_with('\n') && result.ends_with('\n') {
                result.pop();
            }
            Ok(Some(result))
        }
        n => {
            let sites: Vec<String> = matches
                .iter()
                .take(5)
                .map(|s| (s + 1).to_string())
                .collect();
            Err(FileEngineError::EditFailed(format!(
                "SEARCH TEXT NOT FOUND verbatim, and it matches {n} locations once leading \
                 whitespace is ignored (starting at lines {}). Applying to one of them would \
                 be a guess. Copy the exact text of the site you mean — including its \
                 indentation — from a fresh `code/read`, or address it by number with a \
                 line_range edit.",
                sites.join(", ")
            )))
        }
    }
}

/// The line most similar to `needle`, by shared-word overlap. Deliberately crude: its only job
/// is to point at something she can LOOK at, and a wrong-but-close pointer still orients her
/// better than "not found". `None` when nothing shares meaningful content.
pub(super) fn nearest_line(content: &str, needle: &str) -> Option<(usize, String)> {
    let want: Vec<&str> = needle.split_whitespace().collect();
    if want.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize, String)> = None;
    for (i, line) in content.lines().enumerate() {
        let hits = want.iter().filter(|w| line.contains(**w)).count();
        if hits == 0 {
            continue;
        }
        if best.as_ref().map(|(h, _, _)| hits > *h).unwrap_or(true) {
            best = Some((hits, i + 1, line.to_string()));
        }
    }
    // Demand at least half the tokens so an accidental `self`/`)` overlap is not "closest".
    best.filter(|(hits, _, _)| *hits * 2 >= want.len())
        .map(|(_, n, line)| (n, line))
}

/// How far past a mis-bounded `end_line` to look for the line that closes the construct.
///
/// Sized for real code, not for search: a triple-quoted docstring, an argument list, a nested
/// literal — the shapes that actually swallow an edit — close within a couple of dozen lines.
/// A wider window would mostly buy the ability to "repair" a range so wrong that widening it
/// silently eats unrelated code, which is worse than saying nothing.
pub(super) const REPAIR_SEARCH_LINES: u32 = 40;

/// Given a refused edit, find the concrete change that would have worked — and say it.
///
/// Glass-boxed on sympy-22005: she asked for `line_range` 240..247 when the block ends at 249,
/// because 248-249 are the tail of a triple-quoted string. The old refusal told her to "widen
/// the range to cover the whole construct", which is exactly the judgement she had already got
/// wrong once. The parser that proved the edit destructive can also prove which end line is
/// right, so it does the counting and hands her the number.
///
/// This is not a heuristic steering her cognition — nothing is guessed. Each candidate is
/// APPLIED and PARSED; the first that parses is reported as fact, and if none do she gets the
/// general guidance instead of a confident wrong answer.
pub(super) fn repair_hint(abs_path: &std::path::Path, old_content: &str, edit_mode: &EditMode) -> String {
    pub(super) const GENERAL: &str = "The most common cause is a line range that ends INSIDE a construct \
         it does not close — a triple-quoted string, an open paren, a bracket. Widen the range \
         to cover the whole construct, or anchor on the text itself with a search_replace edit \
         instead of counting lines. Re-read the region first: `code/read` numbers lines by \
         absolute position, so the numbers it shows are the numbers this tool expects.";

    let EditMode::LineRange {
        start_line,
        end_line,
        new_content,
    } = edit_mode
    else {
        return GENERAL.to_string();
    };

    let total = old_content.lines().count() as u32;
    for candidate in (end_line + 1)..=(end_line + REPAIR_SEARCH_LINES).min(total) {
        let widened = EditMode::LineRange {
            start_line: *start_line,
            end_line: candidate,
            new_content: new_content.clone(),
        };
        let Ok(text) = apply_edit(old_content, &widened) else {
            continue;
        };
        if parses_clean(abs_path, &text) {
            let extra = candidate - end_line;
            return format!(
                "THE FIX: use end_line={candidate} instead of {end_line}. Your range stopped \
                 {extra} line(s) short — lines {}..{candidate} are the tail of a construct your \
                 replacement re-opens and closes itself, so the original's tail survived after \
                 it. The SAME new_content with end_line={candidate} parses clean; re-issue the \
                 edit with that one number changed.",
                end_line + 1
            );
        }
    }
    GENERAL.to_string()
}

/// The syntax error `content` WOULD produce at this path's language, formatted for a persona.
/// `None` when it parses, or when this language has no checker — a refusal still stands on the
/// `parses_clean` pair, so an unavailable detail costs an explanation, never a wrong verdict.
pub(super) fn syntax_error_detail(abs_path: &std::path::Path, content: &str) -> Option<String> {
    let detail = probe_parse(abs_path, content).ok()??;
    let head = detail.lines().take(6).collect::<Vec<_>>().join("\n");
    // The checker names the temp probe it actually read. Handing a persona a path that does
    // not exist sends her to look for a file she cannot find — say the real one.
    let real = abs_path.file_name()?.to_str()?;
    let probe = format!(
        "{}.continuum-parse-probe.{}",
        abs_path.file_stem()?.to_str()?,
        abs_path.extension()?.to_str()?
    );
    Some(head.replace(&probe, real))
}

/// Run the language's syntax checker over `content` WITHOUT touching what is on disk.
///
/// `Ok(None)` parsed clean, `Ok(Some(stderr))` did not, `Err(())` we could not tell (no
/// extension, no checker for this language, probe unwritable). The three-way return is the
/// point: callers must never be able to read "could not verify" as "already broken".
pub(super) fn probe_parse(abs_path: &std::path::Path, content: &str) -> Result<Option<String>, ()> {
    let validator = validator_for(abs_path).ok_or(())?;
    match validator.parse_check(content) {
        Ok(()) => Ok(None),
        Err(fault) => Ok(Some(fault.to_string())),
    }
}

/// Names the edit CALLS that exist nowhere in the module — a guaranteed `NameError`.
///
/// `parses_clean` is a syntax gate, and a missing import is perfectly valid syntax. So the
/// most destructive edit a model makes routinely sails straight through it.
///
/// Glass-boxed on sympy-21379 (v7 — her first edit to real library source all session). The
/// entire library change was two lines:
///     @cacheit
///     def _subs(self, old, new, **hints):
///   +     # Clear cache to avoid issues with assumptions
///   +     clear_cache()
/// `clear_cache` is imported nowhere in `sympy/core/basic.py` — line 957 is its ONLY
/// occurrence in the file. Every substitution in sympy now raises NameError. Graded result:
/// passToPass 26/40, fourteen previously-passing tests destroyed, and the edit reported
/// `success: true` with no hint of what it had done.
///
/// DELIBERATELY NARROW, because a false refusal here is worse than none
/// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]). Python scoping is genuinely hard
/// — globals, builtins, star-imports, comprehensions, class bodies, late binding. So this
/// flags a name only when EVERY one of these holds:
///   • it is CALLED in the new content,
///   • it is bound nowhere in the new module (no import, def, class, assignment, param,
///     comprehension target, except-as, with-as, global/nonlocal declaration),
///   • it is not a builtin,
///   • and it appears NOWHERE in the OLD file — so the edit INTRODUCED it.
/// That last clause is what makes it safe: a name the file never mentioned, now called and
/// unbound, is a missing import essentially every time. Anything the file already knew about
/// is left alone, star-imports included.
///
/// Returns the offending names, or `None` when the analysis cannot run at all (no
/// validator for this file's language) — an inconclusive probe must never read as a verdict.
pub(super) fn introduced_undefined_calls(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<Vec<String>> {
    let validator = validator_for(abs_path)?;
    let unbound = validator.unbound_calls(new_content)?;
    // The safety clause: only names the edit INTRODUCED. Anything the old file already
    // mentioned (star-imported, conditionally defined, whatever) is none of our business.
    let introduced: Vec<String> = unbound
        .into_iter()
        .filter(|n| !old_content.contains(n.as_str()))
        .collect();
    (!introduced.is_empty()).then_some(introduced)
}

/// Functions whose docstring the edit DISPLACED — code inserted between `def` and the
/// docstring, so the string is now a bare expression statement and no longer a docstring.
///
/// Same 2-line sympy-21379 edit that produced the missing import also did this:
///     def _subs(self, old, new, **hints):
///   +     clear_cache()
///         """Substitutes an expression old -> new. ..."""
/// `ast.get_docstring(_subs)` returns None afterwards. `help()` loses it, and in a project
/// like sympy — where doctests ARE part of the suite — the doctests in that string stop being
/// collected at all. Valid syntax, so the parse gate cannot see it.
///
/// A WARNING, not a refusal, and the distinction is deliberate: a missing import is a
/// guaranteed `NameError` and earns a refusal; a displaced docstring is real degradation but
/// the code still runs, and she may have meant it. Severity should match force — the gate that
/// cries wolf is the gate that gets ignored.
///
/// Reports only functions whose docstring the edit ACTUALLY MOVED: present before, absent
/// after. A function that never had one is untouched.
pub(super) fn displaced_docstrings(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let lost = validator.displaced_docstrings(old_content, new_content)?;
    if lost.is_empty() {
        return None;
    }
    Some(format!(
        "WARNING: {} no longer has a docstring — your insert went BETWEEN the `def` line and \
         the docstring, so that string is now an ordinary expression statement. `help()` loses \
         it, and any doctests inside it stop being collected (in this repo those may be part of \
         the test suite). Move your code to AFTER the closing quotes.\n",
        lost.join(", ")
    ))
}

/// Code the edit put INSIDE a string literal — the write that parses and does nothing (#317).
///
/// The loudest possible wording, because this failure is the quietest one there is. A file that
/// will not parse screams: `syntax_error_after_edit` names the line, the tests fail immediately,
/// and the cause is obvious. This one leaves a green parse, an unchanged docstring, and a test
/// suite that fails for reasons that look like the model was simply wrong.
///
/// Measured on `pallets__flask-4045` (2026-08-06): she derived the correct guard and wrote it
/// into the middle of the class docstring, deleting ~17 lines of API docs. Every gate we had was
/// silent and the zero was charged to her intelligence.
pub(super) fn inert_insertion_sites(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let validator = validator_for(abs_path)?;
    let inert = validator.inert_insertions(old_content, new_content)?;
    if inert.is_empty() {
        return None;
    }
    Some(
        inert
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// The recovery path, shared by the warning and the refusal so the advice cannot drift between
/// them. Numbered and concrete: naming the verbs beats "widen the range", which is advice she
/// has to act on blind (the sympy-21379 lesson — a correct refusal plus a vague hint burned 16
/// of her 30 acts).
pub(super) fn inert_edit_recovery() -> &'static str {
    "\x20 1. `code/read` that region and find where the docstring's closing `\"\"\"` is.\n\
     \x20 2. Choose an anchor INSIDE the function or method body — a real statement you can \
     see, not a line of prose.\n\
     \x20 3. Re-apply with `code/edit`, matching on that statement's TEXT rather than a line \
     number (line numbers shift as you edit; text does not).\n\
     \x20 4. Run it (`code/shell`) and confirm the behavior actually changed — a passing parse \
     is not proof that anything executes.\n"
}

/// The LIVE stance: the edit landed, and she is told what it will and will not do. Writing code
/// as text is first-class (a docstring example, a fixture, a snippet she is quoting), so this
/// opens by saying that outright before offering the way out — it must never read as a scolding
/// for something she may have meant.
pub(super) fn inert_insertions(
    abs_path: &std::path::Path,
    old_content: &str,
    new_content: &str,
) -> Option<String> {
    let where_ = inert_insertion_sites(abs_path, old_content, new_content)?;
    Some(format!(
        "HEADS UP: your edit landed INSIDE A STRING LITERAL ({where_}) — a docstring or quoted \
         block. The file parses and the tests will run, but that code is TEXT: it never executes, \
         so nothing you intended actually changed. This is exactly how a run looks like a wrong \
         answer when the reasoning was right.\n\
         \n\
         If you MEANT to write about code — a docstring example, a snippet you are quoting, a \
         fixture — this is already correct and there is nothing to fix.\n\
         \n\
         If you meant it to RUN:\n{}",
        inert_edit_recovery()
    ))
}

/// What a whole-file write REPLACED, when it replaced anything. `None` for a genuinely
/// new file — creating one destroys nothing and needs no warning.
///
/// Glass-boxed on the M5, 2026-08-04. A persona had just produced a correct 2-line fix to
/// `src/flask/blueprints.py` (128 lines). Two acts later she called `code/write` with a
/// 5-line stub reconstructed from memory — no imports, no class hierarchy, the real module
/// gone — and the receipt told her `success: true, bytes_written: 214`. She read the file
/// afterward and still did not repair it. The task ended with the file destroyed and the
/// correct fix erased.
///
/// The `applied_context` doc says a whole-file write has no landing site to report, and
/// that is true. It does not follow that it has nothing to report. Replacing 128 lines
/// with 5 is the single most consequential thing a file tool can do, and it was the ONE
/// path reporting nothing — so the verb most able to destroy work was the verb with the
/// least feedback. The shrink ratio is the fact; what she does with it stays hers
/// ([[no-hardcoded-heuristics-to-steer-cognition]] — this refuses nothing and gates
/// nothing).
pub(super) fn overwrite_magnitude(old_content: &str, new_content: &str) -> Option<String> {
    if old_content.is_empty() {
        return None; // a new file: nothing was there to lose
    }
    let before = old_content.lines().count();
    let after = new_content.lines().count();
    let mut out = format!(
        "OVERWROTE an existing file: {before} line(s) replaced by {after}. The previous \
         content is gone from disk (recoverable via code/undo).\n"
    );
    // A large shrink is the destructive-clobber shape specifically: a file reconstructed
    // from memory instead of edited in place. Name it, and show what survives, so the next
    // turn perceives the loss instead of reading "success".
    if after * 2 < before {
        out.push_str(
            "This REMOVED most of the file. If you meant to change part of it, code/undo \
             restores it and code/edit changes a region without rewriting the whole file.\n",
        );
    }
    out.push_str("The file now reads:\n");
    out.push_str(&numbered_neighborhood(
        new_content,
        1,
        APPLIED_CONTEXT_RADIUS * 2,
    ));
    Some(out)
}

/// One line of a read as `code/read` shows it: `{n:>6} | {line}`. The gutter is the
/// contract between the read and `code/edit`'s line numbers, so its format and its parse
/// ([`gutter_line_number`]) live together.
pub(super) fn numbered_line(number: u32, line: &str) -> String {
    format!("{number:>6} | {line}")
}

/// The line number a [`numbered_line`] carries, or `None` for a line with no gutter.
/// Working memory reads which lines of a file a read actually showed from these.
pub fn gutter_line_number(rendered: &str) -> Option<u32> {
    let t = rendered.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    // An empty source line renders as `     5 | ` and may arrive with its trailing space trimmed.
    let rest = &t[digits..];
    (rest.starts_with(" | ") || rest == " |").then(|| t[..digits].parse().ok()).flatten()
}

/// A [`numbered_line`] split into its line number and its code: the parser the context
/// crate's code level (`continuum_context::levels::Shape::Code`) takes, so the gutter format
/// stays owned here.
pub fn gutter_split(rendered: &str) -> Option<(u32, &str)> {
    let number = gutter_line_number(rendered)?;
    let code = rendered.split_once(" | ").map_or("", |(_, code)| code); // map_or: an empty source line renders with no code
    Some((number, code))
}

/// Do these bytes look like `code/read` OUTPUT rather than file CONTENT?
///
/// The hazard that arrives with numbered reads: read a file, hand the numbered text
/// straight back to `code/write`, and every line is now prefixed with a gutter that was
/// never in the source. The write succeeds, the file is corrupt, and the receipt would
/// otherwise say `success: true`. This is the read→write round trip, and it is a
/// predictable consequence of the numbering — so it ships WITH the numbering, not after
/// the first time it destroys a file.
///
/// Reports; never refuses. A file that genuinely contains a numbered gutter (a captured
/// listing, a diff fixture) is legitimate content, and the caller is the one who knows
/// which this is. Requires a strong majority so ordinary code with a few `123 | x` lines
/// stays quiet, and a floor of 3 lines so a two-line snippet can't trip it.
pub(super) fn looks_like_numbered_read(content: &str) -> bool {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 3 {
        return false;
    }
    let gutters = lines
        .iter()
        .filter(|l| gutter_line_number(l).is_some())
        .count();
    gutters * 10 >= lines.len() * 9
}

/// The REFUSAL-path sibling of [`numbered_paste_notice`]. Same detection, opposite
/// consequence: here the gate caught it BEFORE writing, so the file is intact and there is
/// nothing to undo. Saying "this file is now corrupt" on this path would be a plain lie, and
/// a diagnostic that lies about the state of the world is worse than none
/// ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
pub(super) fn numbered_paste_refusal(content: &str) -> String {
    if !looks_like_numbered_read(content) {
        return String::new();
    }
    "THE FIX: nearly every line you sent begins with a `NNN | ` line-number gutter. That      gutter is how `code/read` DISPLAYS a file — it is NOT part of the file, and prefixing      every line with it is what made this unparseable (each line ends up indented). Nothing      was written, so the file is fine. Re-send the SAME content with the `NNN | ` prefix      stripped from every line."
        .to_string()
}

/// The notice `looks_like_numbered_read` earns, or empty when the content is clean.
pub(super) fn numbered_paste_notice(content: &str) -> String {
    if !looks_like_numbered_read(content) {
        return String::new();
    }
    "WARNING: nearly every line of what you wrote begins with a `NNN | ` line-number \
     gutter. That gutter is how code/read DISPLAYS a file — it is not part of the file. \
     If you pasted read output back, this file is now corrupt; code/undo restores it, and \
     writing the lines WITHOUT the `NNN | ` prefixes fixes it.\n"
        .to_string()
}

/// What this edit did to every line number BELOW it — the fact that makes a second
/// line-addressed edit safe.
///
/// A line-addressed edit that changes the line COUNT silently invalidates every line
/// number the caller is holding from an earlier read. Glass-boxed on SWE-bench flask-4045
/// (run 4, 2026-08-04): read the file → `line_range 16..17` replacing 2 lines with 4 →
/// `line_range 14..23` computed against the numbers from the READ. The second edit was a
/// correct, complete replacement block aimed exactly 2 lines short, so it left the tail of
/// the old parameter list orphaned below it and broke the parse. Nothing in the receipt
/// said the map had moved.
///
/// So the receipt says it. The delta is arithmetic, not a guess, and it states a fact about
/// the file rather than an instruction about what to do next
/// ([[no-hardcoded-heuristics-to-steer-cognition]]) — a caller who re-reads and a caller who
/// adjusts by `delta` are both acting on the same true statement. Empty when the line count
/// did not change: an in-place replacement invalidates nothing, and a notice that fires on
/// every edit is a notice nobody reads.
pub(super) fn line_shift_notice(old_content: &str, new_content: &str, anchor: u32) -> String {
    let before = old_content.lines().count() as i64;
    let after = new_content.lines().count() as i64;
    let delta = after - before;
    if delta == 0 {
        return String::new();
    }
    format!(
        "LINE NUMBERS SHIFTED: this file went from {before} to {after} lines ({delta:+}). \
         Every line below {anchor} now sits {delta:+} from where it was, so any line number \
         taken from an earlier read of this file is stale. Re-read before addressing lines \
         by number again.\n"
    )
}

/// The line an edit landed on, resolved against the content that was ACTUALLY written.
///
/// Every `EditMode` puts text somewhere, so every one of them has a landing site worth
/// showing — not just the ones addressed by number. A content-anchored `SearchReplace`
/// is the safer idiom precisely because the caller does not have to know a line number,
/// but "safer" is not "always right": the search text can match in a place the caller did
/// not mean, and silently. Locating the replacement in the new content answers that.
pub(super) fn edit_anchor_line(edit_mode: &EditMode, new_content: &str) -> Option<u32> {
    match edit_mode {
        EditMode::LineRange { start_line, .. } => Some(*start_line),
        EditMode::InsertAt { line, .. } => Some(*line),
        // Report the FIRST replacement site. With `all: true` there may be several; the
        // first is the honest representative, and the caller asked for a sweep so the
        // count is not the surprise — placement is.
        EditMode::SearchReplace { replace, .. } => {
            line_of_first_occurrence(new_content, replace.lines().next().unwrap_or(replace))
        }
        // An append always lands at the end; showing the tail proves it joined the file
        // cleanly rather than fusing onto a last line that had no trailing newline.
        EditMode::Append { .. } => {
            let total = new_content.lines().count() as u32;
            (total > 0).then_some(total)
        }
    }
}

/// 1-indexed line of the first line that contains `needle`, or `None` when it does not
/// appear (a replacement that cannot be located is not one we should invent a site for).
pub(super) fn line_of_first_occurrence(content: &str, needle: &str) -> Option<u32> {
    if needle.is_empty() {
        return None;
    }
    content
        .lines()
        .position(|line| line.contains(needle))
        .map(|idx| idx as u32 + 1)
}

/// Numbered lines around `anchor` in `content` — the same shape `code/read` returns, so a
/// persona reads one surface for "what is in this file", never two. 1-indexed, clamped to
/// the file, and marked with `>` on the anchor line so the landing site is unmissable.
pub(super) fn numbered_neighborhood(content: &str, anchor: u32, radius: u32) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len() as u32;
    if total == 0 {
        return "(file is now empty)".to_string();
    }
    let anchor = anchor.clamp(1, total);
    let first = anchor.saturating_sub(radius).max(1);
    let last = (anchor + radius).min(total);
    let mut out = String::new();
    for n in first..=last {
        let marker = if n == anchor { '>' } else { ' ' };
        out.push_str(&format!("{marker}{n:>5} | {}\n", lines[(n - 1) as usize]));
    }
    out
}
