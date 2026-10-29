use super::*;

/// Pull every row of a HuggingFace dataset split, SHAPE-AGNOSTIC.
///
/// # Why this is split out (#370, 2026-08-19)
///
/// The catalog carries ~20 benchmark rows, each with a real `source_url`, and exactly ONE of
/// them could ever be fetched — because the only fetcher in the tree was fused to
/// `SweInstance`. Every other row was a name and a URL that nothing read: LiveCodeBench,
/// aider-polyglot, bigcodebench, evalplus, all declared, none pullable. That is the same
/// shape as the rest of this codebase's recurring defect — machinery written, never wired.
///
/// The paging and caching were never SWE-specific. Only the row DECODE was. So this returns
/// raw `Value` rows and each family maps them to its own type: ONE fetcher, N shape-mappers
/// ([[the-compression-principle]]). `load_dataset` below is now the SWE mapper over this, so
/// the generic path is the one already proven against SWE-bench Lite in production — a new
/// suite inherits a fetcher that works rather than getting a second, untested one.
///
/// Cached by (dataset, config, split) on first use; on-demand, never a gated install step.
pub async fn fetch_hf_rows(
    dataset: &str,
    config: &str,
    split: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let mut rows = Vec::new();
    stream_hf_rows(dataset, config, split, |row| {
        rows.push(row.clone());
        Ok(())
    })
    .await?;
    Ok(rows)
}

/// The datasets-server caps a page at 100. The ceiling is a real bound, not a guess: it stops a
/// mis-typed dataset name from paging forever against a server that answers 200 with an empty
/// list. A suite larger than this needs the bound RAISED deliberately, with its size named —
/// never silently truncated, which would publish a partial denominator as if it were the whole
/// suite.
// Raised 5_000 → 25_000 on 2026-08-23 for SWE-rebench, whose full `test`
// split is 21,336 rows (the real suite size, per the HF size endpoint) — the
// refusal below demands the bound be raised DELIBERATELY, naming the suite,
// rather than publishing a rate over a truncated list.
pub(super) const MAX_ROWS: usize = 25_000;

/// Where a suite's rows are cached. JSONL — one row per line — specifically so it can be read
/// back INCREMENTALLY. A single JSON array cannot be: `from_slice` must materialize the whole
/// thing, which is what made streaming impossible and staging an unbounded allocator.
pub(super) fn rows_cache_path(dataset: &str, config: &str, split: &str) -> std::path::PathBuf {
    let slug = format!("{}__{config}__{split}", dataset.replace('/', "__"));
    swe_cache_dir().join(format!("{slug}.rows.jsonl"))
}

/// The pre-JSONL cache written by every build before 2026-08-19. Read once and migrated rather
/// than discarded — throwing away a correct 500-row cache to change a file extension would make
/// the next fetch re-pull from the network for no reason.
pub(super) fn legacy_array_cache_path(dataset: &str, config: &str, split: &str) -> std::path::PathBuf {
    let slug = format!("{}__{config}__{split}", dataset.replace('/', "__"));
    swe_cache_dir().join(format!("{slug}.rows.json"))
}

