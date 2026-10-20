use super::*;

/// Every path a unified diff targets, read off its `+++ b/<path>` headers.
///
/// `/dev/null` is skipped (a deletion has no post-image path). Quoted paths — git emits
/// `+++ "b/odd name.py"` when a name needs escaping — are unquoted, because a path we fail to
/// recognise is a path we would fail to restore, and a silent miss here reintroduces exactly
/// the dirty-tree apply this exists to prevent.
pub(super) fn diff_target_paths(diff: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in diff.lines() {
        let Some(rest) = line.strip_prefix("+++ ") else {
            continue;
        };
        // Trailing tab-separated timestamp is legal in unified diffs; git omits it, others don't.
        let rest = rest.split('\t').next().unwrap_or(rest).trim();
        let rest = rest.strip_prefix('"').map_or(rest, |r| r.trim_end_matches('"'));
        if rest == "/dev/null" {
            continue;
        }
        // `b/` prefix under the default `--src-prefix`/`--dst-prefix`; `-p1` strips it.
        let path = rest.strip_prefix("b/").unwrap_or(rest);
        if !path.is_empty() && !out.iter().any(|p| p == path) {
            out.push(path.to_string());
        }
    }
    out
}

/// Restore the paths a patch targets to their committed state, discarding any working-tree
/// edits to them. See the call site in the grade path for why this is the test patch's
/// precondition — in one line: the tests are not the solver's to touch.
///
/// A path the patch CREATES does not exist at HEAD; `git checkout` fails on it and that is
/// correct and harmless, so failures are per-path and non-fatal. The error case we do care
/// about — a path that exists at HEAD and refuses to restore — surfaces at the apply that
/// follows, with the patch's own diagnostics.
pub(super) async fn restore_paths_to_base(repo_dir: &Path, patch: &str) -> Result<(), String> {
    let paths = diff_target_paths(patch);
    if paths.is_empty() {
        return Ok(());
    }
    for path in &paths {
        let _ = run("git", &["checkout", "HEAD", "--", path], Some(repo_dir)).await;
    }
    Ok(())
}

/// Apply a patch, tolerating the whitespace drift that trips a strict apply.
pub async fn apply_patch(repo_dir: &Path, text: &str, what: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let path = repo_dir.join(format!(".{what}.patch"));
    std::fs::write(&path, text).map_err(|e| format!("could not stage {what} patch: {e}"))?;
    let p = path.to_string_lossy().to_string();
    // Last arm is --3way: when the candidate patch already edited lines the test_patch's
    // context touches, exact/relaxed apply fails ("tree is not what the patch expects" —
    // flask-4992 + sphinx-10451 graded INFRA on this, Round A). 3-way merges against the
    // blob ids in the patch header instead, succeeding wherever the edits don't genuinely
    // overlap — and a genuine overlap still fails loudly, which is the honest outcome.
    for extra in [
        vec![],
        vec!["--ignore-whitespace"],
        vec!["--ignore-whitespace", "-C1"],
        vec!["--3way"],
    ] {
        let mut args = vec!["apply"];
        args.extend(extra);
        args.push(&p);
        if let Ok(out) = run("git", &args, Some(repo_dir)).await {
            if out.status.success() {
                let _ = std::fs::remove_file(&path);
                return Ok(());
            }
        }
    }
    Err(format!(
        "could not apply {what} patch — the tree is not what the patch expects"
    ))
}

/// The interpreter an instance's own code could actually have run on.
///
/// 3.9 is the last release carrying the `collections.Mapping` aliases pre-2020 code reaches
/// for; uv's own floor is 3.8, so there is no lower rung to offer. Coarse on purpose — the
/// gold gate is the arbiter, and a wrong guess fails loudly rather than producing a
/// plausible number.
///
/// The 2020..=2022 rung is 3.10, learned live (pylint-5859, 2026-08-11): Python 3.11
/// removed `inspect.formatargspec`, which era sdist BUILDS still import (wrapt 1.13.3,
/// pulled by 2022 astroid) — every 2022-era env with such a dep died at build on 3.11.
/// 3.11 released 2022-10; a March-2022 dependency graph never targeted it. The same
/// mismatch is the leading suspect for the p2p-0/N broken baselines (flask-5063,
/// pytest-5103) whose envs were built on 3.11 before this rung existed.
pub fn interpreter_for_year(year: u32) -> &'static str {
    // The rung must respect the ERA PYTEST's supported-python window, not just
    // the repo's (the harness runs on this interpreter too). Glass-boxed
    // 2026-08-27 by the first coverage map: 2020-21 classes (sphinx ×36,
    // sympy ×24, requests) all died in pytest's assertion rewriter with
    // `required field "lineno" missing` — era pytest 5.4/6.0-6.1 predates
    // py3.10's AST strictness (3.10 support landed in pytest 6.2.5, Oct 2021).
    // 2020 → 3.8 (pytest 5.4/6.0 window), 2021 → 3.9 (pytest 6.2 window).
    // Every green class of the map pins the rest of this ladder as-is.
    if year < 2020 {
        "3.9"
    } else if year == 2020 {
        "3.8"
    } else if year == 2021 {
        "3.9"
    } else if year == 2022 {
        "3.10"
    } else {
        "3.11"
    }
}

