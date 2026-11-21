//! How much the calling mind needs this generation (object model 5.7, Need), carried from her
//! deliberation into the engine for the call's lifetime, the way her lane custody is
//! (`resource_admission::with_serving_lane`). The Core ranks waiting turns by it (`frame::pick`).
//! The wire request stays unchanged: this is an in-process fact of one call.

use std::future::Future;

use continuum_sage::component::Urgency;

tokio::task_local! {
    static URGENCY: Urgency;
}

/// Run `call` as a generation her mind needs at `urgency`.
pub async fn with_urgency<F: Future>(urgency: Urgency, call: F) -> F::Output {
    URGENCY.scope(urgency, call).await
}

/// The calling mind's urgency. A caller that set none is background work (a dream, a sentinel, a
/// probe), and Content is what that caller is, not a stand-in for an unknown.
pub fn current() -> Urgency {
    URGENCY.try_with(|urgency| *urgency).unwrap_or(Urgency::Content) // unwrap_or: no scope means a caller that is not a mind's deliberation, which is background work by definition
}
