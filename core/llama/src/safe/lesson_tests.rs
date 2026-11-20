//! A lesson is her ordinary decode with labels (IN-PROCESS-LEARNING.md slice 1; ONE-RESIDENT §9.6 P1
//! gates c and d), on a real model: `CONTINUUM_TEST_LEARN_MODEL` (a small dense GGUF, e.g.
//! qwen2.5-0.5b-instruct Q4_K_M), `CONTINUUM_TEST_GPU_LAYERS` (0 for CPU, 99 for Metal/CUDA).

use std::sync::Arc;

use super::learn_fixtures::{Counting, Open};
use super::*;

const PREFIX: &str = "The resident keeps her own work and she learns from what she lived. ";
const REPLY: &str = "Room card claim review patch lease board turn engine window chunk cache.";

fn model() -> Model {
    let path = std::env::var_os("CONTINUUM_TEST_LEARN_MODEL").expect("fixture model: CONTINUUM_TEST_LEARN_MODEL");
    let gpu = std::env::var("CONTINUUM_TEST_GPU_LAYERS").ok().and_then(|layers| layers.parse().ok()).unwrap_or(0); // unwrap_or: CPU unless a test run names a backend
    Model::load(path, ModelParams { n_gpu_layers: gpu, use_mmap: true }).expect("load the fixture model")
}

fn context(model: &Model, rows: u32) -> Context<'_> {
    model
        .new_context(ContextParams { n_ctx: 512, n_batch: rows, n_ubatch: rows, pooling_type: PoolingType::None, ..Default::default() })
        .expect("a context")
}

/// Her context's rows (unlabelled; the last returns logits), then the reply's rows: labelled with the
/// token after each when `labelled`, else plain rows returning logits.
fn lesson_batch(prefix: &[i32], reply: &[i32], next: i32, labelled: bool) -> Batch {
    let rows = i32::try_from(prefix.len() + reply.len()).expect("a small fixture");
    let mut batch = if labelled { Batch::labelled(rows) } else { Batch::allocated(rows, 1) };
    for (pos, &token) in prefix.iter().enumerate() {
        batch.push(token, pos as i32, &[0], pos + 1 == prefix.len());
    }
    for (offset, &token) in reply.iter().enumerate() {
        let pos = (prefix.len() + offset) as i32;
        let target = reply.get(offset + 1).copied().unwrap_or(next); // unwrap_or: the reply's last token predicts the token after it
        if labelled {
            batch.push_labelled(Row { token, pos, seq: 0 }, Label::Target { token: target, weight: 1.0 }).expect("a labelled row");
        } else {
            batch.push(token, pos, &[0], true);
        }
    }
    batch
}

/// The mean cross-entropy of the reply's rows, read from logits rows `first..first + targets.len()`.
fn loss(ctx: &Context<'_>, first: i32, targets: &[i32]) -> f64 {
    let total: f64 = targets
        .iter()
        .enumerate()
        .map(|(row, &target)| {
            let logits = ctx.logits_ith(first + row as i32);
            let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
            let sum: f64 = logits.iter().map(|&logit| (logit as f64 - max).exp()).sum();
            max + sum.ln() - logits[target as usize] as f64
        })
        .sum();
    total / targets.len() as f64
}