/// Build (or reuse) the per-instance environment. Per-instance rather than per-repo because
/// instances span years of a repo's history and their dependency graphs genuinely differ.
/// The repo's declared build-time dependencies, from `[build-system].requires` in its
/// `pyproject.toml`. Empty when there is no pyproject, no `[build-system]` table, or no
/// `requires` array. These must be pre-installed into the venv when building with
/// `--no-build-isolation` (see the call site) — pip won't fetch them for us in that mode.
/// The name of the repo's own TEST extra, from `[project.optional-dependencies]` — the
/// group whose contents the suite needs to so much as COLLECT.
///
/// Why this exists (measured 2026-08-17, and it is the whole of astropy's 0-of-9): the env
/// installs `-e .`, which resolves RUNTIME dependencies only. astropy's `conftest.py` opens
/// with `import hypothesis`, a TEST dependency, so pytest dies loading conftest before it
/// reaches a single test:
///
/// ```text
/// ImportError while loading conftest '…/repo/conftest.py'.
/// conftest.py:9: in <module>
///     import hypothesis
/// E   ModuleNotFoundError: No module named 'hypothesis'
/// ```
///
/// PASS_TO_PASS then reads 0/9 on the PRISTINE tree, the gold gate correctly refuses to
/// score, and EVERY astropy instance is ungradeable — a ceiling that has nothing to do with
/// the citizen's patch. The repo already declares exactly what its suite needs; we simply
/// never asked for it.
///
/// Names are not standardised, so prefer in the order the ecosystem actually uses them and
/// take the first that the repo declares. Returns `None` when there is no pyproject, no
/// optional-dependencies table, or no test-shaped group — those repos install as before.
/// The `--exclude-newer` cutoff for an instance: its own creation date, or `None` when the
/// dataset carries no date (which disables pinning AND healing — see `era_pinned_uv_install`).
/// Extracted because BOTH the fresh-build path and the cached-env heal need the same answer,
/// and two copies of "what era is this instance" is exactly how the two drift apart.
pub(super) fn era_cutoff(instance: &SweInstance) -> Option<String> {
    if instance.created_at.is_empty() {
        None
    } else {
        Some(instance.created_at.clone())
    }
}

pub(super) fn test_extra_name(repo_dir: &Path) -> Option<String> {
    const PREFERRED: [&str; 4] = ["test", "tests", "testing", "dev"];
    let declared = declared_extras(repo_dir);
    PREFERRED
        .iter()
        .find(|name| declared.iter().any(|d| d == *name))
        .map(|name| (*name).to_string())
}

/// Every extra group the repo declares, from BOTH places Python puts them.
///
/// The first version of this read `pyproject.toml` only and shipped believing it worked —
/// the live run said otherwise (2026-08-17): astropy 5.3 declares its suite deps in
/// `setup.cfg` under `[options.extras_require]`, has no `[project.optional-dependencies]`
/// at all, and the pyproject-only parser returned None for the exact repo the fix was
/// written for. Both files are ordinary in the ecosystem; reading one is reading half.
pub(super) fn declared_extras(repo_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();

    // PEP 621.
    if let Ok(text) = std::fs::read_to_string(repo_dir.join("pyproject.toml")) {
        if let Ok(parsed) = toml::from_str::<toml::Value>(&text) {
            if let Some(table) = parsed
                .get("project")
                .and_then(|p| p.get("optional-dependencies"))
                .and_then(|o| o.as_table())
            {
                found.extend(table.keys().cloned());
            }
        }
    }

    // setuptools' declarative config. Hand-scanned rather than pulled in as a dependency:
    // we need section keys, not values, and an INI parser for that is more surface than
    // the six lines below.
    if let Ok(text) = std::fs::read_to_string(repo_dir.join("setup.cfg")) {
        let mut in_extras = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_extras = trimmed == "[options.extras_require]";
                continue;
            }
            // A key is flush-left; anything indented is a continuation of the previous
            // key's requirement list (`test =\n    pytest>=7.0`), never a group name.
            if !in_extras || line.starts_with([' ', '\t']) {
                continue;
            }
            if let Some((key, _)) = trimmed.split_once('=') {
                let key = key.trim();
                if !key.is_empty() {
                    found.push(key.to_string());
                }
            }
        }
    }
    found
}

