//! Both shapes of a gene signature on a real corpus (card 1d9a3512): the pairwise clusters and the
//! one-pass themes that replace them, on the same vectors, from this node's own recall embedder.
//! Reports the embed time, each shape's time, the matched-theme cosines, the assignment
//! disagreement and the region edges, at the current floor and, when the space is calibrated, at
//! floors read off its measured unrelated null.
//!
//!   cargo run -p continuum-genome --features test-fixtures --example signature_shapes -- <texts.jsonl> <contrast.jsonl>
//!
//! Each line of a file is one JSON string (a training example's text); a bucket's examples are
//! dumped as `prompt\ncompletion`, the text `mint_signature` embeds. Every fifth text is held out of
//! the mint and routed against; the contrast file is unrelated work the gene should not reach.

use continuum_genome::genome::signature::validate::{compare, CURRENT_FLOOR};

#[tokio::main]
async fn main() -> Result<(), String> {
    let usage = "usage: signature_shapes <texts.jsonl> <contrast.jsonl>";
    let texts = read_texts(&std::env::args().nth(1).ok_or(usage)?)?;
    let contrast_texts = read_texts(&std::env::args().nth(2).ok_or(usage)?)?;
    let embedder = continuum_genome::ai::embedding::resolve_recall_embedder_local().await;
    let started = std::time::Instant::now();
    let embedded = embedder.embed_many(&texts).await;
    let embed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let contrast = embedder.embed_many(&contrast_texts).await;
    let (mut embeddings, mut held_out) = (Vec::new(), Vec::new());
    for (i, e) in embedded.into_iter().enumerate() {
        if i % 5 == 4 { held_out.push(e) } else { embeddings.push(e) }
    }
    let calibrated = embedder.unrelated_null();
    println!("{}", serde_json::json!({ "embedder": embedder.id(), "texts": texts.len(), "embed_ms": embed_ms, "unrelated_null": calibrated })); // the report's output format
    let mut floors = vec![CURRENT_FLOOR];
    if let Some((mean, spread)) = calibrated {
        floors.extend([mean + 2.0 * spread, mean + 3.0 * spread, mean + 4.0 * spread]);
    }
    for floor in floors {
        let report = compare(&embeddings, &held_out, &contrast, floor);
        println!("{}", serde_json::to_string(&report).map_err(|e| e.to_string())?); // the report's output format
    }
    Ok(())
}

/// The texts of a dump: one JSON string per line.
fn read_texts(path: &str) -> Result<Vec<String>, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<String>(line).map_err(|e| format!("a line of {path} is not a JSON string: {e}"))) // the dump's file format
        .collect()
}
