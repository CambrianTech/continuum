//! What core hands the work module (`continuum_mind::work`): the benchmark adapter's view of
//! a submission, and the four core-owned commands that register beside the work verbs.
//! One constructor, so the live server and the test kernel build the same module.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use crate::ai::embedding::EmbeddingProvider;
use crate::persona::PersonaAircRuntimeRegistry;
use crate::sdk_codegen::{CommandError, DynCommand};

use super::work::{BenchmarkSubmissionAdapter, WorkModule};

/// Core's implementation of the seam: the benchmark commands answer.
pub struct CoreBenchmarkSubmission;

#[async_trait]
impl BenchmarkSubmissionAdapter for CoreBenchmarkSubmission {
    async fn base_commit(&self, instance: &str) -> Result<Option<airc_work::GitObjectId>, CommandError> {
        crate::commands::benchmark::swe_base_commit_for(instance)
            .await
            .map(|sha| {
                airc_work::GitObjectId::new(&sha).map_err(|e| {
                    CommandError::Internal(format!("dataset base commit {sha:?} for {instance} is not a git object id: {e}"))
                })
            })
            .transpose()
    }

    fn candidate_diff(&self, workspace: &Path, base: Option<&airc_work::GitObjectId>) -> Result<String, CommandError> {
        crate::commands::benchmark::workspace_candidate_diff_from(&workspace.to_string_lossy(), base.map(|b| b.as_str()))
    }

    async fn grade_hand_in(
        &self,
        airc: Arc<airc_lib::Airc>,
        room: airc_lib::RoomId,
        card_title: String,
        instance: String,
        patch: airc_blobs::MediaRef,
    ) {
        super::benchmark_grade::grade_submission(airc, room.as_uuid(), card_title, instance, patch).await;
    }
}

/// The commands core owns that ride on the work module (board-adjacent, built with the
/// same registry).
fn extra_commands(registry: &PersonaAircRuntimeRegistry) -> Vec<Arc<dyn DynCommand>> {
    vec![
        // benchmark/import: task + oracle only, projected into card rows (no side effects).
        Arc::new(crate::commands::benchmark_import::BenchmarkImport),
        // coursework/import: the coursework/round recipe's card source (ONE-RESIDENT §10.4),
        // the same family as benchmark/import.
        Arc::new(crate::commands::benchmark_import::CourseworkImport),
        Arc::new(crate::commands::presence_directory::PresenceDirectory { registry: registry.clone() }),
    ]
}

/// The work module as core serves it.
pub fn work_module(registry: PersonaAircRuntimeRegistry, embedder: Arc<dyn EmbeddingProvider>) -> WorkModule {
    let extras = extra_commands(&registry);
    WorkModule::new(registry, embedder, Arc::new(CoreBenchmarkSubmission), extras)
}
