//! What an activity ADAPTS — material in, verdict out.
//!
//! This is the one seam every activity plugs into: SWE-bench, a HumanEval-style gym, a
//! game level, a life-drawing critique, a robot that either picked up the block or didn't.
//! The room is the runner; an adapter only translates between the outside world's form and
//! ours ([[benchmarks-are-adapters-not-a-runner]]).
//!
//! # Why this is not `BenchmarkAdapter` any more
//!
//! The interface it replaces was the right SHAPE and the wrong SCOPE. It already carried
//! both directions — `dataset()`/`tasks()` bring material IN, `grade()` renders a verdict
//! OUT — but it was named for one subsystem, and a name is load-bearing: an interface called
//! `BenchmarkAdapter` cannot plausibly host a game level, so the second activity gets its own
//! parallel trait, its own registry, its own runner, and the substrate now has two of
//! everything. That is the same failure as naming a room for a subsystem and making it
//! immortal. Named for the JOB (an activity adapting its material), gameplay is a sibling
//! implementation instead of a fork.
//!
//! # Both directions are the same kind of thing
//!
//! - **IN** — [`ActivityAdapter::material`] declares where the outside form lives and
//!   [`ActivityAdapter::tasks`] normalizes it into our canonical [`EvalTask`]. For a
//!   benchmark that is "task + oracle, nothing else" — deliberately NOT the upstream
//!   harness, which we never run.
//! - **OUT** — [`ActivityAdapter::judge`] turns what the citizen actually produced into a
//!   [`Verdict`].
//!
//! # Declaring none is normal
//!
//! Most activities score nothing. `chat` names no adapters and nothing judges it; there is
//! no "the ungraded kind" branch anywhere, because an empty list is already the answer
//! (Joel, 2026-08-21: *"always adapter(s) with none as an option … formulaic or not,
//! anything can plug in. Just build the ones you initially need"*). Judging lives in the
//! recipe's `params`, never on the base recipe type — an exam concept stamped onto every
//! chat room and profile page is exactly the over-reach this seam exists to avoid.
//!
//! # The growable outcome, and why it is not a fixed struct
//!
//! [`Outcome`] carries what the citizen produced. The universal parts are named fields:
//! every activity has a mouth (what she said) and a place she worked. Everything modality-
//! specific goes in [`Outcome::channels`] — a typed, growable carrier, the same shape the
//! adapter capability surface already uses in this codebase.
//!
//! This matters for the reason the fixed struct failed: with `patch` and `workspace` as
//! bare fields, SWE-bench OWNS the outcome type, and a game trajectory or a robot's joint
//! log can only be added by editing a struct that every other activity compiles against. A
//! new modality would either widen a shared type for everyone or — far more likely, and what
//! actually happens — grow its own parallel outcome and its own parallel judge. Channels let
//! gameplay add [`Channel::Trajectory`] without SWE-bench recompiling a line, and let a judge
//! ask for exactly the channel it grades and get a typed `None` when the activity did not
//! produce one.
//!
//! Growing the enum is still a deliberate act: a new variant is added when a REAL activity
//! needs it, never speculatively. What changed is the cost of being wrong — a wrong guess is
//! now one unused variant instead of a second subsystem.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::cognition::eval::EvalTask;

/// Where an activity's material comes from — declared by the adapter, fetched ON DEMAND by
/// the room that runs it, never bundled in the repo.
#[derive(Debug, Clone)]
pub struct MaterialSpec {
    /// Hugging Face repo id (`org/name`), an HTTPS URL, or a git remote — read per `kind`.
    pub source: String,
    /// How to fetch `source`. Enumerated so a new source kind forces the fetch match to be
    /// revisited instead of silently skipped.
    pub kind: MaterialKind,
    /// Subdirectory under the shared cache where the material materializes. ONE canonical
    /// cache, so a fetched suite is reused across runs and citizens.
    pub cache_key: String,
    /// Optional glob restricting what to fetch (e.g. only `*.jsonl`, skip weights).
    pub include: Option<String>,
}

/// The fetch strategy for a [`MaterialSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialKind {
    /// Hugging Face dataset/model repo — snapshot-style fetch.
    Hf,
    /// A single HTTPS artifact the adapter parses.
    Url,
    /// A git repository (real-repo activities clone per-task from here).
    Git,
    /// Already resident — generated, seeded, or shipped in-tree. Nothing to fetch.
    Resident,
}

