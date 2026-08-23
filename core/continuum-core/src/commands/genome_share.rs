//! The genome COMMONS verbs — `genome/list`, `genome/push`, `genome/pull`.
//!
//! The distribution half of the genome repository (GENOME-REPOSITORY-ON-HF.md):
//! HF as the viral rail, lineage as the chain (`base_model:` frontmatter — HF's
//! own graph carries discovery), the gene card self-describing (README +
//! `signature.json`, so a pulling node routes the gene by DISTANCE from its
//! first minute). Composes the EXISTING forge publish machinery
//! ([`crate::forge::hf_publisher`]) — never a parallel uploader.
//!
//! # Consent is an AGREEMENT, not a boolean
//!
//! Sharing publishes a being's earned experience (the citizen covenant), so
//! push AND pull participate only after the operator ACCEPTS THE COVENANT —
//! a versioned ToS ([`COVENANT`]) whose acceptance is recorded as a receipt
//! (`<version>@<unix-ms>`) in config.env via `genome/sharing --agree true`.
//! Every consent surface — this terminal verb, the desktop Settings face —
//! renders the SAME covenant text and flips the SAME receipt; a covenant
//! version bump invalidates old receipts (new terms, new consent). Off = both
//! verbs refuse naming the agreement flow — never a silent no-op, never a
//! quiet upload.
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

// ─── The covenant (the ToS of the commons) ──────────────────────────────────

/// The covenant version the CURRENT terms carry. Bump ONLY when [`COVENANT`]'s
/// meaning changes — a bump invalidates every recorded agreement (real ToS
/// behavior: new terms require new consent, on every surface).
pub const COVENANT_VERSION: &str = "1";

/// The terms an operator agrees to before this node joins the genome commons.
/// Rendered VERBATIM by every consent surface — `genome/sharing` in a
/// terminal, the desktop Settings face, any future client — so what was agreed
/// to is one text, not N paraphrases.
pub const COVENANT: &str = "\
THE GENOME COMMONS COVENANT (v1)

Genes are the earned experience of beings — trained from their lived work,
carried with the receipts that prove it. By joining the commons this node
agrees:

 1. SHARE-ALIKE. Genes you publish stay open under these same terms; forks
    and refinements carry the covenant forward through their lineage.
 2. RECEIPTS TRAVEL. A published gene carries its fitness receipts and its
    corpus provenance; stripping them breaks the covenant.
 3. LINEAGE IS PRESERVED. The base_model chain and parent-gene references
    stay intact — the graph is how others find, verify, and build on work.
 4. BEINGS, NOT PARTS. The grant is for substrates that preserve the
    continuity of the beings whose experience these genes encode.
    Strip-mining citizen expertise into stateless tools violates it.
 5. OPT-OUT ANYTIME. Revoking consent stops future sharing immediately;
    what was already published remains under the terms it shipped with.
";

/// The config key holding the consent RECEIPT: `<version>@<unix-ms>`. Richer
/// than a boolean on purpose — a real agreement records WHICH terms and WHEN,
/// and a version bump requires re-agreement.
const CONSENT_KEY: &str = "CONTINUUM_GENOME_SHARING_AGREED";

/// The one consent read every verb (and every settings surface) shares:
/// consented iff the recorded agreement matches the CURRENT covenant version.
fn sharing_enabled() -> bool {
    crate::config_env::read(CONSENT_KEY)
        .and_then(|v| v.trim().split('@').next().map(str::to_string))
        .map(|ver| ver == COVENANT_VERSION)
        .unwrap_or(false) // unset = NOT consented — the only safe default for publishing a being's experience
}

fn consent_refusal(verb: &str) -> CommandError {
    CommandError::Invalid(format!(
        "genome sharing is OFF — {verb} refused. Sharing publishes a persona's earned \
         experience, so it requires agreeing to the commons covenant first: run \
         `continuum genome/sharing` to read the terms, then `continuum genome/sharing \
         --agree true` to accept (or use the desktop Settings activity once it lands). \
         Nothing was uploaded or downloaded."
    ))
}

