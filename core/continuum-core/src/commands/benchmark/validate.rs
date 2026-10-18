//! `benchmark/validate`: machine-class coverage validation, the coverage-map files, and the known-red-wall lookup.

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use super::*;

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

fn write_coverage_map(dataset: &str, result: &BenchmarkValidateResult) -> Result<(), String> {
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
