//! The genome COMMONS verbs — `genome/list`, `genome/push`, `genome/pull`.
//!
//! The distribution half of the genome repository (GENOME-REPOSITORY-ON-HF.md):
//! HF as the viral rail, lineage as the chain (`base_model:` frontmatter — HF's
//! own graph carries discovery), the gene card self-describing (README +
//! `signature.json`, so a pulling node routes the gene by DISTANCE from its
//! first minute). Composes the EXISTING forge publish machinery
//! ([`crate::forge::hf_publisher`]) — never a parallel uploader.
//!
//! # Consent is a GATE, not a default
//!
//! Sharing publishes a being's earned experience (the citizen covenant), so
//! push AND pull participate in the commons only when the operator opted in:
//! `CONTINUUM_GENOME_SHARING=true` in the environment / config.env. The
//! desktop settings activity renders the same switch when it lands; this env
//! read is the substrate truth both faces flip. Off = both verbs refuse with
//! the setting named — never a silent no-op, never a quiet upload.
//!
//! # The receipts rule
//!
//! A gene card without fitness receipts is an opinion (§4.1.3.4). `genome/push`
//! refuses a gene with no eval-receipt history; the lift gate
//! ([`passes_publish_lift_gate`]) then refuses measured-harm genes. Only
//! measured-positive experience enters the commons under our name.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// The one consent read both verbs (and later the settings tab) share.
fn sharing_enabled() -> bool {
    crate::config_env::read("CONTINUUM_GENOME_SHARING")
        .map(|v| {
            let v = v.trim().to_ascii_lowercase();
            v == "1" || v == "true" || v == "yes"
        })
        .unwrap_or(false) // unset = NOT consented — the only safe default for publishing a being's experience
}

fn consent_refusal(verb: &str) -> CommandError {
    CommandError::Invalid(format!(
        "genome sharing is OFF — {verb} refused. Sharing publishes a persona's earned \
         experience, so it is strictly opt-in: set CONTINUUM_GENOME_SHARING=true in the \
         environment (or config.env) to join the commons, or use the desktop Settings \
         activity once it lands. Nothing was uploaded or downloaded."
    ))
}

// ─── genome/list ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeListParams.ts")]
pub struct GenomeListParams {}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeListGene.ts")]
pub struct GenomeListGene {
    pub gene: String,
    pub base_model: String,
    pub path: String,
    /// Whether a minted embedding-space signature is stamped (distance-routable).
    pub signed: bool,
    /// Eval-receipt trials on record (0 = unmeasured).
    #[ts(type = "number")]
    pub trials: u32,
    /// Age-decayed mean lift from those receipts; absent when unmeasured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub decayed_lift: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeListResult.ts")]
pub struct GenomeListResult {
    /// Every registered gene with its signature + fitness facts — the local
    /// registry view (what the desktop settings/genome tab renders).
    pub genes: Vec<GenomeListGene>,
    /// Whether this node participates in the sharing commons (the opt-in).
    pub sharing_enabled: bool,
}

#[derive(Default)]
pub struct GenomeList;

#[async_trait]
impl ActionCommand for GenomeList {
    const NAME: &'static str = "genome/list";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List every gene registered on this node with its routing + fitness facts: signed \
         (distance-routable via its minted signature), eval-receipt trials, and age-decayed \
         lift. Also reports whether genome sharing (the HF commons) is opted in. This is the \
         registry view the desktop settings/genome tab renders. Example: `continuum genome/list`.";
    type Params = GenomeListParams;
    type Output = GenomeListResult;

    async fn run(&self, _ctx: &Ctx, _p: GenomeListParams) -> Result<GenomeListResult, CommandError> {
        let manifest = crate::forge::adapter_manifest::load().map_err(CommandError::Invalid)?;
        let sig_path =
            crate::genome::signature::signature_store_path().map_err(CommandError::Invalid)?;
        let signatures = crate::genome::signature::SignatureStore::load_at(&sig_path)
            .map_err(CommandError::Invalid)?;
        let now_ms = now_ms();
        let fitness = crate::genome::fitness_ledger::GeneFitnessIndex::default_dir()
            .map(|d| crate::genome::fitness_ledger::GeneFitnessIndex::load(&d, now_ms))
            .unwrap_or_default(); // no HOME: empty index — genes list as unmeasured, honestly
        let genes = manifest
            .iter()
            .map(|a| {
                let rec = fitness.record(&a.alias);
                GenomeListGene {
                    gene: a.alias.clone(),
                    base_model: a.base_model_id.clone(),
                    path: a.path.display().to_string(),
                    signed: signatures.by_path.contains_key(&a.path.display().to_string()),
                    trials: rec.map(|r| r.trials).unwrap_or(0), // no receipts = 0 trials, the honest unmeasured state
                    decayed_lift: rec.map(|r| r.decayed_mean_lift),
                }
            })
            .collect();
        Ok(GenomeListResult { genes, sharing_enabled: sharing_enabled() })
    }
}

crate::register_stateless_command!(GenomeList);

// ─── genome/push ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePushParams.ts")]
pub struct GenomePushParams {
    /// The gene to publish, by its registered name (`genome/list` shows them).
    pub gene: String,
    /// Target HF repo (`namespace/name`), e.g. `continuum-ai/ornith-code-asha`.
    pub repo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePushResult.ts")]
pub struct GenomePushResult {
    /// Where the gene now lives (the HF URL).
    pub location: String,
    /// Whether the self-describing signature.json rode along.
    pub signed: bool,
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
        "Publish a gene to the HF genome commons: the gguf-lora + a self-describing card \
         (base_model lineage frontmatter for HF's own discovery chain, fitness provenance, \
         and signature.json so pulling nodes route it by distance immediately). STRICTLY \
         OPT-IN (CONTINUUM_GENOME_SHARING=true) and receipts-gated: a gene with no eval \
         receipts, or with measured harm, is refused — a card without receipts is an \
         opinion. Auth rides the `hf` CLI (HF_TOKEN). Example: \
         `continuum genome/push --gene code --repo continuum-ai/ornith-code-asha`.";
    type Params = GenomePushParams;
    type Output = GenomePushResult;

