use super::*;

/// Fold one run's on-disk ledgers into a card. Pure over the two JSON values +
/// mtime so the derivation is unit-testable without a filesystem.
pub(super) fn fold_run_card(
    run_id: &str,
    result: Option<&serde_json::Value>,
    grade: Option<&serde_json::Value>,
    last_activity_ms: u64,
    now_ms: u64,
) -> BenchRunCard {
    let s = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_str())
            .map(String::from)
    };
    let n = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_u64())
            .map(|x| x as u32)
    };
    let arr = |v: Option<&serde_json::Value>, k: &str| -> Vec<String> {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    // ABSENCE vs ZERO, on the board. A grade carrying `error` is a harness/env
    // fault the grader PROVED — for the env class it re-runs the PRISTINE tree
    // first, so a genuinely broken patch is never mislabelled — and
    // `SweGradeResult.error` documents the contract in its own doc comment: "a
    // result with `error` is an ABSENCE, not a zero, and must never be tallied
    // as a failed attempt." This projection honoured that for the RESULT's error
    // and ignored the GRADE's, so an env fault folded as `resolved: false` +
    // phase `failed` — indistinguishable from a citizen who tried and lost.
    // Measured 2026-08-13: 8 of 36 instances (22%) grade UNGRADEABLE on this box.
    // `infra_error` already means "no valid verdict, and why", so it takes both
    // sources rather than growing a second field, and `resolved` returns to None
    // — the same "no verdict" the pre-grade card carries, because that is the truth.
    let grade_error = s(grade, "error");
    let ungradeable = grade_error.is_some();
    let resolved = if ungradeable {
        None
    } else {
        grade
            .and_then(|g| g.get("resolved"))
            .and_then(|x| x.as_bool())
    };
    let infra_error = s(result, "infra_error")
        .or_else(|| s(result, "error"))
        .or(grade_error);
    let failed_marker = result
        .and_then(|r| r.get("failed"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let age_secs = now_ms.saturating_sub(last_activity_ms) / 1000;
    let phase = if resolved == Some(true) {
        "resolved"
    } else if ungradeable {
        // Ahead of `failed`: a run can carry both a failed marker and an
        // ungradeable grade, and the absence is the more truthful of the two.
        "ungradeable"
    } else if failed_marker {
        "failed"
    } else if age_secs < RUN_STALL_WINDOW_SECS {
        "active"
    } else {
        "quiet"
    };
    let ratio = |g: Option<&serde_json::Value>, passed: &str, total: &str| match (
        n(g, passed),
        n(g, total),
    ) {
        (Some(p), Some(t)) => Some(format!("{p}/{t}")),
        _ => None,
    };
    BenchRunCard {
        run_id: run_id.to_string(),
        instance: s(result, "instance"),
        attempt: n(result, "attempt"),
        max_attempts: n(result, "max_attempts"),
        solver: s(result, "persona_id"),
        stalled: phase == "quiet",
        phase: phase.to_string(),
        last_activity_ms,
        age_secs,
        acts: n(result, "acts"),
        files_changed: arr(result, "files_changed"),
        files_examined: arr(result, "files_examined"),
        resolved,
        fail_to_pass: ratio(grade, "failToPassPassed", "failToPassTotal"),
        pass_to_pass: ratio(grade, "passToPassPassed", "passToPassTotal"),
        // Graded diff size when a grade exists; before any grade, the RESULT's
        // own patch length — the live "a patch is forming" leading indicator
        // (#329). One field, grade-authoritative, never both shown.
        patch_bytes: n(grade, "patchBytes").or_else(|| {
            result
                .and_then(|r| r.get("patch"))
                .and_then(|p| p.as_str())
                .map(|p| p.len() as u32)
        }),
        failed_tests: arr(grade, "failedTests"),
        infra_error,
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardParams.ts")]
pub struct BenchmarkScoreboardParams {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreRow.ts")]
pub struct BenchmarkScoreRow {
    /// Catalog benchmark this tallies (e.g. `swe-bench-verified`).
    pub benchmark: String,
    /// Distinct instances with a REAL verdict (error-free; absences never tally).
    #[ts(type = "number")]
    pub attempted: u32,
    /// Of those, resolved (all fail-to-pass passed, gate held).
    #[ts(type = "number")]
    pub resolved: u32,
    /// The dataset's full size — the leaderboard denominator this samples.
    #[ts(type = "number")]
    pub dataset_size: u32,
    /// Resolved instance ids — the receipt pointers.
    pub resolved_instances: Vec<String>,
    /// The harness build(s) that produced this row's verdicts, newest-first.
    ///
    /// MORE THAN ONE MEANS THE ROW BLENDS ERAS — and a rate averaged over a
    /// moving instrument is not a measurement. Measured 2026-08-28: 19 of 32
    /// verdicts here had been scored across ten days by three harness builds,
    /// and regrading one from its IDENTICAL banked patch moved pass-to-pass
    /// from 0/40 to 40/40. Publishing an improvement curve over that is
    /// publishing the harness's changes as if they were hers. `<unstamped>`
    /// marks verdicts written before provenance existed.
    #[serde(default)]
    pub harness_builds: Vec<String>,
    /// ENV failures (verdict carries `error`): the harness could not measure
    /// the model at all — clone/env/patch infrastructure, NEVER a model miss.
    /// These are absences owing retakes, and they must read that way.
    #[ts(type = "number")]
    pub env_absences: u32,
    /// The env-absent instance ids, so the failure is chaseable per instance.
    pub env_absent_instances: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardResult.ts")]
pub struct BenchmarkScoreboardResult {
    pub rows: Vec<BenchmarkScoreRow>,
    /// The REGIME every published number must carry: model, window, build, host.
    pub regime: String,
    pub summary: String,
}

#[derive(Default)]
pub struct BenchmarkScoreboard;

#[async_trait::async_trait]
impl ActionCommand for BenchmarkScoreboard {
    const NAME: &'static str = "benchmark/scoreboard";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The SCORE rollup: per SWE-class benchmark, attempted vs resolved from the durable \
         verdicts on disk, with the serving REGIME (model, window, build sha, host) every \
         published claim must carry. ONE read for the operator, the README chart, and a \
         citizen grounding on how the team is scoring — instead of tallying verdict files \
         by hand.";
    type Params = BenchmarkScoreboardParams;
    type Output = BenchmarkScoreboardResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkScoreboardParams,
    ) -> Result<BenchmarkScoreboardResult, CommandError> {
        let verdicts = swe_bench::recorded_verdicts();
        let mut rows = Vec::new();
        for spec in known_benchmarks() {
            let Some(dataset) = spec.swe_dataset() else {
                continue;
            };
            // Membership by the SAME loader the grade path searches with — one
            // source of truth for "which dataset does this instance belong to".
            let Ok(instances) = crate::cognition::swe_bench::load_dataset(dataset).await else {
                continue; // not fetched yet — an un-run benchmark, not an error
            };
            let ids: std::collections::HashSet<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            let mut attempted = 0u32;
            let mut resolved_instances = Vec::new();
            let mut env_absent_instances = Vec::new();
            // Which harness build(s) actually produced the verdicts behind this
            // rate. Collected from the TALLIED verdicts only — the absences are
            // not part of the claim.
            let mut builds: std::collections::BTreeSet<String> = Default::default();
            for (id, v) in &verdicts {
                if !ids.contains(id.as_str()) {
                    continue;
                }
                if v.error.is_some() || !v.gate_ok {
                    // Absence, never a tallied attempt — but never invisible
                    // either: an env failure the user can't see reads as a
                    // model miss in every retelling. A failed GATE is the same
                    // class: the control was broken, nothing was measured
                    // (belt-and-suspenders — record_verdict already refuses
                    // gate-failed verdicts, so this arm should never fire).
                    env_absent_instances.push(id.clone());
                    continue;
                }
                attempted += 1;
                builds.insert(if v.harness_build.is_empty() {
                    "<unstamped>".to_string()
                } else {
                    v.harness_build.clone()
                });
                if v.resolved {
                    resolved_instances.push(id.clone());
                }
            }
            rows.push(BenchmarkScoreRow {
                benchmark: spec.name.to_string(),
                attempted,
                resolved: resolved_instances.len() as u32,
                dataset_size: instances.len() as u32,
                resolved_instances,
                harness_builds: {
                    let mut b: Vec<String> = builds.into_iter().collect();
                    b.sort();
                    b
                },
                env_absences: env_absent_instances.len() as u32,
                env_absent_instances,
            });
        }
        let serving = crate::inference::llama_server::current_serving();
        let regime = format!(
            "model={} served_window={} build={} sha={} host={}-{}",
            serving.active_model.as_deref().unwrap_or("none-serving"), // absence stated, never a fake model id
            serving.served_context_window,
            env!("CONTINUUM_BUILD_NUMBER"),
            env!("CONTINUUM_BUILD_GIT_SHA"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        let summary = rows
            .iter()
            .filter(|r| r.attempted > 0 || r.env_absences > 0)
            .map(|r| {
                let env = if r.env_absences > 0 {
                    format!(
                        " · {} ENV failure(s) — not model misses, they owe retakes: {}",
                        r.env_absences,
                        r.env_absent_instances.join(", ")
                    )
                } else {
                    String::new()
                };
                // SAY IT WHEN THE RATE BLENDS ERAS. A number averaged over a
                // moving instrument is not a measurement, and this string is
                // what gets quoted into charts and READMEs — the exact place a
                // silent blend becomes a published claim.
                let mixed = if r.harness_builds.len() > 1 {
                    format!(
                        " · ⚠ MIXED HARNESS ERAS ({}) — regrade to one build before \
                         publishing this as a curve",
                        r.harness_builds.join(", ")
                    )
                } else {
                    String::new()
                };
                format!(
                    "{}: {}/{} resolved (of {} in the set){env}{mixed}",
                    r.benchmark, r.resolved, r.attempted, r.dataset_size
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let summary = if summary.is_empty() {
            "no real verdicts on disk yet — dispatch a round first".to_string()
        } else {
            summary
        };
        Ok(BenchmarkScoreboardResult {
            rows,
            regime,
            summary,
        })
    }
}

crate::register_stateless_command!(BenchmarkScoreboard);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateParams.ts")]
pub struct BenchmarkValidateParams {
    /// SWE-class benchmark to validate (default swe-bench-verified).
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    /// Cap on env classes to build (default all). Each class = one real
    /// checkout + env build for its representative instance.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateClass.ts")]
pub struct BenchmarkValidateClass {
    /// The env class: repo + era year.
    pub repo: String,
    pub era: String,
    /// The representative instance actually built.
    pub representative: String,
    /// Instances in the dataset this class covers.
    #[ts(type = "number")]
    pub covers: u32,
    /// Did checkout + env build + (for pytest repos) the trivial-test smoke pass?
    pub green: bool,
    /// The named wall when red — actionable, never a mystery.
    #[ts(optional)]
    pub wall: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkPlatformFingerprint.ts")]
pub struct BenchmarkPlatformFingerprint {
    /// The machine CLASS a coverage claim is keyed by (e.g. `m-series-macos`,
    /// `x86_64-linux`, `windows`) — the coarse key an alloy consumer matches.
    pub machine_class: String,
    pub os: String,
    pub arch: String,
    /// The load-bearing toolchain versions — the BITTEN-BY list, grown only
    /// when a new wall names a new dependency (2026-08-27 initial set: clang
    /// broke on `-march=native`, libomp/freetype were the matplotlib/sklearn
    /// walls, uv's interpreter shelf decided the py3.7 structural question).
    pub clang: String,
    pub libomp: bool,
    pub freetype: bool,
    pub uv: String,
    /// Interpreter majors uv can actually provide on this platform.
    pub pythons: Vec<String>,
}

impl BenchmarkPlatformFingerprint {
    pub(super) fn capture() -> Self {
        let run = |cmd: &str, args: &[&str]| -> String {
            std::process::Command::new(cmd)
                .args(args)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "absent".into())
        };
        let machine_class = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "m-series-macos",
            ("macos", _) => "intel-macos",
            ("linux", a) if a == "aarch64" => "arm-linux",
            ("linux", _) => "x86_64-linux",
            ("windows", _) => "windows",
            _ => "other",
        }
        .to_string();
        let pythons = std::process::Command::new("uv")
            .args(["python", "list", "--only-installed"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.split_whitespace().next().map(str::to_string))
                    .take(8)
                    .collect()
            })
            .unwrap_or_default();  // unwrap_or: unreadable = empty, the report shows the tracker's view
        Self {
            machine_class,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            clang: run("clang", &["--version"]),
            libomp: std::path::Path::new("/opt/homebrew/opt/libomp/include/omp.h").exists()
                || std::path::Path::new("/usr/lib/libomp.so").exists(),
            freetype: std::process::Command::new("pkg-config")
                .args(["--exists", "freetype2"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            uv: run("uv", &["--version"]),
            pythons,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateResult.ts")]
pub struct BenchmarkValidateResult {
    /// The platform this coverage map is TRUE FOR — coverage claims are always
    /// per-machine-class; an alloy consumer matches this before trusting them.
    pub platform: BenchmarkPlatformFingerprint,
    pub classes: Vec<BenchmarkValidateClass>,
    /// Instances covered by GREEN classes / dataset size — THE coverage number.
    #[ts(type = "number")]
    pub instances_green: u32,
    #[ts(type = "number")]
    pub dataset_size: u32,
    pub summary: String,
}

/// `benchmark/validate` — the harness proves ITSELF before anyone trusts a
/// round with it. One representative per (repo, era-year) class runs the SAME
/// checkout + env-build seams every real solve and grade use; the result is
/// the env-coverage map ("N of 500 instances sit in classes proven green on
/// this box") with every red class carrying its named wall. Run it before a
/// published round; cite it in the regime. No excuses, surprised-ourselves-first.
#[derive(Default)]
pub struct BenchmarkValidate;

#[async_trait::async_trait]
impl ActionCommand for BenchmarkValidate {
    const NAME: &'static str = "benchmark/validate";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Validate the benchmark harness against a dataset: build one representative env per \
         (repo, era) class through the real checkout/env seams and report the coverage map — \
         which instances sit in proven-green classes, and the named wall for every red one.";
    type Params = BenchmarkValidateParams;
    type Output = BenchmarkValidateResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkValidateParams,
    ) -> Result<BenchmarkValidateResult, CommandError> {
        let name = p.name.as_deref().unwrap_or("swe-bench-verified");
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| CommandError::Invalid(format!("unknown benchmark '{name}'")))?;
        let dataset = spec.swe_dataset().ok_or_else(|| {
            CommandError::Invalid(format!("'{name}' is not an SWE-class benchmark"))
        })?;
        let instances = crate::cognition::swe_bench::load_dataset(dataset)
            .await
            .map_err(CommandError::Internal)?;
        let dataset_size = instances.len() as u32;
        // Class = (repo, created_at year): the era proxy the env machinery keys
        // dependency resolution on. First instance per class represents it.
        let mut classes: std::collections::BTreeMap<(String, String), (String, u32)> =
            Default::default();
        for i in &instances {
            let era = i.created_at.get(0..4).unwrap_or("????").to_string(); // dataset rows carry ISO dates; a malformed one groups under ???? visibly
            let e = classes
                .entry((i.repo.clone(), era))
                .or_insert_with(|| (i.instance_id.clone(), 0));
            e.1 += 1;
        }
        let cap = p.limit.unwrap_or(u32::MAX) as usize; // default: every class — a validation that samples silently is not a validation
        let mut rows = Vec::new();
        let mut instances_green = 0u32;
        for ((repo, era), (rep, covers)) in classes.into_iter().take(cap) {
            let inst = instances
                .iter()
                .find(|i| i.instance_id == rep)
                .expect("representative came from this same list"); // same vec, same loop — cannot miss
            let outcome = async {
                let dir = crate::cognition::swe_bench::ensure_grade_checkout(inst)
                    .await
                    .map_err(|e| format!("checkout: {e}"))?;
                crate::cognition::swe_bench::ensure_env(inst, &dir)
                    .await
                    .map_err(|e| format!("env: {e}"))?;
                Ok::<(), String>(())
            }
            .await;
            let green = outcome.is_ok();
            if green {
                instances_green += covers;
            }
            let wall = outcome.err().map(|e| {
                let t: String = e.chars().take(500).collect();
                t
            });
            crate::probe!(
                class = "benchmark.validate.class",
                repo = %repo,
                era = %era,
                green,
                covers = covers as u64,
                "env class validated through the real seams"
            );
            rows.push(BenchmarkValidateClass {
                repo,
                era,
                representative: rep,
                covers,
                green,
                wall,
            });
        }
        let summary = format!(
            "{instances_green}/{dataset_size} instances sit in proven-green env classes \
             ({} classes green, {} red)",
            rows.iter().filter(|r| r.green).count(),
            rows.iter().filter(|r| !r.green).count()
        );
        let result = BenchmarkValidateResult {
            platform: BenchmarkPlatformFingerprint::capture(),
            classes: rows,
            instances_green,
            dataset_size,
            summary,
        };
        // PERSIST, so the map can GATE. A coverage map that only ever exists in
        // one command's stdout cannot protect anything: dispatch had no way to
        // ask "is this instance's class known-red?", so a citizen could be sent
        // to spend hours inside an env class this box had already PROVEN cannot
        // build (measured 2026-08-28: astropy-6938 dispatched into the numpy-2
        // wall while a validate run sitting minutes away already knew that class
        // was red). Written per (dataset × machine_class) because a coverage
        // claim is only true for the platform that earned it.
        if let Err(e) = write_coverage_map(name, &result) {
            crate::probe!(
                class = "benchmark.validate.map_unwritten",
                error = %e,
                "coverage map could not be persisted — dispatch keeps its \
                 fail-open behaviour and gates nothing"
            );
        }
        Ok(result)
    }
}

/// Where a validated coverage map lives for one (dataset, machine-class) pair.
/// Under the governed benchmarks root, next to the verdicts it protects — one
/// small file per dataset, rewritten in place, so it needs no eviction story.
pub fn coverage_map_path(dataset: &str, machine_class: &str) -> std::path::PathBuf {
    coverage_map_path_in(
        &crate::cognition::swe_bench::swe_cache_dir(),
        dataset,
        machine_class,
    )
}

/// Root-injected core of [`coverage_map_path`] — the filesystem seam, so tests
/// run against a tempdir instead of the operator's real `~/.continuum` (the
/// pattern `tool_executor::spill` already sets). A test that writes into a
/// person's live data directory is both a lie about isolation and a way to
/// clobber their state.
pub fn coverage_map_path_in(
    root: &std::path::Path,
    dataset: &str,
    machine_class: &str,
) -> std::path::PathBuf {
    let safe: String = dataset
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let mc: String = machine_class
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    root.join("coverage").join(format!("{safe}.{mc}.json"))
}

pub(super) fn write_coverage_map(dataset: &str, result: &BenchmarkValidateResult) -> Result<(), String> {
    let path = coverage_map_path(dataset, &result.platform.machine_class);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// The named wall for an instance's env class, when THIS box has already proven
/// that class red. `None` means "no map, or the class is green, or the map was
/// earned on a different machine class" — every one of which must DISPATCH, not
/// block. Fail-open is the contract: a repo user who has never run
/// `benchmark/validate` is never slowed down by a gate that has nothing to say.
pub fn known_red_wall(dataset: &str, repo: &str, era_year: u32) -> Option<String> {
    known_red_wall_in(
        &crate::cognition::swe_bench::swe_cache_dir(),
        dataset,
        repo,
        era_year,
    )
}

/// Root-injected core of [`known_red_wall`]. See [`coverage_map_path_in`].
pub fn known_red_wall_in(
    root: &std::path::Path,
    dataset: &str,
    repo: &str,
    era_year: u32,
) -> Option<String> {
    let machine_class = BenchmarkPlatformFingerprint::capture().machine_class;
    let raw = std::fs::read_to_string(coverage_map_path_in(root, dataset, &machine_class)).ok()?;
    let map: BenchmarkValidateResult = serde_json::from_str(&raw).ok()?;
    if map.platform.machine_class != machine_class {
        return None; // another box's claim is not evidence about this one
    }
    let era = era_year.to_string();
    map.classes
        .iter()
        .find(|c| c.repo == repo && c.era == era && !c.green)
        .map(|c| {
            c.wall
                .clone()
                .unwrap_or_else(|| "class proven red by benchmark/validate".to_string())
        })
}

crate::register_stateless_command!(BenchmarkValidate);

#[derive(Default)]
pub struct BenchmarkRuns;

#[async_trait]
impl ActionCommand for BenchmarkRuns {
    const NAME: &'static str = "benchmark/runs";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The benchmark RunProjection: every agent/solve run's live card — phase \
         (active/quiet/resolved/failed/ungradeable), last-activity age, stall flag, acts, \
         grade summary, \
         investigation trail — folded from the run ledgers. ONE projection for every consumer: \
         the exam-room tab bar, a teacher persona's grounding, and the operator's liveness \
         monitor all read THIS instead of scraping files. `quiet` (stalled=true) is the shape \
         it exists to expose: a non-terminal run with no artifact activity past the stall \
         window — silence must never be ambiguous with progress.";
    type Params = BenchmarkRunsParams;
    type Output = BenchmarkRunsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRunsParams,
    ) -> Result<BenchmarkRunsResult, CommandError> {
        // safe: `limit` is an OPTIONAL page size, so None means "caller didn't ask", not
        // "unknown quantity" — 20 is this command's documented default page and the value
        // the board already uses. `.max(1)` keeps an explicit 0 from returning nothing
        // silently. The count that must never be defaulted is `matched`, which comes from
        // the scan itself and is reported separately.
        let scan = scan_run_cards(p.run_id.as_deref(), p.limit.unwrap_or(20).max(1) as usize) // safe: see the 5 lines above
            .map_err(CommandError::Internal)?;
        let returned = scan.cards.len();
        let matched = scan.matched;
        // Say which of the two this is, in words. A caller comparing `runs.len()` to
        // `matched` would also learn it, but the receipt is what a human (or a citizen
        // reading the board) actually looks at, and the whole point is that truncation
        // must not be invisible there.
        let summary = if returned < matched {
            format!(
                "showing {returned} of {matched} run(s) — NEWEST first, older runs truncated \
                 by `limit`. Raise `--limit` to see the rest; this is a PAGE, not the whole \
                 history."
            )
        } else {
            format!("all {matched} run(s) — this is the complete set for these filters, not a page.")
        };
        Ok(BenchmarkRunsResult {
            runs: scan.cards,
            matched: matched as u32,
            summary,
        })
    }
}
