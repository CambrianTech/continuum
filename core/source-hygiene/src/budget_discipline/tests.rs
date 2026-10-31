use super::*;
use crate::{scan, SourceRule};

/// A fixture at repository path `repo`, its `rel` the path under its crate's `src/` as the scan
/// would give it.
fn file(repo: &str, code: &str) -> SourceFile {
    let rel = repo.split_once("/src/").map_or(repo, |(_, under)| under);
    SourceFile { repo: repo.to_owned(), ..SourceFile::for_test(rel, code) }
}

fn found(rule: &dyn SourceRule, repo: &str, code: &str) -> usize {
    rule.check(&file(repo, code)).len()
}

const CONSUMER: &str = "core/continuum-engine/src/ai/embedding/cache.rs";
const MIND: &str = "core/continuum-cognition/src/cognition/recall.rs";

/// Count `rule` over the tree, hold it against the base, and judge the change's files.
fn judge(rule: &dyn SourceRule, instead: &str) {
    let violations = scan(&[rule]);
    let count = violations.len();
    let advice = || format!("{instead} The contract: docs/architecture/ONE-BUDGET.md.");
    crate::ratchet::touched_clean(rule.name(), &violations, advice);
    crate::ratchet::hold(rule.name(), count, || {
        format!(
            "{} {instead} The contract: docs/architecture/ONE-BUDGET.md.\nFirst {} of {count} (not necessarily new):\n{}",
            rule.name(),
            count.min(20),
            violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
        )
    });
}

// what this catches: a consumer sizing itself (cache caps, KEEP counts, window fractions,
// head cuts, literal ceilings) while the budget it bypasses believes it owns the resource.
#[test]
fn self_sized_consumers_fall_and_touched_files_have_none() {
    judge(&SelfSized, "Take the size from the consumer's grant: claim it (floor = measured need, max = measured want) in the one allocation.");
}

// what this catches: a second measurement of memory, VRAM or disk that disagrees with the
// authority's (five VRAM probes and seven RAM reads on 365f70fe4).
#[test]
fn private_probes_fall_and_touched_files_have_none() {
    judge(&PrivateProbes, "Read the authority's published snapshot or your grant; never measure the resource yourself.");
}

// what this catches: a writer choosing its own path and retention, so its bytes have no
// owner when the disk fills (460 GB on the M5, 2026-07-13).
#[test]
fn writes_outside_a_class_fall_and_touched_files_have_none() {
    judge(&WritesOutsideAClass, "Write through the class handle for the directory (a TrackedDir with a claim) or through the ORM.");
}

// what this catches: each self-sizing shape the inventory found, and the allocator and the
// turn's allocation staying quiet (a rule that flagged its owners would be deleted as noise).
#[test]
fn each_self_sizing_shape_is_found_and_owners_are_not() {
    assert_eq!(found(&SelfSized, CONSUMER, "const EMBEDDING_CACHE_MAX: usize = 20_000;"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "pub(crate) const KEEP: usize = 8;"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let cap = FILL_TARGET_SECONDS * PREFILL_TOKENS_PER_S;"), 0, "a time product is derived_bounds' class");
    assert_eq!(found(&SelfSized, CONSUMER, "let cap = LANE_COUNT_MAX * SLOT_BYTES;"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let (tx, rx) = mpsc::channel(256);"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let (tx, rx) = broadcast::channel::<Event>(1024);"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let (tx, rx) = tokio::sync::watch::channel(0u64);"), 0, "a watch channel's argument is its initial value");
    assert_eq!(found(&SelfSized, CONSUMER, "let (tx, rx) = watch::channel::<u64>(0);"), 0, "nor in the turbofish form (Cormac on #5387)");
    assert_eq!(found(&SelfSized, CONSUMER, "rows.truncate(5);"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let shown = budget_chars.min(1_500);"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let n = retries.min(3);"), 0, "a .min on a line that names no size");
    assert_eq!(found(&SelfSized, CONSUMER, "let b = ContextBudget::live().trail_head_chars();"), 1);
    assert_eq!(found(&SelfSized, MIND, "let head: String = text.chars().take(width).collect();"), 1);
    assert_eq!(found(&SelfSized, CONSUMER, "let first = text.lines().next();"), 0, "a head read outside her text is parsing");
    assert_eq!(found(&SelfSized, MIND, "let grants = flex_allocate(window, &claims);"), 1);
    assert_eq!(
        found(&SelfSized, "core/continuum-cognition/src/cognition/llm_deliberation_faculty/prompt/view.rs", "let grants = flex_allocate(window, &claims);"),
        0,
        "the turn's allocation calls the allocator"
    );
    assert_eq!(found(&SelfSized, "core/continuum-grounding/src/rag_budget/flex_allocate.rs", "const PRIORITY_MAX: u8 = 10;"), 0, "the allocator owns its algorithm");
    assert_eq!(found(&SelfSized, CONSUMER, "const POLL_INTERVAL_MS: u64 = 2_000;"), 0, "a duration is not a size");
    assert_eq!(found(&SelfSized, CONSUMER, "fn cap(&self) -> usize { self.grant.take(8) }"), 1, "a body on the signature's line is code (Codex on #5609)");
    assert_eq!(found(&SelfSized, CONSUMER, "pub fn recall_tokens(&self) -> usize {"), 0, "a signature alone is a definition, not a use");
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "fn save(p: &Path) -> io::Result<()> { std::fs::write(p, b\"x\") }"), 1, "every rule sees a one-line body");
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "std::fs::write(path, \"fn main() {}\")?;"), 1, "a fn inside a literal is not a signature (Codex on #5609)");
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, r##"std::fs::write(path, r#"let source = "fn main() {}";"#)?;"##), 1, "nor inside a raw string with embedded quotes (Codex on #5609)");
    assert_eq!(found(&SelfSized, "core/vendored/llama/x.rs", "const N_CTX_MAX: usize = 4096;"), 0, "vendored upstream");
}