/// THE pager. One row at a time, never accumulating — so peak memory is one page regardless of
/// suite size, and a caller that only needs a count or a projection never holds the dataset.
///
/// # Why this is the pager and `fetch_hf_rows` is a thin collector over it
///
/// The accumulate-then-return shape was the root of the OOM: every caller got the whole suite in
/// memory whether it needed it or not, and `benchmark/fetch` needed only a count. Inverting to a
/// callback makes the memory profile the CALLER's choice — collect if you need the rows, project
/// and drop if you do not — with one paging + caching + error path behind both
/// ([[the-compression-principle]]).
///
/// Returns the number of rows streamed. Three sources, in order:
/// 1. the JSONL cache, read line by line (no whole-file materialization);
/// 2. the legacy single-array cache, migrated to JSONL as it is read;
/// 3. the network, written to a TEMP JSONL page by page and atomically renamed on success —
///    so an interrupted fetch never leaves a short cache that a later run would trust as complete.
pub async fn stream_hf_rows<F>(
    dataset: &str,
    config: &str,
    split: &str,
    mut on_row: F,
) -> Result<usize, String>
where
    F: FnMut(&serde_json::Value) -> Result<(), String>,
{
    use std::io::{BufRead, Write};

    let cache = rows_cache_path(dataset, config, split);
    if let Ok(file) = std::fs::File::open(&cache) {
        let mut streamed = 0usize;
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|e| format!("reading cached rows for `{dataset}`: {e}"))?;
            if line.trim().is_empty() {
                continue;
            }
            let row: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| format!("corrupt cached row for `{dataset}`: {e}"))?;
            on_row(&row)?;
            streamed += 1;
        }
        if streamed > 0 {
            return Ok(streamed);
        }
        // An empty cache file is debris from an interrupted write, not an empty suite. Fall
        // through and re-fetch rather than reporting zero rows as a fact about the dataset.
    }

    // Legacy array cache: read once, hand rows through, and rewrite as JSONL so the next call
    // takes the streaming path. This one read is bounded by a file that already exists on disk.
    let legacy = legacy_array_cache_path(dataset, config, split);
    if let Ok(bytes) = std::fs::read(&legacy) {
        if let Ok(rows) = serde_json::from_slice::<Vec<serde_json::Value>>(&bytes) {
            if !rows.is_empty() {
                let _ = std::fs::create_dir_all(swe_cache_dir());
                if let Ok(mut out) = std::fs::File::create(&cache) {
                    for row in &rows {
                        let _ = writeln!(out, "{}", serde_json::to_string(row).unwrap_or_default());
                    }
                }
                let _ = std::fs::remove_file(&legacy);
                for row in &rows {
                    on_row(row)?;
                }
                return Ok(rows.len());
            }
        }
    }

    // Network. Written page-by-page to a temp file so the cache is BUILT without ever holding
    // the suite, and renamed only after the row count has passed both guards below.
    let _ = std::fs::create_dir_all(swe_cache_dir());
    let tmp = cache.with_extension("jsonl.partial");
    let mut out = std::fs::File::create(&tmp)
        .map_err(|e| format!("cannot open the staging cache for `{dataset}`: {e}"))?;
    let mut streamed = 0usize;
    for offset in (0..MAX_ROWS).step_by(100) {
        let url = format!(
            "https://datasets-server.huggingface.co/rows?dataset={}&config={}&split={}&offset={}&length=100",
            urlencoding_encode(dataset),
            urlencoding_encode(config),
            urlencoding_encode(split),
            offset
        );
        // A 21k-row suite is 200+ pages; the datasets-server rate-limits bursts
        // and answers 429 with an HTML body. Before this backoff, that surfaced
        // as "dataset decode failed at offset N: error decoding response body" —
        // a misdiagnosis that reads as data corruption (glass-boxed 2026-08-23
        // on SWE-rebench at offsets 0 and 3500). Honest handling: name the
        // rate-limit, back off exponentially, and resume the SAME page. Pages
        // are also paced unconditionally so a big pull stops tripping the
        // limiter in the first place.
        let mut backoff_s = 5u64;
        let body: serde_json::Value = loop {
            let resp = reqwest::get(&url)
                .await
                .map_err(|e| format!("dataset fetch failed at offset {offset}: {e}"))?;
            let status = resp.status();
            if status.as_u16() == 429 || status.is_server_error() {
                if backoff_s > 300 {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(format!(
                        "huggingface rate-limited `{dataset}` at offset {offset} and the                          backoff budget is spent — re-run the fetch later; the partial                          cache was discarded so a truncated suite can never be promoted"
                    ));
                }
                tracing::info!(
                    probe_class = "benchmark.fetch.rate_limited",
                    offset,
                    backoff_s,
                    "datasets-server {status} — backing off and resuming the same page"
                );
                tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                backoff_s *= 2;
                continue;
            }
            match resp.json().await {
                Ok(v) => break v,
                Err(e) => {
                    // Non-429 HTML/garbage still means "not the JSON API" — most often
                    // the limiter answering 200 with an interstitial. Same backoff,
                    // same honest label, never "decode failed" (which reads as corruption).
                    if backoff_s > 300 {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(format!(
                            "`{dataset}` at offset {offset}: response is not the rows API's                              JSON after retries ({e}) — likely rate-limiting; re-run later"
                        ));
                    }
                    tracing::info!(
                        probe_class = "benchmark.fetch.rate_limited",
                        offset,
                        backoff_s,
                        "non-JSON rows response — treating as rate-limit, backing off"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_s)).await;
                    backoff_s *= 2;
                }
            }
        };
        // Unconditional inter-page pacing: 200+ page pulls must not burst.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        // The server reports its own errors in-band with a 200. Surface it verbatim rather
        // than returning an empty set that a caller would read as "the suite is empty".
        if let Some(err) = body.get("error").and_then(|e| e.as_str()) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "huggingface refused `{dataset}` (config={config}, split={split}): {err}"
            ));
        }
        let Some(page) = body.get("rows").and_then(|r| r.as_array()) else {
            break;
        };
        if page.is_empty() {
            break;
        }
        for entry in page {
            if let Some(row) = entry.get("row") {
                // Write BEFORE handing to the caller: if the caller aborts, the bytes already
                // fetched are still on disk under the partial name, and the guards below decide
                // whether they are trustworthy enough to promote.
                let _ = writeln!(out, "{}", serde_json::to_string(row).unwrap_or_default());
                on_row(row)?;
                streamed += 1;
            }
        }
    }
    drop(out);

    if streamed == 0 {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "`{dataset}` (config={config}, split={split}) returned no rows — check the dataset \
             id and split name; an empty pull is never treated as an empty suite"
        ));
    }
    // The cap PROMISED it would never silently truncate, and until 2026-08-19 nothing enforced
    // that — a suite at or past the bound returned looking complete, which is precisely how a
    // partial denominator gets published as if it were the whole suite. Refuse, and do NOT
    // promote the partial file: a truncated cache that survives is a wrong denominator that
    // reproduces on every later run.
    if streamed >= MAX_ROWS {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "`{dataset}` (config={config}, split={split}) hit the {MAX_ROWS}-row fetch bound. \
             The suite may be larger than what was pulled, so this row count is NOT a \
             trustworthy denominator. Raise MAX_ROWS in swe_bench.rs deliberately, naming the \
             real suite size, rather than publishing a rate over a truncated list."
        ));
    }
    // Atomic promote — a reader either sees the whole cache or no cache, never a short one.
    std::fs::rename(&tmp, &cache)
        .map_err(|e| format!("cannot promote the staging cache for `{dataset}`: {e}"))?;
    Ok(streamed)
}

