//! The benchmark adapter's view of a submission: the three things `work/submit` needs that
//! only a benchmark knows (an instance's dataset base commit, the candidate diff in the
//! grader's exact form, and grading a hand-in). The work verbs hold this seam and name no
//! benchmark concept; core implements it and injects it at construction, like the embedder.

use std::sync::Arc;

use async_trait::async_trait;

use crate::sdk_codegen::CommandError;

#[async_trait]
pub trait BenchmarkSubmissionAdapter: Send + Sync {
    /// The base commit a benchmark instance's checkout stands on; `None` when no dataset
    /// names it (an absence, never a guessed HEAD).
    async fn base_commit(&self, instance: &str) -> Result<Option<airc_work::GitObjectId>, CommandError>;

    /// The transferable candidate patch of `workspace` against `base` (HEAD when `None`).
    fn candidate_diff(&self, workspace: &std::path::Path, base: Option<&airc_work::GitObjectId>) -> Result<String, CommandError>;

    /// Grade a hand-in on a bench card, where the artifact lives. A card that is not a
    /// graded bench card returns at once.
    async fn grade_hand_in(
        &self,
        airc: Arc<airc_lib::Airc>,
        room: airc_lib::RoomId,
        card_title: String,
        instance: String,
        patch: airc_blobs::MediaRef,
    );
}