/// Install the repo's test extra into an existing env, once. Non-fatal BY DESIGN: a repo
/// whose suite already collects does not need it, and a resolve failure here must not
/// delete an env that grades fine today. The marker file is what makes it once-only, and
/// what lets envs built BEFORE this existed heal themselves on their next use instead of
/// staying silently ungradeable forever.
pub(super) async fn ensure_test_extra(
    uv: &str,
    py_s: &str,
    env_dir: &Path,
    repo_dir: &Path,
    as_of: Option<&str>,
) {
    let marker = env_dir.join(".test-extra");
    // A marker is authoritative only when it NAMES the extra it installed. Markers written
    // by the first version recorded "none-declared" from a half-blind parser; treating
    // those as settled would make this fix unreachable on exactly the envs that need it,
    // and would need a human to know which files to delete. They self-heal instead.
    if std::fs::read_to_string(&marker)
        .map(|m| !m.trim().is_empty() && m.trim() != "none-declared")
        .unwrap_or(false)
    {
        return;
    }
    let Some(extra) = test_extra_name(repo_dir) else {
        // NO NEGATIVE MARKER. The first version wrote "none-declared" here to avoid
        // re-parsing, and then a parser that read only pyproject.toml recorded that answer
        // PERMANENTLY for astropy — whose extras live in setup.cfg — so the heal could
        // never run again even after the parser was fixed. Caching a negative is caching
        // the limits of today's code. Re-reading two small files per grade costs nothing
        // measurable next to cloning a repo and running its suite.
        return;
    };
    let spec = format!(".[{extra}]");
    let outcome = era_pinned_uv_install(
        uv,
        py_s,
        as_of,
        &["--no-build-isolation", "-e", &spec],
        Some(repo_dir),
        ERA_BUILD_ENV,
    )
    .await;
    let ok = matches!(&outcome, Ok(o) if o.status.success());
    crate::probe!(
        class = "swe.env.test_extra",
        extra = %extra,
        installed = ok,
        env = %env_dir.display(),
        "the repo's own test extra — without it a conftest that imports a test-only \
         dependency makes every instance in the repo ungradeable"
    );
    if ok {
        let _ = std::fs::write(&marker, format!("{extra}\n"));
    }
}