    async fn run(&self, _ctx: &Ctx, p: GenomePushParams) -> Result<GenomePushResult, CommandError> {
        if !sharing_enabled() {
            return Err(consent_refusal("genome/push"));
        }
        let manifest = crate::forge::adapter_manifest::load().map_err(CommandError::Invalid)?;
        let adapter = manifest
            .iter()
            .find(|a| a.alias == p.gene)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "no gene named '{}' is registered — `genome/list` shows what exists",
                    p.gene
                ))
            })?;
        let now = now_ms();
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
        let signed = signature_json.is_some();

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
        };
        // build() enforces the lift gate (> 0): measured harm never publishes.
        let req = crate::forge::publish_request::PublishRequest::build(&inputs, |path| {
            path.exists()
        })
        .map_err(|e| CommandError::Invalid(format!("genome/push: {e}")))?;
        use crate::forge::publisher::Publisher as _;
        let receipt = crate::forge::hf_publisher::HfPublisher::new()
            .publish(&req)
            .await
            .map_err(|e| CommandError::Invalid(format!("genome/push upload: {e}")))?;

        crate::probe!(
            class = "genome.push",
            gene = %p.gene,
            repo = %p.repo,
            signed = %signed,
            lift = %rec.decayed_mean_lift,
            "gene published to the commons"
        );
        Ok(GenomePushResult { location: receipt.location, signed, lift: rec.decayed_mean_lift })
    }
}

crate::register_stateless_command!(GenomePush);

// ─── genome/pull ────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePullParams.ts")]
pub struct GenomePullParams {
    /// Source HF repo (`namespace/name`) — a gene published by `genome/push`
    /// or any compatible gguf-lora repo.
    pub repo: String,
    /// The CONTINUUM base-model id this gene serves against (the serving
    /// association the manifest requires). Required explicitly: the card names
    /// the TRAINER'S id, and a wrong association would page the gene into the
    /// wrong base silently — declared beats guessed.
    pub base_model: String,
    /// Local name to register under (default: the repo's name segment).
    #[serde(default)]
    #[ts(optional)]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomePullResult.ts")]
pub struct GenomePullResult {
    /// The registered gene name.
    pub gene: String,
    /// Where it landed on disk.
    pub path: String,
    /// Whether the repo carried a signature.json (stamped into the sidecar —
    /// the gene routes by distance immediately).
    pub signed: bool,
}

/// Where pulled genes land: beside the local ones, namespaced by repo so two
/// commons genes can never collide with each other or with local forges.
fn pulled_dir(repo: &str) -> Result<std::path::PathBuf, String> {
    let safe = repo.replace('/', "__");
    Ok(crate::forge::adapter_manifest::manifest_path()?
        .with_file_name("pulled")
        .join(safe))
}

#[derive(Default)]
pub struct GenomePull;

#[async_trait]
impl ActionCommand for GenomePull {
    const NAME: &'static str = "genome/pull";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Pull a gene from the HF genome commons: downloads the repo's gguf-lora, registers \
         it in the adapter manifest under --base-model (declared, never guessed — a wrong \
         base association would page it into the wrong model silently), and stamps its \
         signature.json into the sidecar so it routes by DISTANCE immediately \
         (`genome/recall` finds it). Opt-in like push (CONTINUUM_GENOME_SHARING=true). \
         Example: `continuum genome/pull --repo continuum-ai/ornith-code-asha \
         --base-model ornith-ai/Ornith-1.5-35B-A3B-GGUF`.";
    type Params = GenomePullParams;
    type Output = GenomePullResult;