// what this catches: a private measurement outside the authority, and the authority's own
// probes staying quiet.
#[test]
fn a_private_probe_is_found_and_the_authority_is_not() {
    assert_eq!(found(&PrivateProbes, CONSUMER, "let mut sys = sysinfo::System::new();"), 1);
    assert_eq!(found(&PrivateProbes, CONSUMER, "let (free, total) = torch.cuda.mem_get_info();"), 1);
    assert_eq!(found(&PrivateProbes, CONSUMER, "Command::new(\"nvidia-smi\")"), 1);
    assert_eq!(found(&PrivateProbes, "core/continuum-kernel/src/system_resources/memory_pressure/poll.rs", "let mut sys = sysinfo::System::new();"), 0);
    assert_eq!(found(&PrivateProbes, "core/continuum-kernel/src/gpu/monitor.rs", "Command::new(\"nvidia-smi\")"), 0);
}

// what this catches: a write to a path the writer chose, and the ORM and the disk classes
// staying quiet.
#[test]
fn a_write_outside_a_class_is_found_and_the_owners_are_not() {
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "std::fs::write(&path, bytes)?;"), 1);
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "let f = File::create(path)?;"), 1);
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "let f = OpenOptions::new().append(true).open(p)?;"), 1);
    assert_eq!(found(&WritesOutsideAClass, "core/continuum-kernel/src/orm/sqlite/file.rs", "std::fs::write(&path, bytes)?;"), 0);
    assert_eq!(found(&WritesOutsideAClass, CONSUMER, "let text = std::fs::read_to_string(p)?;"), 0, "a read is not a write");
}

// what this catches: the head-cut coverage `head_cuts` had and a crate list alone would lose
// (Codex on #5609): text a mind reads lives in these module directories in any crate.
#[test]
fn a_head_cut_is_found_in_every_directory_head_cuts_covered() {
    let cut = "let spoken: String = text.chars().take(MAX_SPOKEN_CHARS).collect();";
    for repo in [
        "core/continuum-core/src/modules/voice_reply_speaker.rs",
        "core/continuum-core/src/airc/relay.rs",
        "core/continuum-genome/src/genome/teach/progress.rs",
        "core/continuum-core/src/experience/lived.rs",
        "core/continuum-core/src/recipe/loader.rs",
        "core/continuum-core/src/positron/chat.rs",
        "core/continuum-core/src/rag/engine.rs",
        "core/continuum-core/src/persona/unified.rs",
        "core/continuum-core/src/cognition/recall.rs",
    ] {
        assert_eq!(found(&SelfSized, repo, cut), 1, "{repo}");
    }
    assert_eq!(found(&SelfSized, "core/continuum-engine/src/inference/lane_args.rs", cut), 0, "engine argv is not her text");
}
