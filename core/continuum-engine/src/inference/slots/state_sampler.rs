//! Single owner of saved-state samples and disk-read admission.
use super::citizen_slots::{
    measured_state_bytes_in, newest_page_states, note_state_bytes_in, seed_dir_in,
    state_dir_known_in, StateSamples, STATE_SAMPLES,
};

/// Owns a bounded sample cache and its single disk read. Production shares one
/// instance; independent engines/tests can own an isolated instance of the same mechanism.
#[derive(Default)]
pub(crate) struct StateSampler {
    samples: parking_lot::Mutex<StateSamples>,
    seeding: parking_lot::Mutex<
        Option<(
            std::path::PathBuf,
            futures::future::Shared<futures::future::BoxFuture<'static, ()>>,
        )>,
    >,
    #[cfg(test)]
    pub(super) reads: parking_lot::Mutex<Vec<std::path::PathBuf>>,
}

pub(super) static STATE_SAMPLER: std::sync::LazyLock<std::sync::Arc<StateSampler>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(StateSampler::default()));

impl StateSampler {
    pub(super) async fn seed(self: &std::sync::Arc<Self>, dir: &std::path::Path) {
        self.seed_with(dir, |dir| newest_page_states(dir, STATE_SAMPLES))
            .await;
    }

    pub(super) fn note(&self, dir: &std::path::Path, bytes: u64) {
        if bytes != 0 {
            note_state_bytes_in(&mut self.samples.lock(), dir, bytes);
        }
    }

    pub(super) fn measured(&self, dir: &std::path::Path) -> Option<u64> {
        measured_state_bytes_in(&self.samples.lock(), dir)
    }

    #[cfg(test)]
    pub(super) fn known(&self, dir: &std::path::Path) -> bool {
        state_dir_known_in(&self.samples.lock(), dir)
    }

    #[cfg(test)]
    pub(super) fn reading(&self, dir: &std::path::Path) -> bool {
        self.seeding
            .lock()
            .as_ref()
            .is_some_and(|(reading, _)| reading == dir)
    }

    pub(super) async fn seed_with(
        self: &std::sync::Arc<Self>,
        dir: &std::path::Path,
        read: impl FnOnce(&std::path::Path) -> Result<Vec<u64>, String> + Send + 'static,
    ) {
        use crate::runtime::daemon::{guarded, Guarded};
        let reading = {
            let mut seeding = self.seeding.lock();
            if state_dir_known_in(&self.samples.lock(), dir) {
                return;
            }
            match &*seeding {
                Some((reading, read)) if reading == dir => read.clone(),
                Some((reading, _)) => {
                    continuum_base::probe!(
                        class = "inference.kv_page.seed_busy",
                        dir = %dir.display(),
                        reading = %reading.display(),
                        "another page dir is still being read; this sizing uses the estimate, the next ask reads this one",
                    );
                    return;
                }
                None => {
                    let landed = self.spawn_read(dir.to_path_buf(), read);
                    *seeding = Some((dir.to_path_buf(), landed.clone()));
                    landed
                }
            }
        };
        if let Guarded::TimedOut = guarded(
            crate::system_resources::disk_pressure::REPORTER_TIMEOUT,
            reading,
        )
        .await
        {
            continuum_base::probe!(
                class = "inference.kv_page.seed_pending",
                dir = %dir.display(),
                "the saved pages are still being read; this sizing uses the estimate, the next one the pages",
            );
        }
    }

    /// Start the read of `dir` as its own task and return its completion for askers to wait on. The
    /// task lands the samples (a live sample already there outranks them), THEN clears its read slot, so
    /// no asker between the two starts a second read; it never holds both locks.
    fn spawn_read(
        self: &std::sync::Arc<Self>,
        dir: std::path::PathBuf,
        read: impl FnOnce(&std::path::Path) -> Result<Vec<u64>, String> + Send + 'static,
    ) -> futures::future::Shared<futures::future::BoxFuture<'static, ()>> {
        use futures::FutureExt;
        #[cfg(test)]
        self.reads.lock().push(dir.clone());
        let released = dir.clone();
        let worker = self.clone();
        let completion = std::sync::Arc::downgrade(self);
        let task = tokio::spawn(async move {
            let path = dir.clone();
            match tokio::task::spawn_blocking(move || read(&path))
                .await
                .map_err(|error| error.to_string())
                .and_then(|states| states)
            {
                Ok(states) => seed_dir_in(&mut worker.samples.lock(), &dir, states),
                Err(error) => continuum_base::probe!(
                    class = "inference.kv_page.seed_failed",
                    dir = %dir.display(),
                    error = %error,
                    "reading the saved pages on disk failed; left unmeasured, the next ask reads again",
                ),
            }
            let mut seeding = worker.seeding.lock();
            if seeding.as_ref().is_some_and(|(reading, _)| reading == &dir) {
                *seeding = None;
            }
        });
        async move {
            // The read's own failures (the reader, the blocking pool) release the slot inside the
            // task. Only a panic of the task itself skips that, and then the slot is released here,
            // when an asker next waits on it: waiter-dependent, and with no reachable panic in the
            // task body, not relied on (root on #5060).
            if task.await.is_err() {
                let Some(completion) = completion.upgrade() else {
                    return;
                };
                let mut seeding = completion.seeding.lock();
                if seeding
                    .as_ref()
                    .is_some_and(|(reading, _)| reading == &released)
                {
                    *seeding = None;
                }
            }
        }
        .boxed()
        .shared()
    }
}
