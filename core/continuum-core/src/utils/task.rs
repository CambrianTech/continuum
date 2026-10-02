//! Ownership for spawned helpers: dropping a JoinHandle alone detaches its task.

pub(crate) struct AbortOnDrop(pub(crate) tokio::task::JoinHandle<()>);

impl AbortOnDrop {
    /// Keep cancellation ownership even while awaiting normal completion.
    pub(crate) async fn join(mut self) -> Result<(), tokio::task::JoinError> {
        (&mut self.0).await
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
