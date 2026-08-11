//! `cognition/<verb>` — the cognition command family (typed, self-routing).
//!
//! These are the persona cognitive-pipeline commands migrated off the legacy
//! `CognitionModule::handle_command` match onto the typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) path. The gating/generation
//! decisions here call free functions in [`crate::cognition`] (no `CognitionModule`
//! state), so each is a **stateless** `action_command!` unit struct: `inventory`
//! publishes both the descriptor and the runtime object, no module `commands()`
//! ceremony. Stateful cognition commands (per-persona genome, inbox, engram store)
//! migrate as dep-holding families in later slices.
//!
//! Everything here is `access: Internal` — substrate cognition IPC the host drives,
//! registered and grid-routable but not remote-callable persona toolbelt verbs.

use std::sync::Arc;

use crate::modules::cognition::CognitionState;
use crate::runtime::{CommandExecutor, LateBound};
use crate::sdk_codegen::DynCommand;

pub mod admit_inbox_message;
pub mod dream_now;
pub mod forget_context;
pub mod redact_memory;
pub mod observe;
pub mod cache_message;
pub mod check_adequacy;
pub mod check_content_dedup;
pub mod check_redundancy;
pub mod classify_domain;
pub mod configure_rate_limiter;
pub mod create_engine;
pub mod embed_tools;
pub mod enqueue_message;
pub mod full_evaluate;
pub mod generate_recipe;
pub mod generate_response;
pub mod genome_activate_skill;
pub mod genome_coverage_report;
pub mod genome_evict_under_pressure;
pub mod genome_record_activity;
pub mod genome_state;
pub mod genome_sync;
pub mod get_state;
pub mod gpu_budget;
pub mod has_evaluated;
pub mod inbox_create;
pub mod inbox_drain_frame;
pub mod mark_evaluated;
pub mod plan_turn_batch;
pub mod rate_proposals;
pub mod recall_engrams;
pub mod record_content;
pub mod respond;
pub mod register_domain_keywords;
pub mod score_interaction;
pub mod select_model;
pub mod semantic_search_tools;
pub mod set_sleep_mode;
pub mod should_respond;
pub mod sync_adapters;
pub mod sync_domain_classifier;
pub mod track_response;
pub mod validate_response_decision;
pub mod vision_describe;

use admit_inbox_message::AdmitInboxMessage;
use dream_now::DreamNow;
use forget_context::ForgetContext;
use redact_memory::RedactMemory;
use cache_message::CacheMessage;
use check_content_dedup::CheckContentDedup;
use classify_domain::ClassifyDomain;
use configure_rate_limiter::ConfigureRateLimiter;
use create_engine::CreateEngine;
use enqueue_message::EnqueueMessage;
use full_evaluate::FullEvaluate;
use genome_activate_skill::GenomeActivateSkill;
use genome_coverage_report::GenomeCoverageReport;
use genome_evict_under_pressure::GenomeEvictUnderPressure;
use genome_record_activity::GenomeRecordActivity;
use genome_state::GenomeState;
use genome_sync::GenomeSync;
use get_state::GetState;
use gpu_budget::GpuBudget;
use has_evaluated::HasEvaluated;
use inbox_create::InboxCreate;
use inbox_drain_frame::InboxDrainFrame;
use mark_evaluated::MarkEvaluated;
use recall_engrams::RecallEngrams;
use record_content::RecordContent;
use respond::Respond;
use register_domain_keywords::RegisterDomainKeywords;
use select_model::SelectModel;
use set_sleep_mode::SetSleepMode;
use sync_adapters::SyncAdapters;
use sync_domain_classifier::SyncDomainClassifier;
use track_response::TrackResponse;
use vision_describe::VisionDescribe;

/// The dep-holding `cognition/*` command objects. Most capture the module's shared
/// [`CognitionState`]; [`VisionDescribe`] instead captures the module's shared late-bound
/// [`CommandExecutor`] slot (it re-enters the bus to run `ai/generate`), same as the
/// `chat/*` family. Called from
/// [`CognitionModule::commands`](crate::modules::cognition::CognitionModule) so they
/// reach `command_registry()`, the persona tool surface, the ACL, codegen, and `uu`.
///
/// The stateless oxidizer commands ([`should_respond`], [`check_redundancy`],
/// [`generate_response`], [`embed_tools`], [`semantic_search_tools`],
/// [`validate_response_decision`], [`score_interaction`], [`check_adequacy`],
/// [`plan_turn_batch`], [`rate_proposals`], [`generate_recipe`]) hold no module state,
/// self-route via `inventory`, and are NOT listed here.
pub fn command_objects(
    state: Arc<CognitionState>,
    executor_slot: Arc<LateBound<CommandExecutor>>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(CacheMessage {
            state: state.clone(),
        }),
        Arc::new(CheckContentDedup {
            state: state.clone(),
        }),
        Arc::new(RecordContent {
            state: state.clone(),
        }),
        Arc::new(HasEvaluated {
            state: state.clone(),
        }),
        Arc::new(MarkEvaluated {
            state: state.clone(),
        }),
        Arc::new(TrackResponse {
            state: state.clone(),
        }),
        Arc::new(GenomeActivateSkill {
            state: state.clone(),
        }),
        Arc::new(GenomeSync {
            state: state.clone(),
        }),
        Arc::new(GenomeState {
            state: state.clone(),
        }),
        Arc::new(GenomeEvictUnderPressure {
            state: state.clone(),
        }),
        Arc::new(GenomeRecordActivity {
            state: state.clone(),
        }),
        Arc::new(GenomeCoverageReport {
            state: state.clone(),
        }),
        Arc::new(ClassifyDomain {
            state: state.clone(),
        }),
        Arc::new(SyncDomainClassifier {
            state: state.clone(),
        }),
        Arc::new(RegisterDomainKeywords {
            state: state.clone(),
        }),
        Arc::new(SelectModel {
            state: state.clone(),
        }),
        Arc::new(SyncAdapters {
            state: state.clone(),
        }),
        Arc::new(SetSleepMode {
            state: state.clone(),
        }),
        Arc::new(ConfigureRateLimiter {
            state: state.clone(),
        }),
        Arc::new(CreateEngine {
            state: state.clone(),
        }),
        Arc::new(InboxCreate {
            state: state.clone(),
        }),
        Arc::new(EnqueueMessage {
            state: state.clone(),
        }),
        Arc::new(FullEvaluate {
            state: state.clone(),
        }),
        Arc::new(GetState {
            state: state.clone(),
        }),
        Arc::new(GpuBudget {
            state: state.clone(),
        }),
        Arc::new(AdmitInboxMessage {
            state: state.clone(),
        }),
        Arc::new(DreamNow {
            state: state.clone(),
        }),
        Arc::new(ForgetContext {
            state: state.clone(),
        }),
        Arc::new(RedactMemory {
            state: state.clone(),
        }),
        Arc::new(RecallEngrams {
            state: state.clone(),
        }),
        Arc::new(Respond {
            state: state.clone(),
        }),
        Arc::new(InboxDrainFrame { state }),
        Arc::new(VisionDescribe { executor_slot }),
    ]
}
