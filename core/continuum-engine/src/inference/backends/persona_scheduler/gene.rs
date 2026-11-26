//! One gene a turn runs under: its id, the adapter file it was loaded from, and its scale. Her blocks are
//! computed under a set of them, so the set is kept with her blocks when she stops: a mind resumed from
//! disk can load exactly the adapters her blocks were computed under, and learn, or be brought to a
//! lesson's context, before any turn of hers hands them back (the M5, 2026-11-26: a resumed resident's
//! every lesson was refused, "her genes were not served", until she next took a turn).

use std::path::PathBuf;

/// One gene: which, from where, how strongly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gene {
    pub id: String,
    pub path: PathBuf,
    pub scale: f32,
}

/// The identity two gene sets compare by: each gene's id and scale, in one order. The file a gene was
/// loaded from is where it lives, not what it is.
pub(super) fn signature<'a>(genes: impl Iterator<Item = &'a Gene>) -> String {
    let mut parts: Vec<String> = genes.map(|gene| format!("{}:{}", gene.id, gene.scale)).collect();
    parts.sort_unstable();
    parts.join(",")
}
