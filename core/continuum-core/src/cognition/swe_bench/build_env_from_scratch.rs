use super::*;

pub(super) async fn build_env_from_scratch(
    instance: &SweInstance,
    repo_dir: &Path,
    env_dir: PathBuf,
    py: PathBuf,
) -> Result<PathBuf, String> {
    let _ = std::fs::create_dir_all(env_dir.parent().unwrap_or(&env_dir));
    // A missing HOST tool is a failure to LOOK, never a fact about the instance: typed
    // by its head so the sweep defers and re-tries every tick instead of recording a
    // standing refusal that only a change in the citizen's WORK can lift (card 1aac0c72:
    // sphinx-7889 on the 5090 sat unre-graded for hours after uv appeared on the box).
    let uv = which("uv").ok_or_else(uv_missing_is_could_not_look)?;
    let interpreter = interpreter_for_year(instance.year());
    let env_s = env_dir.to_string_lossy().to_string();
    let py_s = py.to_string_lossy().to_string();

    let out = run(&uv, &["venv", "--python", interpreter, "-q", &env_s], None).await?;
    if !out.status.success() {
        return Err(format!(
            "could not create a Python {interpreter} venv: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // THE SPLIT: SUBJECT vs HARNESS.
    //
    // Subject — the repo's own runtime dependency graph — is pinned to the instance's date,
    // because those versions define the behavior under test. A plain install resolves 2026
    // packages against 2021 code: flask 2.0 asks for `Werkzeug>=2.0`, gets 3.x, which deleted
    // `url_quote`, and the repo cannot even import.
    //
    // Harness — pytest, setuptools, wheel — is deliberately MODERN, because it has to run on
    // this machine's interpreter. Date-pinning it breaks in ways unrelated to the instance:
    // 2021 `py` raises `AttributeError: __spec__` under 3.11's import machinery, and 2021
    // setuptools predates PEP 660 so `build_meta:__legacy__` has no `build_editable`.
    //
    // `--no-build-isolation` is what lets the two coexist: the build runs against the modern
    // setuptools already in the venv instead of pip fetching a date-pinned one.
    //
    // setuptools is pinned `<70`, NOT bare-latest. Two era-2020..2022 build requirements fight:
    // an editable install (`-e .`) needs PEP 660 `build_editable`, which landed in setuptools 64;
    // and those repos' C-extension setup code imports legacy APIs like `setuptools.dep_util`
    // (astropy's `wcs/setup_package.py`), which setuptools REMOVED in 70.0. setuptools 69.x is the
    // only version that has BOTH — bare-latest (>=70) builds the pure-Python repos but dies on
    // every C-extension instance with `ModuleNotFoundError: setuptools.dep_util`. This is the whole
    // era class, not one repo (#380 "pin era deps"). pytest/wheel stay latest.
    let _ = run(
        &uv,
        &[
            "pip",
            "install",
            "-q",
            "--python",
            &py_s,
            "pytest",
            "setuptools<70",
            "wheel",
        ],
        None,
    )
    .await?;

    // BUILD REQUIRES: `--no-build-isolation` means pip will NOT fetch the repo's declared
    // build-time dependencies — it builds against whatever is already in the venv. A repo whose
    // setup.py imports a build helper (astropy: `import extension_helpers`; any C-extension repo:
    // cython, numpy) therefore fails with `ModuleNotFoundError` at the metadata step unless we
    // pre-install what its own `[build-system].requires` declares. We honor that declaration
    // rather than hardcoding per-repo build deps — the pyproject IS the source of truth. Installed
    // MODERN (no date-pin) like pytest/setuptools/wheel above: these are build harness, run on
    // THIS interpreter, and carry their own version pins where they matter (e.g. cython==0.29.22).
    let build_reqs = build_requires(repo_dir);
    if !build_reqs.is_empty() {
        let mut breq_args = vec!["pip", "install", "-q", "--python", &py_s];
        breq_args.extend(build_reqs.iter().map(String::as_str));
        let out = run(&uv, &breq_args, None).await?;
        if !out.status.success() {
            // Non-fatal: some declared build deps (e.g. `oldest-supported-numpy` on a fresh
            // interpreter) may not resolve, yet the build can still succeed against the modern
            // setuptools already present. Let the `-e .` step below be the real gate; surface this
            // only as a breadcrumb if that step then fails.
            tracing::warn!(
                instance = %instance.instance_id,
                requires = ?build_reqs,
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "build-system.requires install had non-zero exit — proceeding to -e . anyway"
            );
        }
        // RE-ASSERT THE HARNESS FLOOR (pylint-7114, live 2026-08-12): a repo whose own
        // `[build-system].requires` pins an OLDER setuptools (pylint 2022 pins ~62.6)
        // just CLOBBERED the 69.x we installed above — and setuptools <64 predates PEP
        // 660, so the editable build dies with "build_meta has no attribute
        // build_editable". Honoring the repo's declaration is right for ITS build
        // helpers (cython, extension-helpers); setuptools itself is HARNESS, and the
        // window [64, 70) is the only range with BOTH build_editable (>=64) and
        // dep_util (<70) — the same two-sided constraint documented above.
        let _ = run(
            &uv,
            &[
                "pip",
                "install",
                "-q",
                "--python",
                &py_s,
                "setuptools>=64,<70",
            ],
            None,
        )
        .await?;
    }

    let as_of = era_cutoff(instance);
    let repo_s = repo_dir.to_string_lossy().to_string();

    // DEPENDENCY-SDIST BUILD DEPS (#383 cause 2, reproduced live 2026-08-16): the
    // `build_requires` step above honors the TOP repo's `[build-system].requires`, but
    // `--no-build-isolation` extends to every DEPENDENCY built from sdist too — and their
    // declared build deps are honored by NOBODY. astropy pulls `pyerfa`, whose sdist runs an
    // `erfa_generator` step importing jinja2 at build time; with no wheel for this
    // interpreter/arch the sdist build is mandatory, and the whole env died with
    // `ModuleNotFoundError: No module named 'jinja2'` — every astropy instance ungradeable.
    // The table is DATA (repo → the build deps its dependency sdists need), the same shape as
    // the official harness's per-repo spec tables. Era-pinned with the instance's own cutoff
    // so a build tool never smuggles a modern package into a date-pinned subject graph
    // (jinja2 IS a runtime dep of some subjects, e.g. flask).
    let sdist_deps = era_sdist_build_deps_for(&instance.repo, instance.year());
    if !sdist_deps.is_empty() {
        // Shares `era_pinned_uv_install` with the `-e .` install below. It did NOT before,
        // and that asymmetry is what made every astropy instance ungradeable: the pin caps
        // setuptools at the 2017 cutoff while markupsafe's build requires >=40.8.0, and the
        // heal that reads uv's own `exclude-newer-package` hint lived only at the other site.
        let mut out = era_pinned_uv_install(
            &uv,
            &py_s,
            as_of.as_deref(),
            sdist_deps,
            None,
            ERA_BUILD_ENV,
        )
        .await?;
        // BUILD TOOLS ARE NOT SUBJECT CODE — reproduced in isolation 2026-08-17: the era
        // pin on this pre-install is UNSATISFIABLE BY CONSTRUCTION on a modern box, at
        // every rung the heal can reach. Era markupsafe 1.0 (no wheel for this
        // interpreter/arch, sdist build mandatory) needs setuptools>=40.8.0 which the pin
        // excludes; LIFT setuptools (the heal's correct first move, verified firing) and
        // the 2017 sdist dies on `ImportError: cannot import name 'Feature'` (removed in
        // setuptools 46); lift markupsafe instead and era jinja2 2.10 dies at import on
        // `soft_unicode` (removed in markupsafe 2.1). The only installable combination is
        // MODERN jinja2 + MODERN markupsafe — verified importing clean.
        //
        // So when the healed era-pinned install still fails, retry ONCE unpinned, loudly.
        // Scope-safe by construction: this table lists BUILD TOOLS for dependency-sdist
        // code generators (astropy→pyerfa→jinja2), never subject requirements — a repo
        // where the package IS subject code (flask) has no entry here, and the subject
        // graph is resolved by the still-date-pinned `-e .` step below.
        if !out.status.success() && as_of.is_some() {
            tracing::warn!(
                instance = %instance.instance_id,
                deps = ?sdist_deps,
                "era-pinned sdist build-dep install unsatisfiable at every heal rung — \
                 retrying UNPINNED (build tools only; the subject graph stays date-pinned)"
            );
            out = era_pinned_uv_install(&uv, &py_s, None, sdist_deps, None, ERA_BUILD_ENV)
                .await?;
        }
        if !out.status.success() {
            // Fail LOUD and leave no half-built env behind — same doctrine as the `-e .` gate.
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "could not pre-install {}'s dependency-sdist build deps {:?} — env removed \
                 rather than cached broken: {}",
                instance.instance_id,
                sdist_deps,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    // DELETED-HISTORY packages: `--exclude-newer <date>` assumes PyPI still carries the
    // uploads that existed at that date. It does not always — `atomicwrites` had its whole
    // pre-2022 history deleted by its author, so for any pre-2022 instance the date pin
    // leaves ZERO candidates and resolution fails outright (pytest-dev__pytest-5103, live
    // 2026-08-11). uv's error names both the package and the earliest surviving upload, and
    // its own suggested remedy is a per-package `--exclude-newer-package` cutoff. We heal
    // exactly as instructed: parse the hint, override ONLY that package to just past its
    // earliest surviving upload, and retry. The subject stays date-pinned; the override is
    // the minimum needed for a resolvable graph, discovered from the resolver's own evidence
    // rather than a hand-maintained package list. Bounded: each round must surface a NEW
    // package or we stop, and history-holes per graph are few.
    // ERA_CFLAGS rides on this invocation because it is where every C build happens — the
    // repo's own extensions AND its dependency sdists (pyerfa et al) compile inside this
    // resolve. The heal loop lives in `era_pinned_uv_install`, shared with the sdist
    // build-dep pre-install above.
    // The env teardown below deletes `env_dir` — but a failed build ALSO leaves debris in the
    // CHECKOUT, which nothing was cleaning, and that debris is permanently fatal.
    clear_setuptools_build_debris(Path::new(&repo_s));
    // MATPLOTLIB BUILDS AGAINST SYSTEM FREETYPE (glass-boxed 2026-08-26): its
    // default build downloads freetype 2.6.1 and `make`s it — and that
    // tarball's config.guess predates Apple Silicon, so the vendored build
    // dies on every arm64 Mac. `MPLSETUPCFG` → `system_freetype = true` links
    // the host's freetype instead (verified: mpl 3.4.2 builds + imports on the
    // era interpreter). Fail-loud when the host lacks it: the message names
    // the exact install command, because "build backend error" is not a fix.
    let mut build_env: Vec<(&str, &str)> = ERA_BUILD_ENV.to_vec();
    // Host-prereq gate for matplotlib's system-freetype build — fail LOUD with
    // the remedy; "build backend error" is not a fix.
    if instance.repo == "matplotlib/matplotlib" {
        let freetype_ok = match which("pkg-config") {
            Some(pc) => run(&pc, &["--exists", "freetype2"], None)
                .await
                .map(|o| o.status.success())
                .unwrap_or(false), // a failing probe means "not available" — the message below names the remedy
            None => false,
        };
        if !freetype_ok {
            let _ = std::fs::remove_dir_all(&env_dir);
            // A HOST prerequisite, not a model result — and not the instance's standing
            // refusal either: typed COULD_NOT_LOOK so the sweep looks again after
            // `brew install` instead of holding the marker until the citizen's work changes.
            return Err(host_prereq_is_could_not_look(
                "the HOST freetype that matplotlib needs (its vendored freetype 2.6.1 cannot \
                 build on this machine)",
                &format!(
                    "install it with `brew install freetype pkg-config` and re-run ({})",
                    instance.instance_id
                ),
            ));
        }
    }
    let repo_env = repo_build_env(instance, &env_dir)?;
    build_env.extend(repo_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let out = era_pinned_uv_install(
        &uv,
        &py_s,
        as_of.as_deref(),
        &["--no-build-isolation", "-e", "."],
        Some(Path::new(&repo_s)),
        &build_env,
    )
    .await?;
    if !out.status.success() {
        // DELETE the half-built env rather than cache it. Keying on "the directory exists"
        // made a failed install sticky: every later run reused a venv with no repo in it and
        // reported a gold failure whose real cause was three steps upstream.
        //
        // Same reasoning, the other half of the state: the build writes into the CHECKOUT too,
        // and that debris outlives `env_dir` (see `clear_setuptools_build_debris`).
        clear_setuptools_build_debris(Path::new(&repo_s));
        let _ = std::fs::remove_dir_all(&env_dir);
        // uv's stderr often carries ONLY "the build backend returned an error";
        // the backend's actual traceback lands on stdout. Carry BOTH tails so
        // the prewarm probe / user receipt is actionable, never a mystery.
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let tail = |t: &str| -> String {
            let t = t.trim();
            t.chars().skip(t.chars().count().saturating_sub(1500)).collect()
        };
        return Err(format!(
            "could not install {}'s repo into a venv — a cached broken env would poison every \
             later run, so it was removed: {} ||| backend output tail: {}",
            instance.instance_id,
            tail(&stderr),
            tail(&stdout)
        ));
    }

    // The era-pinned `-e .` resolve can sweep setuptools into the dated subject
    // graph (sympy-12481: 2017 pin → setuptools 34.3.3, no build_meta) — every
    // later re-point then dies. Floor it back: harness, never subject.
    assert_harness_floor(&uv, &py_s).await?;

    // PYTEST IS SUBJECT, NOT HARNESS, for date-pinned instances (#380, glass-boxed
    // 2026-08-12): era test suites import pytest INTERNALS — flask 2.2's test_cli.py does
    // `from _pytest.monkeypatch import notset`, deleted by modern pytest — so the
    // latest-pytest installed above voids every such baseline. The era interpreter rungs
    // (#2253) removed the original reason pytest had to be modern, so downgrade it to the
    // instance's own date. Skipped when the repo IS pytest: the editable install already
    // provides the (exactly-era) subject and a PyPI pytest would stomp it. Skipped too when
    // the repo's runner isn't pytest AT ALL (#383: django grades through its own
    // runtests.py) — era-pinning a harness the grade never invokes is dead weight, and the
    // pytest smoke gate below would DELETE a perfectly good env whenever the era pytest
    // can't execute on this interpreter, voiding instances pytest has nothing to do with.
    if as_of.is_some()
        && instance.repo != "pytest-dev/pytest"
        && runner_for_repo(&instance.repo) == TestRunner::Pytest
    {
        let date = instance.created_at.clone();
        // --reinstall is load-bearing: the modern pytest above already satisfies the bare
        // requirement, so without it this resolve is a no-op (hand-verified: flask-5063
        // stayed on 9.1.1 until --reinstall brought it to era 7.3.0, tests then green).
        //
        // THROUGH THE HEALING INSTALLER (2026-08-27 coverage map, ~23 instances):
        // pre-2018 cutoffs exclude every usable setuptools (pytest's own sdist build
        // needs >=40.8), and uv's hint names the per-package override remedy —
        // which era_pinned_uv_install already parses and applies. The plain `run`
        // here was the one era-resolve NOT routed through the heal loop.
        let out = era_pinned_uv_install(
            &uv,
            &py_s,
            Some(date.as_str()),
            &["--reinstall", "pytest"],
            None,
            ERA_BUILD_ENV,
        )
        .await?;
        if !out.status.success() {
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "could not era-pin pytest for {} (cutoff {date}) — env removed rather than \
                 cached broken: {}",
                instance.instance_id,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }

        // GOLD-GATE THE HARNESS (#380's other half). The era pin above assumes the era
        // INTERPRETER rung (#2253) found a matching interpreter. When it can't — no
        // Python 3.5 on a modern macOS — the venv falls back to a modern interpreter
        // while pytest is still pinned to the instance's year, and that PAIR can be
        // structurally unable to run. Glass-boxed 2026-08-13 on sympy__sympy-11400
        // (2016 → pytest 2.9.2 on Python 3.9.6): pytest dies in `pytest_configure`
        // with INTERNALERROR before collecting anything, on a two-line trivial test
        // with no conftest and no sympy involved. Every test then "fails", the
        // pristine p2p reads 0/29, and the whole tree grades UNGRADEABLE — 8 of 36
        // instances on this box.
        //
        // A version pin is a GUESS about compatibility; running it is the evidence. So
        // prove the harness executes before handing the env to a citizen.
        //
        // This REFUSES rather than self-heals, and that is deliberate — measured, not
        // assumed. The obvious repair (reinstall a modern pytest) was tried against this
        // exact instance and does NOT work: pytest 8.4.2 loads, then dies in sympy 1.0's
        // own 2016 conftest on the `py.path` hook API that pytest 7 removed; pytest 6.2.5
        // dies on `py.test.mark.slow`, removed in pytest 4. The band of pytest versions
        // that both RUN on Python 3.9 and LOAD a 2016 conftest is EMPTY, so no version
        // choice rescues this class. What does work — verified on this tree, 30/30 passing
        // — is sympy's OWN runner (`sympy.test(...)`) on the same interpreter. That is
        // #383's shape ("django needs its OWN test runner") generalised: the runner is a
        // property of the repo era, not a pytest version to search for. `run_tests` is
        // pytest-only today, so until it grows a runner seam, an env in this state cannot
        // produce a verdict and must say so loudly instead of caching a tree where every
        // test errors ([[brittleness-is-the-highest-priority-work-there-is]] — heal what is
        // known-safe, REPORT what needs a human decision; a wrong auto-repair here would
        // silently trade one void tree for another).
        if let Err(why) = smoke_test_pytest(&py, &env_dir).await {
            let _ = std::fs::remove_dir_all(&env_dir);
            return Err(format!(
                "{}'s env has no runnable test harness: era-pinned pytest (cutoff {}) \
                 cannot execute even a trivial test on this venv's interpreter, which \
                 means the era INTERPRETER rung fell back to a modern one and left an \
                 incompatible pair. Env removed rather than cached — a cached copy grades \
                 every attempt UNGRADEABLE. This repo era likely needs its own native \
                 runner rather than any pytest version (#383). Detail: {why}",
                instance.instance_id, instance.created_at
            ));
        }
    }
    // LAST, because it is additive and must not be able to fail the build: the repo's own
    // test extra. Everything above decides whether the env can BUILD and RUN a harness;
    // this decides whether the suite can COLLECT. Non-fatal by design — see
    // `ensure_test_extra`.
    ensure_test_extra(&uv, &py_s, &env_dir, repo_dir, as_of.as_deref()).await;
    Ok(py)
}

/// Can this venv's pytest actually RUN? Not "is it installed", not "does `--version`
/// answer" — both stay true for a pytest that dies in `pytest_configure` (sympy-11400:
/// `pytest --version` prints 2.9.2 happily, then INTERNALERRORs on any real run).
///
/// So: write a trivial passing test to a scratch dir OUTSIDE the repo (no conftest, no
/// subject imports — a failure here is the harness, never the code under test) and
/// require a clean exit. This is the smallest honest question, and it is the one the
/// grader's whole verdict rests on.
pub(super) async fn smoke_test_pytest(py: &std::path::Path, env_dir: &std::path::Path) -> Result<(), String> {
    let smoke_dir = env_dir.join(".harness-smoke");
    std::fs::create_dir_all(&smoke_dir)
        .map_err(|e| format!("could not create the harness smoke dir: {e}"))?;
    std::fs::write(
        smoke_dir.join("test_harness_smoke.py"),
        "def test_the_harness_can_run():\n    assert True\n",
    )
    .map_err(|e| format!("could not write the harness smoke test: {e}"))?;
    let out = run(
        &py.to_string_lossy(),
        &["-m", "pytest", "-q", "test_harness_smoke.py"],
        Some(&smoke_dir),
    )
    .await?;
    if out.status.success() {
        return Ok(());
    }
    // stderr carries the INTERNALERROR trace; stdout carries collection errors.
    let detail = {
        let e = String::from_utf8_lossy(&out.stderr);
        let s = if e.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).to_string()
        } else {
            e.to_string()
        };
        s.lines().rev().take(3).collect::<Vec<_>>().join(" | ")
    };
    Err(format!(
        "a trivial one-assert test did not pass under this venv's pytest: {detail}"
    ))
}

/// Parse uv's deleted-history hint into an `--exclude-newer-package` value (`pkg=cutoff`).
///
/// The hint shape (uv 0.11):
/// ```text
/// hint: `atomicwrites` was filtered by `exclude-newer` to only include packages uploaded
/// before 2019-04-13T16:17:45Z. The latest version satisfying the requirement is v1.4.1,
/// published at 2022-07-08T18:31:40.459Z. Consider using `exclude-newer-package` to
/// override the cutoff for this package.
/// ```
/// The cutoff is that surviving upload's timestamp plus one second — `--exclude-newer-package`
/// is exclusive ("prior to"), so the publish instant itself must land inside the window, and
/// one second past it admits nothing else.
pub(super) fn deleted_history_override(stderr: &str) -> Option<String> {
    let hint_at = stderr.find("Consider using `exclude-newer-package`")?;
    let region = &stderr[..hint_at];
    let pkg = {
        let rest = &region[region.rfind("hint: `")? + "hint: `".len()..];
        rest.split('`').next()?
    };
    let published = {
        let rest = &region[region.rfind("published at ")? + "published at ".len()..];
        &rest[..=rest.find('Z')?]
    };
    let ts = chrono::DateTime::parse_from_rfc3339(published).ok()?;
    let cutoff =
        (ts + chrono::Duration::seconds(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}={cutoff}"))
}

/// Parse uv's metadata-version-mismatch failure into an `--exclude-newer-package` value
/// that LIFTS the era cutoff for the one unbuildable package.
///
/// The failure shape (uv 0.11, live pylint-5859 2026-08-11):
/// ```text
/// ╰─▶ Package metadata version `0.0.0` does not match given version `1.7.1`
/// hint: `lazy-object-proxy` (v1.7.1) was included because `pylint` (v2.13.0.dev0) ...
/// ```
/// The `0.0.0` is an sdist built without git metadata (setuptools_scm fallback) — it happens
/// exactly when the era release ships no wheel for this platform, so no date-pinned retry can
/// ever succeed. Lifting that ONE package's cutoff (far-future bound) lets uv take a modern
/// wheel-shipping release; the rest of the graph stays era-pinned. Verified live: pylint
/// 2.13.0-dev0 + astroid 2.9.3 (era-correct) with only the shim floated.
pub(super) fn metadata_mismatch_override(stderr: &str) -> Option<String> {
    if !stderr.contains("Package metadata version `0.0.0` does not match given version") {
        return None;
    }
    let rest = &stderr[stderr.find("hint: `")? + "hint: `".len()..];
    let pkg = rest.split('`').next()?;
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}=9999-01-01T00:00:00Z"))
}

/// Third heal arm: an era `importlib-metadata` 0.x in the graph (2019 pluggy 0.12 pulls it
/// unconditionally) crashes the MODERN setuptools that `--no-build-isolation` builds with —
/// setuptools' own banner names the clash and its own remedy is "install an updated
/// version" (setuptools/importlib_metadata#396). Lift that one package's cutoff, exactly
/// the metadata-mismatch shape (live: pytest-5413 fresh env, 2026-08-12; sibling 5221 is
/// one pluggy release older and never pulls it).
pub(super) fn setuptools_importlib_clash_override(stderr: &str) -> Option<String> {
    if stderr.contains("`importlib-metadata` version is incompatible with `setuptools`") {
        Some("importlib-metadata=9999-01-01T00:00:00Z".to_string())
    } else {
        None
    }
}

/// How many per-package cutoff overrides one install may discover before we stop.
///
/// Each round must surface a NEW package (see the loop), so this bounds a pathological
/// graph rather than a healthy one — history-holes per graph are few, and the live maximum
/// observed across Lite is two.
pub(super) const MAX_ERA_OVERRIDES: usize = 8;

/// The head of an env-build error that is a failure to LOOK, never a fact about the
/// instance: the resolver could not reach the index. Card cffc9c5e (2026-09-16 07:28Z,
/// the router down in a power outage): uv's "dns error … Failed to fetch
/// https://pypi.org/simple/cython/" was recorded as xarray-4356's standing env refusal,
/// the env removed, and the REAL env fault (pandas 1.1.0 under -Werror, #4118) had to be
/// re-derived by hand. COULD_NOT_LOOK ≠ NOT_FOUND ([[unknown-is-not-a-quantity]]); a
/// reader that sees this head defers and looks again, it never records a verdict.
pub const COULD_NOT_LOOK: &str = "COULD NOT LOOK — ";

/// ONE answer to "how is a missing HOST prerequisite recorded": a [`COULD_NOT_LOOK`] —
/// the box could not look at the instance at all, so no marker stands and the next sweep
/// looks again. The marker's own invalidation rule (`refusal_stands_at`) watches the
/// citizen's WORK mtime and cannot see a tool appearing on the host; a bare reason from
/// any of these gates therefore stood forever (card 1aac0c72: sphinx-7889 after uv
/// appeared on the 5090; every matplotlib graded before `brew install freetype`).
/// `what` names the prerequisite, `remedy` says how to put it on the box.
pub fn host_prereq_is_could_not_look(what: &str, remedy: &str) -> String {
    format!(
        "{COULD_NOT_LOOK}{what} is not available on this box — {remedy}; nothing about the \
         instance is known until it is"
    )
}

/// `uv` — the one tool that builds every per-instance environment — is not on this
/// process's PATH.
pub fn uv_missing_is_could_not_look() -> String {
    host_prereq_is_could_not_look(
        "uv",
        "it builds the per-instance environment (a Rust binary, install from \
         https://astral.sh/uv)",
    )
}

/// Does uv's stderr say the INDEX was unreachable (resolver/network), as opposed to a
/// package that would not resolve or build? Anchored on uv's/reqwest's own phrasing for
/// transport failures; a build backend error, a missing wheel, or an unsatisfiable pin
/// never carries one of these.
pub fn network_failure_signature(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    [
        "dns error",
        "failed to fetch",
        "request failed after",
        "error sending request",
        "connection refused",
        "connection reset",
        "network is unreachable",
        "no route to host",
        "temporary failure in name resolution",
        "operation timed out",
        "connect timed out",
    ]
    .iter()
    .any(|sig| s.contains(sig))
}

/// ONE era-pinned `uv pip install`, healing from uv's own evidence.
///
/// # Why this is a function and not two copies of a loop
///
/// "Install under the instance's date pin, and when the resolver hits a hole that the pin
/// itself created, read uv's hint and retry with the minimum per-package override" is ONE
/// decision. It had TWO call sites in `ensure_env` — the editable `-e .` install, which had
/// the heal loop, and the dependency-sdist build-dep pre-install, which did not — so the
/// same class of failure was survivable at one site and fatal at the other.
///
/// Measured 2026-08-17: every astropy instance in swe-bench-lite's head died at the
/// unhealed site with `could not pre-install …'s dependency-sdist build deps ["jinja2"]`.
/// The cause is the pin working exactly as designed: jinja2 2.10 needs markupsafe 1.0,
/// whose `setup.py` build requires `setuptools>=40.8.0`, while the 2017 cutoff caps
/// setuptools at 38.2.4 — unsatisfiable by construction. uv's stderr named the package and
/// suggested `exclude-newer-package`, which is precisely what the loop 30 lines below knew
/// how to parse. Extracting it heals both sites and makes a third site inherit the
/// behaviour by construction.
///
/// `tail` is the call-site-specific argv after the shared pin flags (`["jinja2"]`, or
/// `["--no-build-isolation", "-e", "."]`). `as_of: None` disables pinning AND healing —
/// with no pin there is no pin-induced hole to heal.
pub(super) async fn era_pinned_uv_install(
    uv: &str,
    py: &str,
    as_of: Option<&str>,
    tail: &[&str],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut overrides: Vec<String> = Vec::new();
    loop {
        let mut args = vec!["pip", "install", "-q", "--python", py];
        if let Some(date) = as_of {
            args.push("--exclude-newer");
            args.push(date);
        }
        for pin in &overrides {
            args.push("--exclude-newer-package");
            args.push(pin);
        }
        args.extend(tail.iter().copied());
        let out = run_env(uv, &args, cwd, envs).await?;
        if out.status.success() {
            return Ok(out);
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        // Every install failure says whether the transport signature matched (Cormac's
        // review of #4130): the signature is a substring match on uv's prose, and a
        // rewording upstream would fail SILENTLY toward the wrong side — a transport
        // failure recorded as the instance's env verdict, card cffc9c5e again. A step
        // change in unmatched failures is how that drift is seen the day it lands.
        let matched = network_failure_signature(&stderr);
        crate::probe!(
            class = "benchmark.env.install_failed",
            signature_matched = matched,
            tail = ?tail,
            stderr_head = %stderr.trim().chars().take(200).collect::<String>(),
            "an era-pinned uv install failed — matched = the transport signature read it as COULD NOT LOOK"
        );
        // A failure to REACH the index is not a resolution failure: no heal rung fixes DNS,
        // and no caller may read it as the instance's env verdict. Typed by its head so
        // every reader (the sweep, the refusal marker, the deck) can tell it apart.
        if matched {
            let last: String = stderr.trim().chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
            return Err(format!(
                "{COULD_NOT_LOOK}the resolver could not reach the package index during \
                 `uv pip install {tail:?}` — nothing about the instance is known; look \
                 again once a fetch succeeds: {last}"
            ));
        }
        if as_of.is_none() {
            return Ok(out);
        }
        // Three heal arms, same bounded loop: (1) deleted-history — the date pin leaves zero
        // candidates and uv's hint names the earliest surviving upload; (2) metadata-mismatch
        // — an era sdist with no wheel for this platform builds as version 0.0.0
        // (setuptools_scm without git metadata), so the ONE unbuildable package's cutoff is
        // lifted entirely; (3) the setuptools/importlib-metadata clash. All three parse uv's
        // OWN evidence; no hand-maintained package list.
        match deleted_history_override(&stderr)
            .or_else(|| metadata_mismatch_override(&stderr))
            .or_else(|| setuptools_importlib_clash_override(&stderr))
        {
            Some(pin) if !overrides.contains(&pin) && overrides.len() < MAX_ERA_OVERRIDES => {
                tracing::warn!(
                    r#override = %pin,
                    tail = ?tail,
                    "date-pinned resolution hit an unresolvable era package — retrying with \
                     a per-package cutoff derived from uv's own error"
                );
                // The setuptools/importlib clash needs MORE than a lifted cutoff: the broken
                // importlib-metadata 0.x is ALREADY INSTALLED in the venv (2019 pluggy pulled
                // it in the requirements step), and an install won't touch an already-satisfied
                // package — so the cutoff pin alone retries into the exact same crash (live:
                // pytest-5413/5495 kickoff, 2026-08-12, the first run after this arm shipped).
                // Apply the banner's own remedy directly: upgrade the installed copy, then
                // retry. General to any `--no-build-isolation` build, not just `-e .`.
                if pin.starts_with("importlib-metadata=") {
                    let up = run(
                        uv,
                        &[
                            "pip",
                            "install",
                            "-q",
                            "--python",
                            py,
                            "--upgrade",
                            "importlib-metadata",
                        ],
                        None,
                    )
                    .await?;
                    if !up.status.success() {
                        // The heal itself failed — no point looping on the same wall.
                        return Ok(out);
                    }
                }
                overrides.push(pin);
            }
            _ => return Ok(out),
        }
    }
}

pub(super) fn which(bin: &str) -> Option<String> {
    crate::shell_portable::locate_executable(bin)
        .map(|path| path.to_string_lossy().into_owned())
}

/// The test files an instance's own `test_patch` touches — the scope to run.
pub fn patched_test_files(test_patch: &str) -> Vec<String> {
    let mut files: Vec<String> = test_patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Which harness executes an instance's tests — a property of the REPO, not a pytest
/// version to search for (#383). django (114 of Lite's 300 instances) does not use pytest
/// at all: its harness is `./tests/runtests.py <module directives>`, and running pytest
/// against it grades every django instance UNGRADEABLE. The official SWE-bench per-repo
/// spec tables are the reference semantics for each arm's command shape.
///
/// This is the seam the sympy-11400 refusal in `ensure_env` predicted: "the runner is a
/// property of the repo era, not a pytest version to search for."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRunner {
    /// The default: `python -m pytest <files> -v` and pytest's verbose report.
    Pytest,
    /// django's own runner: `python tests/runtests.py --verbosity 2 --settings=test_sqlite
    /// --parallel 1 <module directives>` from the repo root, unittest-style report.
    DjangoRuntests,
}

/// Select the runner from the instance's `repo` field. Unknown repos default to pytest —
/// the wrong default fails LOUDLY (the pristine gates in `grade` void the tree) rather
/// than silently, so a new repo class surfaces as a named gap, not a fake zero.
pub fn runner_for_repo(repo: &str) -> TestRunner {
    match repo {
        "django/django" => TestRunner::DjangoRuntests,
        _ => TestRunner::Pytest,
    }
}
