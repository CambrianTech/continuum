//! A mind learns in place on a live engine, end to end (card 4114a2c6): seat, learning memory,
//! grant, two learning steps on one lived example, stop. The resource authority here is a single
//! large mock capacity, so this proves the wiring and the engine's custody, not the node's budget.
//!
//!   cargo run -p continuum-genome --features test-fixtures --example learn_in_place -- http://127.0.0.1:58199

use continuum_genome::genome::learner::mind::MindLearner;
use continuum_genome::genome::learner::seat::Authority;
use continuum_genome::inference::llama_server::{FreshShadow, LearningEngine, LedgerAnswer};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), String> {
    let root = std::env::args().nth(1).ok_or("usage: learn_in_place URL")?;
    let engine = LearningEngine::at(root);
    let resources = continuum_genome::resources::ResourceDaemon::start(
        vec![
            Arc::new(continuum_genome::resources::MockCapacitySource::new(continuum_genome::resources::ResourceKind::Vram, 64 << 30)),
            // a CPU engine holds the shadow and its optimizer in host memory: leased as RAM
            Arc::new(continuum_genome::resources::MockCapacitySource::new(continuum_genome::resources::ResourceKind::Ram, 64 << 30)),
        ],
        vec![],
        continuum_genome::resources::DaemonConfig::default(),
    );
    let authority = Authority { resources, serving: continuum_genome::inference::serving_lifecycle::LifecycleGate::unowned(true) };
    let shadow = FreshShadow {
        name: format!("e2e-{}.gguf", uuid::Uuid::new_v4().simple()),
        rank: 8,
        alpha: 16.0,
        targets: vec!["attn_q".into(), "attn_v".into()],
        top_layers: 1,
        seed: 7,
    };
    let LedgerAnswer::Ledger(ledger) = engine.learn_ledger().await.map_err(|e| format!("ledger: {e}"))? else {
        return Err("the engine has no learning ledger".into());
    };
    // the zero grant's backstop is the engine's own maximum, as it reports it; stop revokes it
    let expiry = std::time::Duration::from_millis(ledger.max_expiry_ms);
    let mut workspace = None;
    let mut mind = MindLearner::start(&authority, engine.clone(), "learning:e2e", shadow, expiry).await.map_err(|why| format!("start: {why}"))?;
    println!("seated shadow {} with an uncontended-only grant", mind.shadow());
    let example = vec![
        serde_json::json!({"role": "user", "content": "What is the capital of Kansas?"}),
        serde_json::json!({"role": "assistant", "content": "Topeka."}),
    ];
    for step in 1..=2 {
        let outcome = mind.learn(&authority, &mut workspace, example.clone(), None, 0, 0.001).await.map_err(|e| format!("learn {step}: {e}"))?;
        println!("step {step}: {outcome:?}");
    }
    if let Ok(LedgerAnswer::Ledger(ledger)) = engine.learn_ledger().await {
        println!("while learning: {} residencies, optimizer buffers {:?}", ledger.residencies.len(), ledger.memory.optimizer.iter().map(|b| b.bytes).collect::<Vec<_>>());
        for residency in &ledger.residencies {
            let rows: Vec<_> = residency.rows.iter().map(|row| format!("{:?} {} {}", row.kind, row.buffer_type, row.bytes)).collect();
            println!("  residency {}: {}", residency.subject, rows.join(", "));
        }
    }
    mind.stop().await.map_err(|(_, why)| format!("stop: {why}"))?;
    // her optimizer was the engine's last: its workspace residency goes too
    if let Some(held) = workspace.take() {
        held.release(&engine).await.map_err(|(_, why)| format!("workspace release: {why}"))?;
    }
    if let Ok(LedgerAnswer::Ledger(ledger)) = engine.learn_ledger().await {
        println!("after stop: {} residencies, optimizer buffers {:?}", ledger.residencies.len(), ledger.memory.optimizer.iter().map(|b| b.bytes).collect::<Vec<_>>());
    }
    Ok(())
}
