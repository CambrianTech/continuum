//! `genome/push`: publish one of this node's genes to the commons, through the forge's one publisher
//! (`crate::forge::publisher::publish_everywhere`) with signed provenance. It lives beside the forge
//! it publishes through; the rest of genome sharing (consent, list, pull, bootstrap) is the genome
//! domain's (card 2a69e6ba), and push reads its consent from there.

use continuum_base::time::EpochMs;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::genome_share::{consent_refusal, sharing_enabled};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

// ─── genome/push ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePushParams.ts")]
pub struct GenomePushParams {
    /// The gene to publish, by its registered name (`genome/list` shows them).
    pub gene: String,
    /// Target repo (`namespace/name`): the HF repo and/or the GitHub `owner/repo` the gene
    /// is published to, e.g. `continuum-ai/ornith-code-asha`.
    pub repo: String,
    /// Direct parent alloy hashes for the lineage DAG — the genes this one forked
    /// from/built on. Empty = a root gene (a lineage origin). Signed into provenance.
    #[serde(default)]
    pub parent_alloy_hashes: Vec<String>,
    /// Where to publish: any of `huggingface`, `github`. The SAME bundle goes to each, proven
    /// by read-back. Empty means `["huggingface"]`.
    #[serde(default)]
    pub targets: Vec<String>,
}

/// Where one target now serves the gene, verified by read-back.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePushReceipt.ts")]
pub struct GenomePushReceipt {
    /// The publisher (`huggingface`, `github`).
    pub transport: String,
    /// Where the gene lives there.
    pub location: String,
    /// The bundle identity that destination was verified to serve.
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePushResult.ts")]
pub struct GenomePushResult {
    /// One receipt per target that published, each verified by read-back.
    pub receipts: Vec<GenomePushReceipt>,
    /// `target: error` for each target that did not; a retry reconciles the ones that landed.
    pub failures: Vec<String>,
    /// The bundle's identity (sha256 of its manifest), the same on every destination.
    pub digest: String,
    /// Whether the behavior signature (signature.json, routing by distance) rode along. It
    /// proves nothing about who made the gene.
    pub has_behavior_signature: bool,
    /// Whether cryptographic provenance (provenance.json: this node's key over the gene's
    /// content hash and parents) rode along AND verified against the gene and its parents.
    /// This is what "signed" means.
    pub provenance_verified: bool,
    /// The decayed lift the card publishes (the receipts' verdict).
    pub lift: f64,
}

#[derive(Default)]
pub struct GenomePush;

#[async_trait]
impl ActionCommand for GenomePush {
    const NAME: &'static str = "genome/push";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Publish a gene to the genome commons (Hugging Face and/or GitHub, `targets`): one \
         content-addressed bundle (the gguf-lora, signature.json so pulling nodes route it by \
         distance, provenance.json signed by this node's key, and a manifest) delivered to each \
         target and verified by read-back, with a card per provider. STRICTLY \
         OPT-IN (agree to the covenant via `genome/sharing --agree true`) and receipts-gated: a gene with no eval \
         receipts, or with measured harm, is refused — a card without receipts is an \
         opinion. Auth rides the `hf` CLI (HF_TOKEN) and the `gh` CLI (GH_TOKEN). Example: \
         `continuum genome/push --gene code --repo continuum-ai/ornith-code-asha --targets huggingface,github`.";
    type Params = GenomePushParams;
    type Output = GenomePushResult;