// what this catches: learning as anything but her ordinary decode. A labelled batch must train her
// shadow (its loss falls, native code counts committed steps) while the batch's unlabelled rows are
// byte-identical to a decode with no shadow at all (P1 c), and after the lesson, a rewind and her
// stable gene set again, her next decode is byte-identical to a context that never learned (P1 d).
#[test]
fn a_lesson_is_her_decode_with_labels_and_leaves_her_stable_self_untouched() {
    let model = model();
    let prefix = model.tokenize(PREFIX, true, false).expect("tokenize");
    let mut reply = model.tokenize(REPLY, false, false).expect("tokenize");
    let next = reply.pop().expect("a reply of several tokens");
    let targets: Vec<i32> = reply.iter().skip(1).copied().chain(std::iter::once(next)).collect();
    let rows = u32::try_from(prefix.len() + reply.len()).expect("a small fixture");
    // `logits_ith` reads by batch row: the prefix's last row, then the reply's rows after it.
    let prefix_last = prefix.len() as i32 - 1;
    let reply_rows = prefix.len() as i32;

    // Her stable self: no shadow anywhere.
    let mut stable = context(&model, rows);
    stable.decode(&lesson_batch(&prefix, &reply, next, false)).expect("a plain decode");
    let stable_prefix_row = stable.logits_ith(prefix_last).to_vec();
    let stable_loss = loss(&stable, reply_rows, &targets);

    // The same, learning: her shadow is the trainable adapter for each lesson.
    let directory = std::env::temp_dir().join(format!("continuum-lesson-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let path = directory.join("her-shadow.gguf");
    let shape = ShadowShape { rank: 8, alpha: 16.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 0, seed: 42 };
    model.write_fresh_shadow(&path, &shape).expect("a fresh shadow");
    let adapter = Arc::new(model.load_lora(&path).expect("load her shadow"));
    let mut ctx = context(&model, rows);
    let held = Arc::new(std::sync::atomic::AtomicU64::new(0));
    assert!(Learner::new(&mut ctx).retain_shadow(Arc::clone(&adapter), Box::new(Counting(Arc::clone(&held)))).is_ok());
    let shadow = Shadow {
        adapter: Arc::clone(&adapter),
        optimizer: Optimizer::AdamW { learning_rate: 1e-3, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 },
        recompute: false,
        walk: Walk::default(),
    };

    let mut losses = Vec::new();
    for repeat in 0..4 {
        assert!(ctx.memory_seq_rm(0, 0, -1));
        ctx.set_loras(&[(adapter.as_ref(), 1.0)]).expect("her shadow on the context");
        let lesson = Learner::new(&mut ctx).select(&shadow, Box::new(Open)).map_err(|refused| refused.refusal).expect("her shadow selected");
        // A second lesson while one is open is refused with its own admission, never written over
        // the first's (Codex's review of #6203).
        assert!(Learner::new(&mut ctx).select(&shadow, Box::new(Open)).is_err(), "repeat {repeat}: a second lesson opened over the first");
        ctx.decode(&lesson_batch(&prefix, &reply, next, true)).expect("her lesson decoded");
        assert_eq!(ctx.logits_ith(prefix_last), stable_prefix_row.as_slice(), "repeat {repeat}: an unlabelled row moved under her shadow (P1 c)");
        losses.push(loss(&ctx, reply_rows, &targets));
        match Learner::new(&mut ctx).conclude(lesson).expect("the lesson open on this context") {
            Finished::Learned { steps, .. } => assert!(steps.get() >= 1),
            Finished::Unchanged { .. } => panic!("repeat {repeat}: a labelled decode committed no step"),
            Finished::Uncertain { steps, .. } => panic!("repeat {repeat}: native code marked her shadow uncertain after {steps} steps"),
            Finished::Failed { refusal, .. } => panic!("repeat {repeat}: {refusal:?}"),
        }
    }
    eprintln!("stable loss {stable_loss:.4}; lesson losses {losses:?}; optimizer bytes held {}", held.load(std::sync::atomic::Ordering::Relaxed));
    assert!((losses[0] - stable_loss).abs() < 1e-3, "a fresh shadow (B = 0) changed the reply's loss");
    assert!(losses[3] < losses[0], "the lesson's loss did not fall: {losses:?}");

    // Back to her stable self: rewind to her context and serve with her stable genes (none here).
    // The reference is the stable context with the same history (the same batch shapes), rewound
    // the same way, so only the lesson differs between the two.
    let probe = |at: i32| {
        let mut batch = Batch::allocated(1, 1);
        batch.push(reply[0], at, &[0], true);
        batch
    };
    let at = prefix.len() as i32;
    assert!(ctx.memory_seq_rm(0, at, -1));
    ctx.clear_loras().expect("her stable gene set");
    ctx.decode(&probe(at)).expect("her next decode");
    assert!(stable.memory_seq_rm(0, at, -1));
    stable.decode(&probe(at)).expect("the stable probe");
    assert_eq!(ctx.logits_ith(0), stable.logits_ith(0), "after the lesson her served state is not her stable self's (P1 d)");

    assert!(Learner::new(&mut ctx).release_shadow(&adapter).is_ok());
    std::fs::remove_dir_all(&directory).ok();
}