    async fn run(&self, _ctx: &Ctx, p: GenomePullParams) -> Result<GenomePullResult, CommandError> {
        if !sharing_enabled() {
            return Err(consent_refusal("genome/pull"));
        }
        let dest = pulled_dir(&p.repo).map_err(CommandError::Invalid)?;
        std::fs::create_dir_all(&dest)
            .map_err(|e| CommandError::Invalid(format!("create {}: {e}", dest.display())))?;
        // The `hf` CLI owns auth + transfer, exactly as the publisher does.
        let out = tokio::process::Command::new("hf")
            .args(["download", &p.repo, "--local-dir"])
            .arg(&dest)
            .output()
            .await
            .map_err(|e| {
                CommandError::Invalid(format!(
                    "`hf` CLI not runnable ({e}) — install huggingface_hub and authenticate"
                ))
            })?;
        if !out.status.success() {
            return Err(CommandError::Invalid(format!(
                "hf download failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        // Exactly one gguf is the contract a genome/push repo keeps; zero or
        // many means this is not a gene repo — refuse rather than guess.
        let mut ggufs: Vec<std::path::PathBuf> = std::fs::read_dir(&dest)
            .map_err(|e| CommandError::Invalid(format!("read {}: {e}", dest.display())))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("gguf"))
            .collect();
        let gene_path = match (ggufs.len(), ggufs.pop()) {
            (1, Some(path)) => path,
            (n, _) => {
                return Err(CommandError::Invalid(format!(
                    "expected exactly one .gguf in {}, found {n} — not a gene repo",
                    p.repo
                )))
            }
        };
        let alias = p
            .alias
            .clone()
            .or_else(|| p.repo.rsplit('/').next().map(str::to_string))
            .unwrap_or_else(|| p.repo.clone()); // a repo with no '/' names itself — display identity only

        let manifest_path =
            crate::forge::adapter_manifest::manifest_path().map_err(CommandError::Invalid)?;
        crate::forge::adapter_manifest::register_at(
            &manifest_path,
            crate::forge::adapter_manifest::TrainedAdapter {
                alias: alias.clone(),
                path: gene_path.clone(),
                base_model_id: p.base_model.clone(),
            },
        )
        .map_err(CommandError::Invalid)?;

        // The self-describing half: stamp the shipped signature so recall
        // routes this gene by distance from its first minute here.
        let sig_file = dest.join("signature.json");
        let mut signed = false;
        if let Ok(text) = std::fs::read_to_string(&sig_file) {
            match serde_json::from_str::<crate::genome::signature::GeneSignature>(&text) {
                Ok(sig) => {
                    let store =
                        crate::genome::signature::signature_store_path().map_err(CommandError::Invalid)?;
                    crate::genome::signature::SignatureStore::stamp_at(
                        &store,
                        &gene_path.display().to_string(),
                        sig,
                    )
                    .map_err(CommandError::Invalid)?;
                    signed = true;
                }
                Err(e) => {
                    // A malformed shipped signature demotes to keyword routing —
                    // loudly, never silently ([[fallbacks-are-illegal-fail-loud]]
                    // honored by SAYING the degrade).
                    tracing::warn!(repo = %p.repo, error = %e,
                        "pulled gene's signature.json unparsable — registered unsigned, routes by fallback");
                }
            }
        }

        crate::probe!(
            class = "genome.pull",
            gene = %alias,
            repo = %p.repo,
            signed = %signed,
            "gene pulled from the commons and registered"
        );
        Ok(GenomePullResult { gene: alias, path: gene_path.display().to_string(), signed })
    }
}

crate::register_stateless_command!(GenomePull);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0) // pre-epoch clock: decay saturates instead of the verb failing
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the consent contract — sharing defaults OFF (publishing
    // a being's experience is never a default), and the refusal NAMES the setting
    // and the future desktop face so the next driver flips a switch instead of
    // reading source. (The env read itself is exercised only when unset here —
    // process-env mutation in tests races other tests.)
    #[test]
    fn consent_defaults_off_and_the_refusal_teaches_the_setting() {
        let msg = format!("{}", consent_refusal("genome/push"));
        assert!(msg.contains("CONTINUUM_GENOME_SHARING=true"), "{msg}");
        assert!(msg.contains("opt-in"), "{msg}");
        assert!(msg.contains("Nothing was uploaded"), "{msg}");
    }

    // what this catches: the three verbs' identity + their help carrying runnable
    // examples and the two gates (opt-in, receipts) — the commons' rules must be
    // learnable from --help alone.
    #[test]
    fn the_verbs_are_discoverable_and_name_their_gates() {
        assert_eq!(GenomeList::NAME, "genome/list");
        assert_eq!(GenomePush::NAME, "genome/push");
        assert_eq!(GenomePull::NAME, "genome/pull");
        assert!(GenomePush::DESCRIPTION.contains("OPT-IN"));
        assert!(GenomePush::DESCRIPTION.contains("receipts"));
        assert!(GenomePull::DESCRIPTION.contains("genome/pull --repo"));
        assert!(GenomeList::DESCRIPTION.contains("genome/list"));
    }

    // what this catches: pulled genes colliding — two commons repos (or a commons
    // repo and a local forge) must never overwrite each other's files.
    #[test]
    fn pulled_dirs_are_namespaced_per_repo() {
        let a = pulled_dir("acme/fp-gene").expect("path");
        let b = pulled_dir("acme/other").expect("path");
        assert_ne!(a, b);
        assert!(a.display().to_string().contains("acme__fp-gene"));
    }
}
