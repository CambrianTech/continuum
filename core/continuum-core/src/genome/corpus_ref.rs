//! A pointer to a corpus by name, content hash, size and source: the identity a gene's
//! signature and a forge recipe's calibration both name. Training data is the genome's, so
//! the type lives here; the forge's recipe carries one.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Pointer to the calibration corpus used for the importance profile +
/// (eventual) compensation LoRA. Held-out from `evaluation_benchmarks`.
///
/// Bytes don't live in Continuum's ORM (corpora can be MB-GB). The
/// recipe carries a pointer; the bytes live in HF datasets, foundry-
/// node-local storage, or wherever the `source_url` resolves.
///
/// `content_hash` uses the canonical `"sha256:<hex>"` format that
/// matches `persona::admission` content_hash on the engram side
/// (consensus position #8 from the design review). Cross-domain
/// consistency: any two subsystems comparing hashes can do
/// string-equality without normalization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/CorpusRef.ts")]
pub struct CorpusRef {
    /// Human-readable corpus name (e.g., "wikitext-103-v1").
    pub name: String,
    /// SHA-256 of the canonical corpus contents in `"sha256:<hex>"` form.
    /// Tamper-detection anchor + cross-domain equality with admission's
    /// content_hash convention.
    pub content_hash: String,
    /// Size in bytes (informational; helps the foundry pre-flight storage).
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// Where the bytes live (HF dataset id, file:// URL, etc.). Optional
    /// because some corpora are foundry-node-local with no shareable URL.
    #[ts(optional)]
    pub source_url: Option<String>,
}