/// The SWE shape-mapper over [`fetch_hf_rows`]. Cached on first use.
pub async fn load_dataset(dataset: &str) -> Result<Vec<SweInstance>, String> {
    let cache = swe_cache_dir().join(format!("{}.json", dataset.replace('/', "__")));
    if let Ok(bytes) = std::fs::read(&cache) {
        if let Ok(rows) = serde_json::from_slice::<Vec<SweInstance>>(&bytes) {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
    }
    let raw = fetch_hf_rows(dataset, "default", "test").await?;
    let total = raw.len();
    let rows: Vec<SweInstance> = raw
        .into_iter()
        .filter_map(|row| serde_json::from_value::<SweInstance>(row).ok())
        .collect();
    if rows.is_empty() {
        return Err(format!(
            "{dataset} fetched {total} rows but NONE decoded as a SWE instance — this dataset \
             is not SWE-instance-shaped, so it needs its own mapper rather than this one"
        ));
    }
    let _ = std::fs::create_dir_all(swe_cache_dir());
    let _ = std::fs::write(&cache, serde_json::to_vec(&rows).unwrap_or_default());
    Ok(rows)
}

/// Minimal percent-encoding for the one query param we send (the dataset name's `/`).
pub(super) fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

/// Ceiling on any single grader subprocess. Nothing legitimate here — a git
/// mirror fetch, a uv venv build, one instance's full pytest files — takes
/// this long; the class this kills is INFINITE (task #381: sympy
/// symbolic-computation hangs orphaned to launchd at 100% CPU for ELEVEN
/// HOURS, detected by the operator's cooling fan, not by any instrument).
pub(super) const SUBPROCESS_CEILING: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub(crate) async fn run(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
) -> Result<std::process::Output, String> {
    run_env(program, args, cwd, &[]).await
}

/// `run` with extra environment variables — the seam era C-extension builds need
/// (see `ERA_CFLAGS`). Everything else (ownership, ceiling, how it ends) is
/// identical; `run` delegates here so there is exactly one subprocess path.
pub(crate) async fn run_env(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    // #381: pytest spawns grandchildren (sympy's own runner, plugins) that survived a
    // parent-only kill and orphaned to launchd. The run is OWNED (continuum_process::run_owned):
    // past the ceiling, or when this future is dropped (a core reboot cancels it mid-await),
    // the child and everything it started are ended deepest first, and what isn't confirmed
    // gone is named. It stays in the core's process group, so a stop ends it too.
    let out = continuum_process::run_owned(cmd, Some(SUBPROCESS_CEILING), "grader", std::sync::Arc::default())
        .await
        .map_err(|e| format!("could not run `{program}`: {e}"))?;
    if let (continuum_process::Ended::Exited(_), Some(note)) = (&out.ended, out.capture_note()) {
        // a test run whose output isn't whole can't be graded on it: an environment fault, never a verdict
        return Err(format!("`{program}` exited but {note} — an environment fault, never a verdict"));
    }
    match out.ended {
        continuum_process::Ended::Exited(status) => Ok(std::process::Output { status, stdout: out.stdout.bytes, stderr: out.stderr.bytes }),
        continuum_process::Ended::TimedOut { unconfirmed } => {
            continuum_base::probe!(
                class = "benchmark.subprocess.ceiling",
                program = %program,
                ceiling_s = SUBPROCESS_CEILING.as_secs(),
                unconfirmed = ?unconfirmed,
                "grader subprocess exceeded the ceiling — it and everything it started were ended \
                 (a hung test run is an environment fault, never a verdict)"
            );
            Err(format!(
                "`{program}` exceeded the {}s subprocess ceiling and was ended with everything it \
                 started{} — a hung run is an environment fault, never a verdict (task #381)",
                SUBPROCESS_CEILING.as_secs(),
                if unconfirmed.is_empty() { String::new() } else { format!(" (not confirmed gone: {unconfirmed:?})") }
            ))
        }
    }
}

/// One live `clone_at` per DESTINATION. Keyed by `repo_dir`, not instance — the same
/// instance staged at two different roots is two independent jobs, while two callers
/// aiming at ONE path are a razed tree waiting to happen (see the guard below).
pub(super) static CLONE_AT_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<
        std::collections::HashMap<std::path::PathBuf, std::sync::Arc<tokio::sync::Mutex<()>>>,
    >,
> = std::sync::LazyLock::new(Default::default);

pub(super) fn clone_lock_for(repo_dir: &Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    CLONE_AT_LOCKS
        .lock()
        .expect("clone-lock registry poisoned")
        .entry(repo_dir.to_path_buf())
        .or_default()
        .clone()
}

/// The staging tree a fresh clone is built in before the rename commit-point.
///
/// Unique per INVOCATION, not per process: `cloning-{pid}` was a Node-era assumption
/// (every clone its own process). In the headless core every clone shares ONE pid, so
/// two concurrent stages of the same instance computed the same path and the
/// stale-staging sweep deleted a LIVE clone mid-fetch — the 2026-08-18 grade loss
/// (`repo.cloning-78289/.git/: No such file or directory`, sympy-18057's finished
/// artifact voided at scoring). The per-destination lock in [`clone_at`] already
/// serializes callers; the nonce keeps even a future unlocked path collision-free and
/// keeps a crashed run's leftovers from ever matching a live invocation's name.
/// Whether a directory name is an in-flight staging tree minted by [`staging_path_for`]
/// (`<instance>.cloning-<pid>-<nonce>`): a clone before its rename commit-point, never a
/// usable checkout even when it already carries a `.git`. The one place that knows the
/// shape, beside the one place that makes it.
pub fn is_in_flight_staging_name(name: &str) -> bool {
    name.contains(".cloning-")
}

pub(super) fn staging_path_for(repo_dir: &Path) -> PathBuf {
    static CLONE_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    repo_dir.with_extension(format!(
        "cloning-{}-{}",
        std::process::id(),
        CLONE_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

/// Clone the repo at `base_commit`. The commit is the whole point — a clone left at HEAD is
/// how eight runs got scored against a tree with the fix already in it.
pub(super) fn refuse_unrecovered_checkouts(repo_dir: &Path) -> Result<(), String> {
    let (Some(parent), Some(name)) = (repo_dir.parent(), repo_dir.file_name()) else {
        return Ok(());
    };
    let entries = match std::fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("cannot inspect interrupted staging at {}: {e}", parent.display())),
    };
    let prefix = format!("{}.cloning-", name.to_string_lossy());
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot inspect interrupted staging: {e}"))?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let path = entry.path();
            if path.join(".git").try_exists().map_err(|e| format!("cannot inspect {}: {e}", path.display()))? {
                return Err(format!(
                    "interrupted staging checkout {} requires recovery before restaging {}; preserved because it may contain citizen work",
                    path.display(), repo_dir.display()
                ));
            }
        }
    }
    Ok(())
}