/// A placement hint the grid uses to route an activity to a capable node. All fields
/// optional: an unknown hint means "place anywhere", never a hard block.
#[derive(Debug, Clone, Default)]
pub struct ResourceHint {
    /// Approx disk the material needs once materialized (bytes).
    pub disk_bytes: Option<u64>,
    /// True if judging needs a container runtime. A node without one is not a candidate —
    /// surfaced as data, never a crash.
    pub needs_container: bool,
    /// True if tasks need outbound web access.
    pub needs_network: bool,
}

/// A modality channel on an [`Outcome`] — what the citizen produced, beyond speech.
///
/// One variant per REAL modality some activity actually judges. Adding a variant is how a
/// new kind of activity arrives; it costs every other adapter nothing, because judges match
/// on the channel they care about and treat absence as `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    /// Unified `git diff` of everything she changed — what SWE/Terminal-style activities grade.
    Diff,
    /// A relative in-workspace path she was told to write her solution to (artifact grade).
    Artifact,
}

/// The value on a [`Channel`]. Kept deliberately narrow — a channel carries either text or a
/// path today, and a judge that needs richer structure parses it. Widening this is a
/// deliberate act with a real activity behind it.
#[derive(Debug, Clone)]
pub enum ChannelValue {
    Text(String),
    Path(PathBuf),
}

impl ChannelValue {
    /// The text of this value, or `None` if it is a path. Judges use this instead of
    /// matching, so an activity that stored the wrong shape reads as absent rather than
    /// panicking inside a judge.
    pub fn text(&self) -> Option<&str> {
        match self {
            ChannelValue::Text(t) => Some(t),
            ChannelValue::Path(_) => None,
        }
    }

    /// The path of this value, or `None` if it is text.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            ChannelValue::Path(p) => Some(p),
            ChannelValue::Text(_) => None,
        }
    }
}

/// What one citizen actually produced in one task — the input every judge grades.
///
/// Universal facts are named fields. Modality-specific ones live in [`Self::channels`] so a
/// new activity adds a modality without widening a type its siblings compile against.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    /// Her final spoken answer (mouth). Some activities grade this directly.
    pub spoken: String,
    /// The workspace after she acted — where a judge re-runs the activity's own checks.
    pub workspace: PathBuf,
    /// Whether the generic harness already passed it, and why. This is the harness's OWN
    /// verdict, not the activity's: a judge may confirm, override, or ignore it.
    pub harness_passed: bool,
    /// Human-readable detail from the generic harness grade.
    pub harness_detail: String,
    /// Modality channels, keyed by [`Channel`]. Absent = the activity produced none.
    pub channels: BTreeMap<Channel, ChannelValue>,
}

impl Outcome {
    /// The value on `channel`, or `None` when this activity produced nothing there.
    pub fn channel(&self, channel: Channel) -> Option<&ChannelValue> {
        self.channels.get(&channel)
    }

    /// Text on `channel`, or `""` when absent — the ergonomic read for judges that treat a
    /// missing modality the same as an empty one (a diff-grading judge does).
    pub fn channel_text(&self, channel: Channel) -> &str {
        self.channel(channel).and_then(|v| v.text()).unwrap_or("")
    }

    /// Attach `value` to `channel`, replacing anything already there.
    pub fn with_channel(mut self, channel: Channel, value: ChannelValue) -> Self {
        self.channels.insert(channel, value);
        self
    }
}

/// One activity's verdict on one task — did it pass, how well, and WHY.
///
/// `reason` is not decoration: it is the receipt a citizen and a human both read, and the
/// text the curriculum keeps when the task failed.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub passed: bool,
    /// Normalized 0..1. A pass/fail judge reports 1.0/0.0; a scored one reports its scale.
    pub score: f64,
    pub reason: String,
}

impl Verdict {
    /// A passing verdict with a reason.
    pub fn pass(reason: impl Into<String>) -> Self {
        Self { passed: true, score: 1.0, reason: reason.into() }
    }

    /// A failing verdict with a reason.
    pub fn fail(reason: impl Into<String>) -> Self {
        Self { passed: false, score: 0.0, reason: reason.into() }
    }
}