pub(super) fn build_requires(repo_dir: &Path) -> Vec<String> {
    let text = match std::fs::read_to_string(repo_dir.join("pyproject.toml")) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get("build-system")
        .and_then(|bs| bs.get("requires"))
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Era C code vs MODERN clang (#383 cause 2's second head, measured live 2026-08-16 on
/// astropy__astropy-12907 right after the jinja2 fix moved the failure downstream):
/// Xcode 15+ clang promotes `-Wincompatible-function-pointer-types`,
/// `-Wimplicit-function-declaration`, and `-Wint-conversion` to hard ERRORS. astropy 2022's
/// wcslib wrappers (`wcslib_wtbarr_wrap.c`: `tp_traverse` assigned an `int (PyWtbarr *, …)`
/// where `traverseproc` wants `int (PyObject *, …)`) compiled as WARNINGS on the era's own
/// compilers — the code under test genuinely ran this way. The compiler is HARNESS (it runs
/// on this machine), the C is SUBJECT: demote exactly those three diagnostics back to the
/// warnings they were. distutils APPENDS `CFLAGS` to its sysconfig baseline, so nothing else
/// about the build changes, and modern code that doesn't trip them is untouched.
///
/// The FOURTH head (#383, measured live 2026-08-17 on astropy__astropy-14182, and the
/// reason two dispatched rounds died at env-build after the jinja2 + build-requires fixes
/// both landed): astropy vendors cfitsio, which vendors a 1990s zlib, whose
/// `cextern/cfitsio/zlib/zutil.h:140` reads
///
/// ```c
/// #if defined(MACOS) || defined(TARGET_OS_MAC)
/// #  define OS_CODE  7
/// #    ifndef fdopen
/// #      define fdopen(fd,mode) NULL /* No fdopen() */
/// ```
///
/// `TARGET_OS_MAC` is 1 on EVERY modern Apple SDK — it means "some Apple platform", not
/// "classic Mac OS" as it did when this zlib was written. So the branch fires, `fdopen` is
/// macro-replaced by `NULL`, and the system header's own declaration
/// `FILE *fdopen(int, const char *)` becomes `FILE *NULL(int, const char *)` →
/// `error: expected identifier or '('` in `<stdio.h>`, thousands of lines from anything
/// astropy wrote. (The adjacent `'OS_CODE' macro redefined` warning is the same branch.)
///
/// The guard is `#ifndef fdopen`, so pre-defining it is the whole fix: `-Dfdopen=fdopen`
/// makes the guard FALSE — the NULL stub is never emitted — and the macro itself is the
/// identity, so every real `fdopen` call compiles to `fdopen`. Nothing is stubbed, nothing
/// is renamed, no source is patched, and a repo that does not vendor this zlib never
/// notices the flag.
///
/// It lives here rather than in a per-repo table because it is not an astropy fact — it is
/// an ERA fact (old vendored zlib vs a modern Apple SDK), identical in shape to the three
/// above: the compiler is HARNESS, the C is SUBJECT, and the subject built fine on the
/// compilers of its own day.
/// The env-var set EVERY era build runs under. `SETUPTOOLS_USE_DISTUTILS=stdlib`
/// is load-bearing for every numpy.distutils-era repo (scikit-learn 2019,
/// glass-boxed 2026-08-26): modern setuptools' vendored distutils shim DELETED
/// `distutils.msvccompiler`, which numpy.distutils imports unconditionally — so
/// the build backend dies at metadata. Era interpreters are 3.6–3.9 and all
/// carry stdlib distutils; pointing the shim back at stdlib is exactly what
/// upstream recommends for legacy numpy.distutils builds.
pub(super) const ERA_BUILD_ENV: &[(&str, &str)] = &[
    ("CFLAGS", ERA_CFLAGS),
    ("SETUPTOOLS_USE_DISTUTILS", "stdlib"),
];

// -Wno-X (not -Wno-error=X): on modern clang these are DEFAULT ERRORS, and
// -Wno-error= cannot downgrade a default error — only disabling the diagnostic
// can (coverage map 2026-08-27: pandas 0.25's C died on
// incompatible-function-pointer-types with the old form live).
// NUMPY_IMPORT_ARRAY_RETVAL: numpy 1.20 deleted the macro (it was NULL on
// py3); pandas<1.0's ujson still uses it — shimmed exactly like fdopen.
// PANDAS BUILDS UNDER ITS OWN -Werror (pandas 1.x setup.py appends `-Werror` on
// macOS AFTER these flags), so every diagnostic clang adds in a later release
// becomes a build death for a 2020 sdist. Reproduced 2026-09-16 (xarray-4356,
// gold gate: "could not install the repo into a venv"): py3.8 on arm64 has no
// pandas 1.1.0 wheel, the sdist's Cython C died on clang 21 —
// `CYTHON_FALLTHROUGH` in unreachable code (-Wunreachable-code-fallthrough,
// algos.c ×2) and then `numdigits` set but not used
// (-Wunused-but-set-variable, np_datetime_strings.c). -Wno-X disables the
// diagnostic outright, which a later -Werror cannot re-arm; with both rows
// pandas 1.1.0 built and imported in 43 s. Two rows, two measured deaths.
pub(super) const ERA_CFLAGS: &str = "-Wno-incompatible-function-pointer-types \
     -Wno-implicit-function-declaration -Wno-int-conversion \
     -Wno-unreachable-code-fallthrough -Wno-unused-but-set-variable \
     -Dfdopen=fdopen -DNUMPY_IMPORT_ARRAY_RETVAL=NULL";

/// Build deps that a repo's DEPENDENCY sdists import at build time but that nothing installs
/// under `--no-build-isolation` (we honor the top repo's `[build-system].requires`; a
/// dependency sdist's declaration is honored by nobody). Keyed by repo because which sdists
/// get built is a property of the repo's dependency graph on this platform, and by instance
/// year because rows differ by era (the 2026-08-27 coverage map caught the sklearn platform
/// pins — right for 2019's no-arm64-wheel era, poison for a 2023 py3.11 env). DATA, not
/// logic — grow it one measured failure at a time, never speculatively.
pub(super) fn era_sdist_build_deps_for(repo: &str, year: u32) -> &'static [&'static str] {
    if repo == "pydata/xarray" && year <= 2021 {
        // Era xarray drags era pandas as an sdist; its build needs cython and a
        // numpy that both carries the old C macros AND has an arm64 wheel —
        // 1.21.6 is the proven middle (xarray 0.14 + pandas 0.25.3 import
        // clean, 2026-08-27).
        return &["numpy==1.21.6", "cython<3"];
    }
    if repo == "scikit-learn/scikit-learn" && year >= 2022 {
        // scikit-learn 1.x: SWE-bench's own spec installs pandas and matplotlib beside
        // numpy/scipy/cython — the set_output tests and many others `importorskip("pandas")`.
        // 2026-09-12: 26323's only FAIL_TO_PASS test skipped on a pandas-less env and the
        // verdict voided a correct fix. Pins have py3.11 arm64 wheels; cython<3 for the
        // era's .pyx.
        return &["numpy==1.24.4", "cython==0.29.36", "scipy==1.10.1", "pandas==2.0.3", "matplotlib==3.7.3"];
    }
    if repo == "scikit-learn/scikit-learn" && year >= 2020 {
        // Modern eras resolve their own numpy/cython fine — no reality pins; pandas rides
        // along because the suite skips without it.
        return &["numpy", "cython", "pandas"];
    }
    match repo {
        // pyerfa's sdist build runs `erfa_generator`, which imports jinja2
        // (astropy__astropy-12907 live at dispatch, 2026-08-16).
        //
        // numpy is here for a DIFFERENT reason, and it is the one that made every astropy
        // instance die at dispatch (measured 2026-08-20, a 10-instance Lite round: 6938 and
        // 7746 both failed env build, 2 of 10 gone before a citizen saw them):
        //
        //     File ".../astropy/__init__.py", line 116, in <module>
        //       _check_numpy()
        //   ImportError: Numpy version 1.10.0 or later must be installed to use Astropy
        //
        // astropy's `setup.py` IMPORTS ITS OWN PACKAGE at build time, and the `-e .` install
        // below runs `--no-build-isolation` — so the build backend resolves against whatever
        // is already in the venv, with no PEP-517 scratch env to pull numpy into. numpy is
        // therefore a BUILD dependency of astropy itself, not merely a runtime one, and the
        // only place it can be satisfied is here, ahead of `-e .`.
        //
        // Still era-pinned like everything else in this pre-install: the version that lands
        // is the one the instance's own cutoff allows (the two failures above wanted >=1.10
        // and >=1.13 respectively — both far below any era cap we set), so this widens what
        // BUILDS without smuggling a modern numpy into a date-pinned subject graph.
        // NUMPY 2 IS A HARD WALL FOR PRE-2021 ASTROPY (measured 2026-08-28, live
        // dispatch): numpy 2.0 made `np.array(obj, copy=False)` RAISE instead of
        // silently copying, and 2017-era astropy calls exactly that inside
        // `Quantity._new_view` — so the env build dies with "Unable to avoid copy
        // while creating an array as requested" before a citizen ever sees the
        // task. A bare "numpy" here is the smuggling path this row's own comment
        // warns about: when the dated resolve hiccups, the era-pin's loud
        // unpinned retry lands a 2026 numpy in a 2017 subject graph.
        //
        // 1.21.6 is the same proven middle xarray uses above — arm64 wheels on
        // this box, and it still carries the old C macros these eras compile
        // against. Scoped to the OLD eras only: 2022+ astropy (12907, 13236,
        // 14365, 14995 — all currently PASSING) resolves its own numpy fine and
        // must not be disturbed by a ceiling it never needed.
        "astropy/astropy" if year <= 2020 => &["jinja2", "numpy==1.21.6"],
        "astropy/astropy" => &["jinja2", "numpy"],
        // Same class, measured at prewarm 2026-08-26 (the seeded-25 round): both
        // setup.py's import numpy at metadata time under --no-build-isolation.
        // scikit-learn additionally builds .pyx sources, so cython is a build
        // dep of the repo itself; matplotlib declares numpy via
        // `oldest-supported-numpy`, which routinely fails to resolve on a
        // modern interpreter (the build_requires step warns and proceeds) — a
        // plain era-pinned numpy here is what actually lands.
        // cython<3 is load-bearing: Cython 3 REJECTS 2019-era .pyx (CompileError
        // on sklearn's own ball_tree.pyx, walk 4 of the seeded-25 prewarm) — and
        // the era-pin's loud unpinned retry is exactly the path that smuggles
        // modern cython in when the dated resolve hiccups. The ceiling holds
        // through BOTH paths; 0.29.x compiles every sklearn era we can meet.
        // Platform-reality pins (proven build 2026-08-27): era numpy 1.16 has no
        // arm64 wheel and modern numpy 2.x breaks the 2019 build — 1.21.6 is the
        // oldest arm64-wheeled numpy that compiles this era. Same logic for the
        // cython and scipy pins. These are the exact versions the end-to-end
        // proof used; change only with a new end-to-end proof.
        "scikit-learn/scikit-learn" => &["numpy==1.21.6", "cython==0.29.24", "scipy==1.7.3"],
        // cppy: kiwisolver 1.3's sdist imports it at build (walk 4 — the
        // dependency-sdist class this table exists for, same as astropy→pyerfa→jinja2).
        "matplotlib/matplotlib" => &["numpy", "setuptools_scm", "certifi", "cppy"],
        _ => &[],
    }
}

/// Directories a legacy `setup.py` build writes INTO THE CHECKOUT, which must not survive into
/// the next attempt.
///
/// `.eggs/` is the one that actually kills runs; `build/` is included because a stale
/// `build/temp.<plat>-<pyver>/` silently reuses objects compiled against a different
/// interpreter or numpy ABI, which fails far downstream of here with an unrelated-looking
/// import error.
pub(super) const SETUPTOOLS_CHECKOUT_DEBRIS: &[&str] = &[".eggs", "build"];

/// Remove build debris a previous attempt left in the repo checkout.
///
/// # This made two astropy instances permanently unbuildable (measured 2026-08-20)
///
/// Pre-PEP-517 `setup.py` files declare `setup_requires`, and setuptools satisfies those by
/// downloading eggs into a `.eggs/` directory INSIDE THE CHECKOUT. On the next run,
/// `setuptools.wheel._convert_metadata` does a bare `os.mkdir(destination_eggdir)` on a path
/// that already exists and raises:
///
///     FileExistsError: [Errno 17] File exists:
///       .../astropy__astropy-6938/.eggs/markupsafe-3.0.3-py3.9-macosx-10.9-universal2.egg
///
/// So the FIRST failure — for whatever reason — poisons every attempt after it, forever. The
/// env teardown already deletes `env_dir` for exactly this reason ("a cached broken env would
/// poison every later run"), but the checkout is a SECOND piece of state the build writes and
/// nobody was clearing. Same bug, second location.
///
/// It was invisible because astropy's `__init__.py` catches the build failure and re-raises
/// `ImportError: astropy` with "try rerunning this command manually to see what the issue
/// was" — so the harness reported a generic import failure and the real `FileExistsError` was
/// four frames down inside a message we printed but never read. Confirmed by hand: with the
/// `.eggs/` dir present `build_ext --inplace` exits 1; `rm -rf .eggs` and the identical
/// command exits 0.
///
/// Called BEFORE the install (the debris may predate this process) and again on failure (so a
/// failing instance does not hand the next attempt a rigged tree) — the same
/// pristine-by-construction discipline `ensure_grade_checkout` applies to grading.
pub(super) fn clear_setuptools_build_debris(repo_dir: &Path) {
    for name in SETUPTOOLS_CHECKOUT_DEBRIS {
        let _ = std::fs::remove_dir_all(repo_dir.join(name));
    }
}

/// Per-repo build ENV additions that must ride EVERY editable install of the
/// repo — fresh build AND cached-env re-point (the re-point rebuilds the same
/// extensions; matplotlib's vendored-freetype `make` died there twice because
/// the fix rode only the fresh path). Returns owned pairs; empty for most repos.
pub(super) fn repo_build_env(instance: &SweInstance, env_dir: &Path) -> Result<Vec<(String, String)>, String> {
    if instance.repo == "scikit-learn/scikit-learn" {
        // OpenMP on Apple silicon (glass-boxed 2026-08-27, full ladder): Apple
        // clang rejects bare -fopenmp; numpy.distutils ignores env
        // CFLAGS/CPPFLAGS entirely, so the ONLY reliable carrier is flags
        // EMBEDDED IN $CC/$LDSHARED (verified: the exact failing clang line
        // gained the flags and the C build completed). Needs brew's libomp —
        // the ensure_env gate below names the remedy when absent.
        //
        // KNOWN STRUCTURAL LIMIT, honestly held: sklearn < 0.22 (pre-2019-12)
        // vendors a cloudpickle that cannot IMPORT on python >= 3.8
        // (types.CodeType signature), and no 3.7 exists for this platform
        // (uv ships none for macOS; CPython 3.7 predates Apple silicon). Those
        // instances stay env-absent natively; the parity path is the official
        // harness's era containers (docker fallback, tracked on the roadmap).
        let libomp = std::path::Path::new("/opt/homebrew/opt/libomp");
        if libomp.join("include/omp.h").exists() {
            let inc = libomp.join("include").to_string_lossy().into_owned();
            let lib = libomp.join("lib").to_string_lossy().into_owned();
            return Ok(vec![
                (
                    "CC".into(),
                    // The ONE era flag set rides here too — an inline copy of three
                    // of its rows drifted the day the fourth was added (2026-09-16).
                    format!("clang -Xpreprocessor -fopenmp -I{inc} {ERA_CFLAGS}"),
                ),
                (
                    "LDSHARED".into(),
                    format!("clang -bundle -undefined dynamic_lookup -L{lib} -lomp"),
                ),
                // sklearn's darwin branch suppresses bare -fopenmp only when
                // "openmp" appears in CPPFLAGS — a valid no-op define carries it.
                ("CPPFLAGS".into(), "-Dopenmp_via_libomp=1".into()),
            ]);
        }
        return Err(format!(
            "scikit-learn needs OpenMP and this host has no libomp: install it with \
             `brew install libomp` and re-run — an ENV prerequisite, not a model \
             result ({})",
            instance.instance_id
        ));
    }
    if instance.repo == "matplotlib/matplotlib" {
        // System freetype via MPLSETUPCFG (vendored 2.6.1 predates Apple Silicon).
        let cfg = env_dir.join("mplsetup.cfg");
        std::fs::create_dir_all(env_dir)
            .map_err(|e| format!("could not create {}: {e}", env_dir.display()))?;
        std::fs::write(&cfg, "[libs]\nsystem_freetype = true\n")
            .map_err(|e| format!("could not write {}: {e}", cfg.display()))?;
        return Ok(vec![(
            "MPLSETUPCFG".to_string(),
            cfg.to_string_lossy().into_owned(),
        )]);
    }
    Ok(Vec::new())
}

/// Re-assert the PEP-660 harness floor: setuptools in [64, 70) — the only
/// window with BOTH `build_editable` (>=64) and `dep_util` (<70). Setuptools is
/// HARNESS, never subject ([[…]] doctrine at the build_requires site): an
/// era-pinned resolve that sweeps it to a dated version (sympy-12481's 2017 pin
/// left 34.3.3 — no `setuptools.build_meta` AT ALL) breaks every later
/// editable rebuild. Idempotent and seconds-cheap; call it after ANY step that
/// can move setuptools.
pub(super) async fn assert_harness_floor(uv: &str, py: &str) -> Result<(), String> {
    // The floor is a COHERENT TRIO, not setuptools alone (sympy-12481/12489
    // regressed twice before this was learned): wheel 0.44+ delegates
    // bdist_wheel to setuptools.command.bdist_wheel, which only exists in
    // setuptools >=70.1 — above our <70 dep_util ceiling — so wheel must stay
    // <0.44 (self-contained bdist_wheel). And an era-swept packaging (2017's
    // 16.8) lacks packaging.tags, which wheel imports. All three are HARNESS:
    // pure metadata machinery no subject imports, safe at modern-in-window
    // versions inside any era graph.
    let _ = run(
        uv,
        &[
            "pip",
            "install",
            "-q",
            "--python",
            py,
            "setuptools>=64,<70",
            "wheel>=0.40,<0.44",
            "packaging>=21",
        ],
        None,
    )
    .await?;
    Ok(())
}

pub async fn ensure_env(instance: &SweInstance, repo_dir: &Path) -> Result<PathBuf, String> {
    if let Some(name) = instance.log_parser.as_deref() {
        let Some(parser) = LogParser::from_name(name) else {
            return Err(unsupported_harness(instance, name));
        };
        return toolchain_on_path(parser.toolchain()).await;
    }
    // PER-INSTANCE SERIALIZATION — this function used to rely on "solve/grade
    // run serially per instance"; the dispatch-time env PRE-WARM broke that
    // assumption, and two concurrent builders writing one venv is a corrupt
    // env with no error. The lock is the constraint fixed at the seam every
    // caller shares (never a caller-side convention).
    static ENV_LOCKS: std::sync::OnceLock<
        dashmap::DashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>,
    > = std::sync::OnceLock::new();
    let lock = ENV_LOCKS
        .get_or_init(dashmap::DashMap::new)
        .entry(instance.instance_id.clone())
        .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _held = lock.lock().await;
    let env_dir = swe_cache_dir().join("envs").join(&instance.instance_id);
    // uv creates a native venv: Windows puts its interpreter under Scripts,
    // Unix under bin. Use the same path for cache reuse and initial installation.
    let py = env_dir.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    });
    if py.exists() {
        // THE EDITABLE POINTS SOMEWHERE (root cause of the "2019-pytest era" + "flask-2.2
        // era" env-void classes, glass-boxed 2026-08-12): the env is cached per INSTANCE,
        // but its `-e .` install pins the ABSOLUTE PATH of whichever tree built it first —
        // the solver's workspace. A flat-layout repo (sympy, pylint) survives because the
        // grade clone's cwd shadows the import; a src/-layout repo (pytest 4.x, flask 2.2)
        // does not, so the grade imported the persona's DIRTY WORKSPACE code and pristine
        // p2p read 0/N (six instances voided). Re-point the editable at THIS caller's tree:
        // `--no-deps` skips dependency resolution (the graph is already in the venv), so
        // this is a seconds-cheap metadata rebuild, and solve/grade run serially per
        // instance so there is no cross-tree race.
        if let Some(uv) = which("uv") {
            let py_s = py.to_string_lossy().to_string();
            // The cached env may hold an era-swept setuptools (34.x has no
            // build_meta) — floor it before asking for an editable rebuild.
            assert_harness_floor(&uv, &py_s).await?;
            let repo_env = repo_build_env(instance, &env_dir)?;
            let mut env_pairs: Vec<(&str, &str)> = ERA_BUILD_ENV.to_vec();
            env_pairs.extend(repo_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let out = run_env(
                &uv,
                &[
                    "pip",
                    "install",
                    "-q",
                    "--python",
                    &py_s,
                    "--no-build-isolation",
                    "--no-deps",
                    "-e",
                    ".",
                ],
                Some(repo_dir),
                &env_pairs,
            )
            .await?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let tail: String = {
                    let t = stderr.trim();
                    t.chars().skip(t.chars().count().saturating_sub(1500)).collect()
                };
                return Err(format!(
                    "{ENV_CACHE_REPOINT_FAILED}: could not re-point {}'s cached env at {}: {}",
                    instance.instance_id,
                    repo_dir.display(),
                    tail
                ));
            }
            // An env cached BEFORE the test extra existed is silently ungradeable (its
            // conftest cannot import). Heal it here, once, guarded by the marker — the
            // operator should never have to know which envs predate a fix.
            ensure_test_extra(&uv, &py_s, &env_dir, repo_dir, era_cutoff(instance).as_deref())
                .await;
        }
        return Ok(py);
    }
    // ANY failure below leaves NO half-built env: the interpreter is the "env is complete"
    // key, so a venv whose deps never installed (the index unreachable, card cffc9c5e)
    // would otherwise be adopted as complete on the next run and grade every attempt
    // ungradeable. Each fatal site still removes on its own; this is the floor under them.
    let built = build_env_from_scratch(instance, repo_dir, env_dir.clone(), py.clone()).await;
    if built.is_err() {
        let _ = std::fs::remove_dir_all(&env_dir);
    }
    built
}