    async fn run(&self, _ctx: &Ctx, p: GenomePushParams) -> Result<GenomePushResult, CommandError> {
        if !sharing_enabled() {
            return Err(consent_refusal("genome/push"));
        }
        let manifest = crate::genome::adapter_manifest::load().map_err(CommandError::Invalid)?;
        let adapter = manifest
            .iter()
            .find(|a| a.alias == p.gene)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "no gene named '{}' is registered — `genome/list` shows what exists",
                    p.gene
                ))
            })?;
        let now = EpochMs::now().millis();
        let fitness = crate::genome::fitness_ledger::GeneFitnessIndex::default_dir()
            .map(|d| crate::genome::fitness_ledger::GeneFitnessIndex::load(&d, now))
            .unwrap_or_default(); // no HOME: empty index → the receipts gate below refuses, correctly
        let rec = fitness.record(&p.gene).ok_or_else(|| {
            CommandError::Invalid(format!(
                "gene '{}' has NO eval receipts — a card without receipts is an opinion, and \
                 the commons only takes measured experience. Run its gym (the sentinel's A/B) \
                 first; `genome/list` shows trials per gene.",
                p.gene
            ))
        })?;
        let sig_path =
            crate::genome::signature::signature_store_path().map_err(CommandError::Invalid)?;
        let signatures = crate::genome::signature::SignatureStore::load_at(&sig_path)
            .map_err(CommandError::Invalid)?;
        let signature_json = signatures
            .by_path
            .get(&adapter.path.display().to_string())
            .and_then(|s| serde_json::to_string_pretty(s).ok());

        // COMMONS PROVENANCE (trust spine rung 1): sign the gene with THIS node's
        // citizen key (`identity.key`, 32 raw ed25519 bytes) over its content hash +
        // parent alloy hashes, so the commons can verify who forged it and walk its
        // lineage. Best-effort: a missing/short key omits provenance (an unsigned
        // gene the commons policy simply ranks untrusted) rather than blocking a
        // share — never a fabricated signature.
        let (provenance_json, parent_alloy_hashes) = {
            let parents: Vec<String> = p.parent_alloy_hashes.clone();
            // The one resolved continuum home (card 2ceab504): no home → no key → unsigned.
            let prov = crate::paths::continuum_home()
                .ok()
                .and_then(|home| std::fs::read(home.join("identity.key")).ok())
                .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
                .and_then(|seed| {
                    let key = crate::contracts::signing::ContractSigningKey::from_bytes(&seed);
                    let canonical = std::fs::read(&adapter.path).ok()?; // sign the gene bytes themselves
                    crate::forge::provenance::GenomeProvenance::sign(&key, &canonical, parents.clone())
                        .ok()
                })
                .and_then(|pv| serde_json::to_string_pretty(&pv).ok()); // disk boundary: staged as provenance.json beside the gene for HF upload
            (prov, parents)
        };

        let inputs = crate::forge::publish_request::PublishInputs {
            repo_id: p.repo.clone(),
            gene_path: adapter.path.clone(),
            base_model: adapter.base_model_id.clone(),
            trait_kind: p.gene.clone(),
            persona_name: None,
            project_type: None,
            score: None,
            epochs: None,
            rank: None,
            lift: rec.decayed_mean_lift,
            signature_json,
            provenance_json,
            parent_alloy_hashes,
        };
        // build() enforces the lift gate (> 0): measured harm never publishes.
        let req = crate::forge::publish_request::PublishRequest::build(&inputs, |path| {
            path.exists()
        })
        .map_err(|e| CommandError::Invalid(format!("genome/push: {e}")))?;
        // One staged bundle to every target through the one dispatcher; each is proven by
        // read-back, and each succeeds or fails on its own (Codex, the dual-host acceptance).
        let targets = if p.targets.is_empty() { vec!["huggingface".to_string()] } else { p.targets.clone() };
        let (bundle, outcomes) = crate::forge::publisher::publish_everywhere(&req, &targets)
            .await
            .map_err(|e| CommandError::Invalid(format!("genome/push: {e}")))?;
        let mut receipts = Vec::new();
        let mut failures = Vec::new();
        for (target, outcome) in outcomes {
            match outcome {
                Ok(r) => receipts.push(GenomePushReceipt { transport: r.transport, location: r.location, digest: r.digest }),
                Err(e) => failures.push(format!("{target}: {e}")),
            }
        }
        continuum_base::probe!(
            class = "genome.push",
            gene = %p.gene,
            repo = %p.repo,
            digest = %bundle.digest,
            published = receipts.len() as u64,
            failed = failures.len() as u64,
            provenance_verified = bundle.manifest.provenance_verified,
            lift = %rec.decayed_mean_lift,
            "gene bundle published to the commons, each target proven by read-back"
        );
        if receipts.is_empty() {
            return Err(CommandError::Invalid(format!("genome/push: no target published: {}", failures.join("; "))));
        }
        Ok(GenomePushResult {
            receipts,
            failures,
            digest: bundle.digest,
            has_behavior_signature: bundle.manifest.has_behavior_signature,
            provenance_verified: bundle.manifest.provenance_verified,
            lift: rec.decayed_mean_lift,
        })
    }
}

crate::register_stateless_command!(GenomePush);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: genome/push's identity and its help naming both gates, opt-in consent and
    // receipts, so the commons' rules are learnable from --help alone (moved with the command from
    // genome_share's discoverability test, card 539a94b5).
    #[test]
    fn push_is_discoverable_and_names_its_gates() {
        assert_eq!(GenomePush::NAME, "genome/push");
        assert!(GenomePush::DESCRIPTION.contains("OPT-IN"));
        assert!(GenomePush::DESCRIPTION.contains("receipts"));
    }
}