pub async fn clone_at(instance: &SweInstance, repo_dir: &Path) -> Result<(), String> {
    // SERIALIZE PER DESTINATION (2026-08-21, from the 2026-08-18 grade loss). This
    // function both REMOVES `repo_dir` at entry and razes/renames it at the commit
    // point, and the protocol legitimately wants the same instance's tree more than
    // once (her work tree + the pristine score tree, a lapsed-claim auto-grade racing
    // a re-stage). Concurrent callers therefore delete each other's trees mid-use —
    // measured: sympy-18057's FINISHED artifact died at grading with "clone from its
    // local mirror failed: repo.cloning-78289/.git/: No such file or directory", which
    // is caller B's stale-staging sweep removing caller A's clone WHILE git wrote into
    // it (same pid → same staging name, see below). The tree is one resource; its
    // builder is a critical section. tokio::Mutex (never a sync lock) because the
    // section awaits on git subprocesses throughout.
    let lock = clone_lock_for(repo_dir);
    let _staged = lock.lock().await;
    // A temporary name does not establish disposability. Citizens have reached
    // interrupted clones and edited them (Sahar, 2026-09-29). Refuse BEFORE any
    // destination removal; recovery must preserve those checkouts explicitly.
    refuse_unrecovered_checkouts(repo_dir)?;
    // A stale tree here is not "probably fine" — it is the tree the score comes from. Removal
    // failing used to be SWALLOWED (`let _ =`), and the clone below then died on git's own
    // "destination path already exists and is not an empty directory", which reads like a
    // harness bug rather than a filesystem one. Say which it is.
    if repo_dir.exists() {
        std::fs::remove_dir_all(repo_dir).map_err(|e| {
            format!("could not clear the stale grade tree {}: {e}", repo_dir.display())
        })?;
    }
    // The PARENT must exist first. `git clone` creates its target directory but not the chain
    // above it, and the failure surfaces late and cryptically — as a mid-fetch "unable to write
    // .git/objects/pack/*.pack: No such file or directory", which reads like a disk or network
    // fault rather than a missing mkdir.
    if let Some(parent) = repo_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    // ONE network fetch per REPO, not per clone.
    //
    // The protocol needs two trees per instance — the one she works in and a pristine one to
    // score in — and a sweep re-grades the same repo across many instances. Cloning from
    // GitHub each time was ~240 MB of network AND disk per tree: a 300-instance Lite sweep
    // would have been ~140 GB and hours of fetch. A local mirror makes every tree after the
    // first nearly free and costs one fetch per repo for the whole sweep.
    //
    // Each tree is a plain local clone of the mirror: git HARD-LINKS the mirror's objects
    // (same filesystem), so the tree costs about what a `--shared` borrow did and never
    // depends on the mirror surviving. These trees are not disposable: they become a
    // resident's working checkout, with her uncommitted work in it. Borrowed through
    // `--shared`, every one of them broke when the mirrors were wiped (the M5, 2026-10-24: 243
    // of 262 checkouts, `unable to normalize alternate object path`, her work reading as no
    // candidate 238 times).
    let mirror = swe_cache_dir()
        .join("mirrors")
        .join(instance.repo.replace('/', "__"));
    let url = format!("https://github.com/{}.git", instance.repo);
    if !mirror.exists() {
        if let Some(parent) = mirror.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = run(
            "git",
            &[
                "clone",
                "--quiet",
                "--bare",
                &url,
                &mirror.to_string_lossy(),
            ],
            None,
        )
        .await?;
        if !out.status.success() {
            return Err(format!(
                "mirror clone of {} failed: {}",
                instance.repo,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    // Clone into a STAGING path and move it into place, never straight into `repo_dir`.
    //
    // `git clone` refuses any destination that is not empty, and on macOS `repo_dir` does not
    // stay empty on its own: Finder / Spotlight drop a `.DS_Store` into a directory the instant
    // they notice it. Measured 2026-08-17 — a re-grade of astropy-14995 failed with
    // "destination path already exists and is not an empty directory" against a directory whose
    // ONLY content was a `.DS_Store` stamped AFTER the remove above. The window between
    // remove and clone was enough. A grade voided by an OS indexer is indistinguishable, from
    // the receipt, from a grade voided by the harness — so close the window instead of
    // widening the diagnosis. Staging + rename also means a half-fetched tree is never
    // visible at `repo_dir`: the move is the commit point.
    let staging = staging_path_for(repo_dir);
    // Sweep incomplete staging remnants only. Git checkouts were refused above;
    // their temporary names alone cannot prove that no citizen owns work there.
    // Under the per-destination lock nothing live matches, and a crashed run's leftovers
    // (whose nonce-bearing names can never collide with ours) must not accumulate as
    // unswept disk litter (the 2026-07-13 rule: no cache dir without an eviction story).
    if let (Some(parent), Some(name)) = (repo_dir.parent(), repo_dir.file_name()) {
        let dead_prefix = format!("{}.cloning-", name.to_string_lossy());
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(&dead_prefix)
                    && !entry.path().join(".git").exists()
                {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }
    let out = run(
        "git",
        &[
            "clone",
            "--quiet",
            "--local",
            &mirror.to_string_lossy(),
            &staging.to_string_lossy(),
        ],
        None,
    )
    .await?;
    if !out.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "clone of {} from its local mirror failed: {}",
            instance.repo,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // Anything that reappeared at the destination while we fetched (see above) loses to the
    // tree we just built.
    if repo_dir.exists() {
        std::fs::remove_dir_all(repo_dir).map_err(|e| {
            format!("could not clear {} before staging the fresh clone: {e}", repo_dir.display())
        })?;
    }
    std::fs::rename(&staging, repo_dir).map_err(|e| {
        format!(
            "could not move the fresh clone {} into place at {}: {e}",
            staging.display(),
            repo_dir.display()
        )
    })?;
    let out = run(
        "git",
        &["checkout", "--quiet", &instance.base_commit],
        Some(repo_dir),
    )
    .await?;
    if !out.status.success() {
        // A mirror created earlier can predate this instance's base_commit. Refresh it once and
        // retry rather than failing — the alternative is a cache that silently rots into
        // "instance not gradeable" as the dataset grows.
        let _ = run("git", &["fetch", "--quiet", "--all"], Some(&mirror)).await;
        let _ = run("git", &["fetch", "--quiet", "origin"], Some(repo_dir)).await;
        let retry = run(
            "git",
            &["checkout", "--quiet", &instance.base_commit],
            Some(repo_dir),
        )
        .await?;
        if !retry.status.success() {
            return Err(format!(
                "checkout {} failed even after refreshing the local mirror: {}",
                &instance.base_commit[..12.min(instance.base_commit.len())],
                String::from_utf8_lossy(&retry.stderr).trim()
            ));
        }
    }
    shield_workspace_excludes(repo_dir);
    era_checkout_fixups(instance, repo_dir);
    // Record the staged base so "work" can be measured against it later — a committed fix on
    // a clean tree is work (staged_workspace::work_mtime_of). Best effort: an unwritable
    // record falls back to the reflog read.
    let _ = std::fs::write(repo_dir.join(".git").join("continuum-base"), &instance.base_commit);
    Ok(())
}

/// Era HARNESS accommodations applied to EVERY checkout of an instance —
/// solve staging and grade clone alike, through this one shared seam, so the
/// tree she works and the tree that scores her are identically accommodated
/// (the same class of change the official harness's environment-setup commits
/// make). NEVER touches anything a task's tests assert on.
///
/// scikit-learn < 0.22 (proven end-to-end on Apple silicon 2026-08-27,
/// SKLEARN 0.21.dev0 imports clean): the vendored cloudpickle's
/// `_make_cell_set_template_code` calls `types.CodeType(...)` with the
/// pre-3.8 signature and cannot IMPORT on any interpreter this platform has
/// (no 3.7 exists for arm64 macOS). The canonical upstream cloudpickle fix —
/// build the template via `co.replace(...)` on 3.8+ — is injected ahead of
/// the old constructor. Vendored build/serialization plumbing, not subject
/// logic: no SWE task asserts on cloudpickle internals.
pub(super) fn era_checkout_fixups(instance: &SweInstance, repo_dir: &Path) {
    if instance.repo != "scikit-learn/scikit-learn" {
        return;
    }
    let cp = repo_dir.join("sklearn/externals/joblib/externals/cloudpickle/cloudpickle.py");
    let Ok(src) = std::fs::read_to_string(&cp) else {
        return; // newer sklearn: nothing vendored here, nothing to accommodate
    };
    if src.contains("continuum era harness") {
        return; // already applied (idempotent across re-stages)
    }
    let anchor = "def _make_cell_set_template_code():";
    if !src.contains(anchor) {
        return;
    }
    let replacement = "def _make_cell_set_template_code():
    import sys as _sys
    if _sys.version_info >= (3, 8):
        # py3.8+ compat (canonical upstream cloudpickle fix), injected by the
        # continuum era harness at checkout staging — applied uniformly to
        # solve and grade trees.
        def _cell_set_factory(value):
            lambda: cell
            cell = value
        co = _cell_set_factory.__code__
        return co.replace(co_argcount=0, co_varnames=('cell',), co_name='cell_set_template', co_freevars=('cell',), co_cellvars=())
";
    let patched = src.replacen(anchor, replacement, 1);
    if std::fs::write(&cp, patched).is_ok() {
        continuum_base::probe!(
            class = "benchmark.env.checkout_fixup",
            instance = %instance.instance_id,
            fixup = "cloudpickle-py38-codetype",
            "era harness accommodation applied at the shared clone seam"
        );
    }
}

/// SUBSTRATE ARTIFACTS NEVER ENTER HER PATCH (2026-08-22, seen live: an airc
/// scope auto-minted itself INSIDE a staged checkout — `$PWD/.airc` resolution
/// while her hands had cwd in the repo — and its sqlite/identity-key binaries
/// landed in her diff, which is her graded artifact). `.git/info/exclude` is the
/// right container: it suppresses the noise from every diff WITHOUT touching a
/// tracked file (writing .gitignore would itself pollute the patch). The scope-
/// minting itself is a separate airc-side follow-up; this makes every staged
/// tree immune regardless of who writes scratch into it.
///
/// Idempotent MERGE, never an overwrite: called on fresh clones (`clone_at`) AND
/// on already-staged checkouts every dispatch (the self-healing that replaced a
/// one-off 76-checkout hand backfill, 2026-08-22) — an overwrite would clobber
/// an operator's own exclude entries on the second call.
pub fn shield_workspace_excludes(repo_dir: &Path) {
    const SHIELDED: [&str; 2] = [".airc/", ".DS_Store"];
    let exclude = repo_dir.join(".git/info/exclude");
    if let Some(dir) = exclude.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let current = std::fs::read_to_string(&exclude).unwrap_or_default(); // unreadable/absent both mean "has no entries yet" — the write below is the fix either way
    let missing: Vec<&str> = SHIELDED
        .iter()
        .copied()
        .filter(|line| !current.lines().any(|l| l.trim() == *line))
        .collect();
    if missing.is_empty() {
        return;
    }
    let mut merged = current;
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    for line in missing {
        merged.push_str(line);
        merged.push('\n');
    }
    if let Err(e) = std::fs::write(&exclude, merged) {
        // Best-effort: a missing exclude degrades to the old (noisy) diff, never a
        // failed stage — but say so.
        tracing::warn!(error = %e, repo = %repo_dir.display(), "could not write .git/info/exclude — substrate artifacts may pollute the graded diff");
    }
}
