//! What a learning lesson holds on a real model, by reply and lesson rows: the measure admission
//! reads, with nothing allocated. Run on the node's base to see which rows a lesson's bytes follow.
//! Run: cargo run -p llama --features metal --release --example lesson_size -- <model.gguf> <n_ctx> <reply:lesson>...

use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;

use llama::{ContextParams, FlashAttn, FreshGeometry, Learner, MemoryRole, Model, ModelParams, Optimizer, PoolingType, Shadow, ShadowMeasure, Walk};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().ok_or("a model path")?);
    let n_ctx: u32 = args.next().ok_or("n_ctx")?.parse()?;
    let pairs: Vec<(NonZeroU32, NonZeroU32)> = args
        .map(|pair| {
            let (reply, lesson) = pair.split_once(':').ok_or("reply:lesson")?;
            Ok((reply.parse()?, lesson.parse()?))
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;

    let model = Model::load(&path, ModelParams::default())?;
    let scratch = std::env::temp_dir().join(format!("lesson-size-{}.gguf", std::process::id()));
    let targets = vec!["attn_q".to_string(), "attn_v".to_string()];
    model.write_fresh_shadow(&scratch, &FreshGeometry { rank: 8, alpha: 16.0, targets: &targets, seed: 42, top_layers: 0 })?;
    let adapter = Arc::new(model.load_lora(&scratch)?);
    std::fs::remove_file(&scratch)?;
    let params = ContextParams { n_ctx, pooling_type: PoolingType::None, flash_attn: FlashAttn::Enabled, ..ContextParams::default() };
    let mut ctx = model.new_context(params)?;
    ctx.set_loras(&[(&adapter, 1.0)])?;
    let shadow = Shadow {
        adapter: Arc::clone(&adapter),
        optimizer: Optimizer::AdamW { learning_rate: 1e-4, beta1: 0.9, beta2: 0.999, eps: 1e-8, weight_decay: 0.0 },
        recompute: false,
        walk: Walk::default(),
    };
    let mut learner = Learner::new(&mut ctx);
    for (reply, lesson) in pairs {
        let ShadowMeasure { optimizer, workspace } = learner.measure(&shadow, reply, lesson)?;
        let sum = |rows: &[llama::MemoryRow], role: MemoryRole| rows.iter().filter(|row| row.role == role).map(|row| row.bytes).sum::<u64>();
        println!(
            "reply {reply:>6} lesson {lesson:>6}: optimizer {:>14} B, workspace required {:>16} B, held {:>14} B",
            sum(&optimizer, MemoryRole::OptimizerStatic),
            sum(&workspace, MemoryRole::SharedComputeRequired),
            sum(&workspace, MemoryRole::SharedComputeHeld),
        );
    }
    Ok(())
}
