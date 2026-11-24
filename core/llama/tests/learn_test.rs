//! In-place learning on a real model (IN-PROCESS-LEARNING.md, the L1 validator, never run before
//! L2): a fresh shadow is measured, written and retained; one reply trains on the sequence that
//! served it, its loss falls across replies, and the sequence holds exactly what it held before.
//! Run: cargo test -p llama --features metal --test learn_test -- --nocapture

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use llama::{
    Batch, BufferType, ContextParams, Finished, FreshGeometry, Label, Learner, Model, ModelParams, Optimizer, OptimizerLease, PoolingType,
    Pos, Reply, ReplyAdmission, Shadow, Slot, Walk,
};

/// The 0.5B the genome trains on; absent on a node without it, which skips (said, never passed).
fn small_model() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var("HOME").ok()?).join(".continuum/genome/models/qwen2.5-0.5b-instruct/qwen2.5-0.5b-instruct-q4_k_m.gguf");
    path.exists().then_some(path)
}

/// A lease that admits everything and counts what is held: the bytes native code charged.
#[derive(Default)]
struct Counted(Arc<AtomicU64>);

impl OptimizerLease for Counted {
    fn admit(&self, _: Slot, bytes: u64) -> bool {
        self.0.fetch_add(bytes, Ordering::SeqCst);
        true
    }
    fn release(&self, _: Slot, bytes: u64) {
        self.0.fetch_sub(bytes, Ordering::SeqCst);
    }
}

struct Open;

impl ReplyAdmission for Open {
    fn workspace(&self, _: BufferType, _: u64, _: u64) -> bool {
        true
    }
}

// what this catches: in-place learning that does not reach the model (the M5 serves in-process and
// nothing has ever learned there): a fresh shadow that cannot be measured or written, a reply that
// does not train (its loss never falls), or a reply that leaves her sequence changed.
#[test]
fn a_reply_trains_her_shadow_and_leaves_her_sequence_as_it_was() {
    let Some(path) = small_model() else {
        eprintln!("SKIPPED: no qwen2.5-0.5b-instruct gguf under ~/.continuum/genome/models");
        return;
    };
    let model = Model::load(&path, ModelParams::default()).expect("test: the 0.5B loads");
    let dir = ScratchDir::new();
    let shadow_path = dir.path().join("shadow.gguf");
    let targets = vec!["attn_q".to_string(), "attn_v".to_string()];
    let geometry = FreshGeometry { rank: 8, alpha: 16.0, targets: &targets, seed: 42, top_layers: 0 };
    let measured = model.measure_fresh_shadow(&geometry).expect("test: a fresh shadow measures");
    model
        .write_fresh_shadow(&shadow_path, &geometry)
        .expect("test: a fresh shadow is written");
    let loaded = model.measure_adapter(&shadow_path).expect("test: the written shadow measures");
    let weight_bytes: u64 = measured.iter().map(|row| row.bytes).sum();
    assert!(weight_bytes > 0 && loaded == measured, "a fresh shadow measures, before it is written, as it loads: {measured:?} vs {loaded:?}");

    let adapter = Arc::new(model.load_lora(&shadow_path).expect("test: the shadow loads"));
    let params = ContextParams { n_ctx: 1024, pooling_type: PoolingType::None, ..ContextParams::default() };
    let mut ctx = model.new_context(params).expect("test: a generation context");
    // native code trains only a shadow attached at a non-zero scale; a fresh one leaves her output unchanged
    ctx.set_loras(&[(&adapter, 1.0)]).expect("test: the shadow attaches");

    // what she was served: a prompt, decoded into her sequence
    let prompt = model.tokenize("<|im_start|>user\nWhat is the capital of Kansas?<|im_end|>\n<|im_start|>assistant\n", true, true).expect("test: prompt");
    let mut batch = Batch::allocated(prompt.len() as i32, 1);
    for (i, &token) in prompt.iter().enumerate() {
        batch.push(token, Pos::at(i as u16), &[0], i + 1 == prompt.len());
    }
    ctx.decode(&batch).expect("test: her prompt decodes");
    let held = ctx.memory_seq_pos_max(0);

    let shadow = Shadow {
        adapter: Arc::clone(&adapter),
        optimizer: Optimizer::AdamW { learning_rate: 5e-3, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 },
        recompute: false,
        walk: Walk::default(),
    };
    let charged = Arc::new(AtomicU64::new(0));
    let mut learner = Learner::new(&mut ctx);
    let optimizer_rows = learner.measure_optimizer(&shadow).expect("test: the optimizer measures");
    let workspace_rows = learner.measure_workspace(&shadow, NonZeroU32::new(64).expect("test: nonzero")).expect("test: the workspace measures");
    eprintln!("measured: weights {weight_bytes} B, optimizer {optimizer_rows:?}, workspace {workspace_rows:?}");
    assert!(learner.retain_shadow(Arc::clone(&adapter), Box::new(Counted(Arc::clone(&charged)))).is_ok(), "the shadow is retained");

    // the reply to learn: from her last served token, the answer; the label after each token is the next
    let answer = model.tokenize("Topeka.<|im_end|>", false, true).expect("test: answer");
    let last = *prompt.last().expect("test: a prompt");
    let tokens: Vec<i32> = std::iter::once(last).chain(answer.iter().copied()).collect();
    let labels: Vec<Label> = (0..tokens.len()).map(|i| tokens.get(i + 1).map_or(Label::Unlabelled, |&next| Label::Target(next))).collect();
    let n_cached = prompt.len() as i32 - 1;
    let reply = Reply { seq: 0, n_cached, tokens: &tokens, labels: &labels, restore: &[last] };

    let mut losses: Vec<f64> = Vec::new();
    for round in 0..6 {
        let run = learner.begin(&shadow, Box::new(Open), &reply).map_err(|refused| refused.refusal).expect("test: the reply begins");
        let mut run = run;
        loop {
            match run.step().expect("test: a chunk runs") {
                llama::Progress::More => {}
                llama::Progress::Done => break,
            }
        }
        match run.finish() {
            Finished::Learned { steps, loss, .. } => {
                eprintln!("round {round}: {steps} step(s), loss {loss:?}");
                losses.push(loss.expect("test: a learned reply reports its loss"));
            }
            Finished::Unchanged { .. } => panic!("round {round}: nothing committed"),
            Finished::Uncertain { steps, .. } => panic!("round {round}: uncertain after {steps} steps"),
            Finished::Failed { refusal, .. } => panic!("round {round}: failed: {refusal}"),
        }
    }
    drop(learner);
    assert_eq!(ctx.memory_seq_pos_max(0), held, "her sequence holds what it held before the replies");
    let (first, last_loss) = (losses[0], losses[losses.len() - 1]);
    assert!(last_loss < first, "the reply trains: loss {first} -> {last_loss} over {} replies", losses.len());
    assert!(charged.load(Ordering::SeqCst) > 0, "the optimizer's bytes were charged to its lease");

    let saved = Learner::new(&mut ctx).save_generation(&adapter, &dir.path().join("generation"), "test-binding").expect("test: a generation saves");
    assert!(saved >= losses.len() as u64, "the saved generation holds every committed step: {saved}");
    assert!(dir.path().join("generation/adapter.gguf").exists() && dir.path().join("generation/optimizer.gguf").exists());
}

/// A scratch directory removed when the test ends.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("learn-test-{}-{}", std::process::id(), std::time::SystemTime::UNIX_EPOCH.elapsed().map_or(0, |d| d.as_nanos())));
        std::fs::create_dir_all(&dir).expect("test: a scratch dir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