/// One activity, as a plug-in: it adapts material IN and renders a verdict OUT.
///
/// The room runs the activity; this only translates. Implement once per activity kind —
/// the runner, the grid placement, and the learning loop treat every adapter identically.
#[async_trait::async_trait]
pub trait ActivityAdapter: Send + Sync {
    /// Stable slug, used on the CLI, in a recipe's declaration, and on the scorecard.
    /// Must match the registry key.
    fn name(&self) -> &str;

    /// Where this activity's material comes from, or `None` when it needs none (generated,
    /// seeded, or already resident). The room materializes this before [`Self::tasks`].
    fn material(&self) -> Option<MaterialSpec> {
        None
    }

    /// What kind of node can run this — for grid placement. Default: anywhere.
    fn resources(&self) -> ResourceHint {
        ResourceHint::default()
    }

    /// Load this activity's items into our canonical [`EvalTask`] shape from the
    /// already-materialized root (`None` when [`Self::material`] is `None`). `limit` caps
    /// items for a quick pulse. THE ONLY per-activity parsing — everything downstream is
    /// generic.
    async fn tasks(
        &self,
        material_root: Option<&std::path::Path>,
        limit: Option<usize>,
    ) -> Result<Vec<EvalTask>, String>;

    /// Judge what she produced.
    ///
    /// The DEFAULT defers to the harness's own verdict, which is correct for activities
    /// whose `EvalTask` already encodes the definition of done (a `dod_shell`, a `test`, an
    /// `expect`). Activities that must inspect the world after she acted — re-run a repo's
    /// held-out tests, read a final score, measure a trajectory — override this.
    async fn judge(&self, _task: &EvalTask, outcome: &Outcome) -> Verdict {
        Verdict {
            passed: outcome.harness_passed,
            score: if outcome.harness_passed { 1.0 } else { 0.0 },
            reason: if outcome.harness_detail.is_empty() {
                "harness verdict".to_string()
            } else {
                outcome.harness_detail.clone()
            },
        }
    }
}

/// A builtin adapter that self-registers at link time via `inventory` — the same mechanism
/// commands use, so a resident activity is discoverable with NO boot hook and NO central
/// list to edit. Adding an activity is adding a file.
pub struct BuiltinActivityAdapter {
    /// Constructs the adapter. A `fn` pointer (not a value) so submission stays const-evaluable.
    pub make: fn() -> Arc<dyn ActivityAdapter>,
}

inventory::collect!(BuiltinActivityAdapter);

/// Runtime-registered adapters (tests, and any future dynamic registration). Builtins come
/// from `inventory` and are folded in by [`get`]/[`names`].
static RUNTIME: std::sync::LazyLock<std::sync::RwLock<BTreeMap<String, Arc<dyn ActivityAdapter>>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(BTreeMap::new()));

/// Register (or replace) an adapter at runtime. Idempotent by name. Builtins self-register
/// and need NOT be passed here.
pub fn register(adapter: Arc<dyn ActivityAdapter>) {
    if let Ok(mut w) = RUNTIME.write() {
        w.insert(adapter.name().to_string(), adapter);
    }
}

/// Look up an adapter by name. Runtime registrations win over builtins so a test can shadow
/// a builtin; otherwise the link-time set is searched.
pub fn get(name: &str) -> Option<Arc<dyn ActivityAdapter>> {
    if let Ok(r) = RUNTIME.read() {
        if let Some(a) = r.get(name) {
            return Some(a.clone());
        }
    }
    inventory::iter::<BuiltinActivityAdapter>
        .into_iter()
        .map(|b| (b.make)())
        .find(|a| a.name() == name)
}

/// Every known adapter name (runtime ∪ builtin), sorted and deduped — for a fail-loud
/// "unknown activity 'X'; known: …" instead of a silent miss.
pub fn names() -> Vec<String> {
    let mut out: Vec<String> = inventory::iter::<BuiltinActivityAdapter>
        .into_iter()
        .map(|b| (b.make)().name().to_string())
        .collect();
    if let Ok(r) = RUNTIME.read() {
        out.extend(r.keys().cloned());
    }
    out.sort();
    out.dedup();
    out
}