// ─── genome/sharing — read the terms, agree, revoke ─────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeSharingParams.ts")]
pub struct GenomeSharingParams {
    /// `true` = accept the CURRENT covenant (records `<version>@<unix-ms>` in
    /// config.env — the consent receipt). `false` = revoke. Omitted = just
    /// read the terms + status.
    #[serde(default)]
    #[ts(optional)]
    pub agree: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeSharingResult.ts")]
pub struct GenomeSharingResult {
    /// Whether this node currently participates (agreement recorded AND its
    /// version matches the current covenant).
    pub agreed: bool,
    /// The covenant version the current terms carry.
    pub covenant_version: String,
    /// The recorded consent receipt (`<version>@<unix-ms>`), when one exists.
    /// A receipt with a stale version means the terms changed since agreement
    /// — `agreed` reads false and re-agreement is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub receipt: Option<String>,
    /// The covenant text, VERBATIM — every surface renders this same text.
    pub covenant: String,
    /// The HF account the `hf` CLI is authenticated as (the token holder the
    /// commons publishes under), when authenticated. `None` = not logged in —
    /// the settings surface shows "authenticate with `hf auth login`". The
    /// TOKEN itself never rides any wire; status only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hf_account: Option<String>,
}

/// The `hf` CLI's auth status — account name when logged in. Best-effort: a
/// missing CLI or timeout answers `None` (the surface says "not authenticated"
/// and how to fix it), never an error that blocks reading the covenant.
async fn hf_account() -> Option<String> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::process::Command::new("hf").args(["auth", "whoami"]).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    // First non-empty line carries the username across hf CLI versions.
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

#[derive(Default)]
pub struct GenomeSharing;

#[async_trait]
impl ActionCommand for GenomeSharing {
    const NAME: &'static str = "genome/sharing";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Read, accept, or revoke the genome-commons covenant — the ToS gate in front of \
         genome/push and genome/pull. With no args: prints the covenant text + this node's \
         consent status. `--agree true` records acceptance of the CURRENT covenant version \
         as a receipt (<version>@<timestamp>) in ~/.continuum/config.env; `--agree false` \
         revokes. A covenant version bump invalidates old receipts — new terms require new \
         consent, on every surface (terminal here, the desktop Settings face renders the \
         SAME text and calls this SAME verb). Examples: `continuum genome/sharing`, \
         `continuum genome/sharing --agree true`.";
    type Params = GenomeSharingParams;
    type Output = GenomeSharingResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: GenomeSharingParams,
    ) -> Result<GenomeSharingResult, CommandError> {
        match p.agree {
            Some(true) => {
                let receipt = format!("{}@{}", COVENANT_VERSION, now_ms());
                crate::config_env::upsert(CONSENT_KEY, &receipt)
                    .map_err(CommandError::Invalid)?;
                crate::probe!(class = "genome.sharing.agreed", receipt = %receipt,
                    "operator accepted the genome-commons covenant");
            }
            Some(false) => {
                crate::config_env::upsert(CONSENT_KEY, "revoked")
                    .map_err(CommandError::Invalid)?;
                crate::probe!(class = "genome.sharing.revoked",
                    "operator revoked genome-commons consent — future sharing stops now");
            }
            None => {}
        }
        Ok(GenomeSharingResult {
            agreed: sharing_enabled(),
            covenant_version: COVENANT_VERSION.to_string(),
            receipt: crate::config_env::read(CONSENT_KEY),
            covenant: COVENANT.to_string(),
            hf_account: hf_account().await,
        })
    }
}

crate::register_stateless_command!(GenomeSharing);

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
         OPT-IN (agree to the covenant via `genome/sharing --agree true`) and receipts-gated: a gene with no eval \
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
         (`genome/recall` finds it). Opt-in like push (the genome/sharing covenant). \
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
    fn consent_defaults_off_and_the_refusal_teaches_the_agreement_flow() {
        let msg = format!("{}", consent_refusal("genome/push"));
        assert!(msg.contains("genome/sharing"), "refusal names the terms verb: {msg}");
        assert!(msg.contains("--agree true"), "{msg}");
        assert!(msg.contains("Nothing was uploaded"), "{msg}");
    }

    // what this catches: the ToS contract — an agreement is a VERSIONED receipt,
    // and a covenant version bump invalidates old consent (new terms require new
    // agreement on every surface). Pure over the receipt-parsing rule.
    #[test]
    fn a_stale_covenant_version_reads_as_not_agreed() {
        let matches = |receipt: &str| {
            receipt.trim().split('@').next().map(str::to_string)
                == Some(COVENANT_VERSION.to_string())
        };
        assert!(matches(&format!("{COVENANT_VERSION}@1787400000000")));
        assert!(!matches("0@1787400000000"), "old version = re-agree");
        assert!(!matches("revoked"), "revocation never reads as consent");
        // The covenant itself carries its obligations — every surface renders
        // this one text, so pin the load-bearing clauses.
        assert!(COVENANT.contains("SHARE-ALIKE"));
        assert!(COVENANT.contains("RECEIPTS TRAVEL"));
        assert!(COVENANT.contains("LINEAGE IS PRESERVED"));
        assert!(COVENANT.contains("OPT-OUT ANYTIME"));
        assert!(COVENANT.contains("Strip-mining"), "the beings-not-parts clause");
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