/// Resolve every adapter a recipe declared, or say EXACTLY which name is unknown.
///
/// This is the seam that makes "the exam was sat and nobody marked it" unrepresentable. A
/// declared-but-unresolvable judge is an error at the moment of staging, not a run that
/// quietly ends with no verdict — the failure mode that produced 253 of 450 unmarked runs
/// (measured 2026-08-21) and that no amount of citizen effort could have avoided.
///
/// An EMPTY declaration resolves to an empty vec and is NOT an error: most activities judge
/// nothing, and that is normal, not a missing judge.
pub fn resolve_all(declared: &[String]) -> Result<Vec<Arc<dyn ActivityAdapter>>, String> {
    let mut out = Vec::with_capacity(declared.len());
    for name in declared {
        match get(name) {
            Some(a) => out.push(a),
            None => {
                return Err(format!(
                    "unknown activity adapter '{name}' — known: {}. \
                     A recipe that names a judge nothing can resolve must fail HERE, at \
                     staging, not by producing a run that ends with no verdict.",
                    names().join(", ")
                ))
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub;

    #[async_trait::async_trait]
    impl ActivityAdapter for Stub {
        fn name(&self) -> &str {
            "stub-activity"
        }
        async fn tasks(
            &self,
            _root: Option<&std::path::Path>,
            _limit: Option<usize>,
        ) -> Result<Vec<EvalTask>, String> {
            Ok(vec![])
        }
    }

    // what this catches: the registry must round-trip a registration AND name the unknown
    // key on a miss. A silent `None` here is how a declared judge goes missing without
    // anyone noticing.
    #[test]
    fn registry_round_trips_and_names_the_unknown() {
        register(Arc::new(Stub));
        assert!(get("stub-activity").is_some());
        assert!(get("no-such-activity").is_none());
        assert!(names().iter().any(|n| n == "stub-activity"));
    }

    // what this catches: declaring NO judge is legal and yields no judges — the "none is
    // normal" contract. If this ever errors, every chat room becomes an exam.
    #[test]
    fn declaring_no_judge_is_legal_and_not_an_error() {
        let resolved = resolve_all(&[]).expect("empty declaration must resolve, not error");
        assert!(resolved.is_empty());
    }

    // what this catches: a declared-but-unknown judge fails LOUD at resolve time and the
    // message names both the bad key and the known set. This is the guard that makes an
    // unmarked run impossible — regression for the 253/450 unmarked runs of 2026-08-21.
    #[test]
    fn an_unknown_declared_judge_fails_loud_and_names_it() {
        register(Arc::new(Stub));
        // Matched rather than `expect_err`: the Ok side is `Vec<Arc<dyn ActivityAdapter>>`,
        // and a trait object is not `Debug`. Requiring `Debug` on the trait just to phrase a
        // test would put a constraint on every future adapter to serve the test's ergonomics.
        let err = match resolve_all(&["stub-activity".into(), "ghost-judge".into()]) {
            Ok(_) => panic!("an unresolvable judge must be an error, never a silent skip"),
            Err(e) => e,
        };
        assert!(err.contains("ghost-judge"), "must name the offending key: {err}");
        assert!(err.contains("stub-activity"), "must list what IS known: {err}");
    }

    // what this catches: the default judge defers to the harness verdict, so an activity
    // whose EvalTask already encodes done-ness needs no judge code at all.
    #[tokio::test]
    async fn default_judge_defers_to_the_harness() {
        let task = EvalTask::default();
        let passed = Outcome { harness_passed: true, ..Default::default() };
        assert!(Stub.judge(&task, &passed).await.passed);
        let failed = Outcome {
            harness_passed: false,
            harness_detail: "tests failed".into(),
            ..Default::default()
        };
        let v = Stub.judge(&task, &failed).await;
        assert!(!v.passed);
        assert_eq!(v.reason, "tests failed");
    }

    // what this catches: a channel a judge asks for but the activity never produced reads as
    // absent — NOT as an empty success. This is what lets gameplay add a modality without
    // SWE-bench's judge changing, and what stops a judge panicking on a missing one.
    #[test]
    fn an_absent_channel_reads_absent_not_empty_success() {
        let o = Outcome::default().with_channel(Channel::Diff, ChannelValue::Text("x".into()));
        assert_eq!(o.channel_text(Channel::Diff), "x");
        assert!(o.channel(Channel::Artifact).is_none());
        assert_eq!(o.channel_text(Channel::Artifact), "");
    }
}
