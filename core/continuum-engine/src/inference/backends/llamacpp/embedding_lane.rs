//! One embedding context per loaded backend; cancellation cannot abandon its custody.
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(crate) struct EmbeddingLane(Arc<Semaphore>);

impl EmbeddingLane {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }

    pub(crate) async fn acquire(&self) -> Result<EmbeddingPermit, String> {
        self.0
            .clone()
            .acquire_owned()
            .await
            .map(|permit| EmbeddingPermit { _permit: permit })
            .map_err(|e| format!("embedding lane closed: {e}"))
    }
}

pub(crate) struct EmbeddingPermit {
    _permit: OwnedSemaphorePermit,
}

impl EmbeddingPermit {
    pub(crate) async fn run<C, T>(
        self,
        custody: C,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> Result<T, String>
    where
        C: Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(move || {
            let _permit = self;
            let _custody = custody;
            work()
        })
        .await
        .map_err(|e| format!("embedding worker failed: {e}"))?
    }
}

/// Admission is bound to the model it protects, so a caller cannot spend it on another backend.
pub(crate) struct EmbeddingCall {
    pub(super) model: Arc<llama::Model>,
    pub(super) permit: EmbeddingPermit,
}

impl EmbeddingCall {
    pub(crate) async fn run<C: Send + 'static>(
        self,
        texts: Vec<String>,
        custody: C,
    ) -> Result<Vec<Vec<f32>>, String> {
        self.permit
            .run(custody, move || super::embed_model(&self.model, &texts))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires CONTINUUM_TEST_EMBED_MODEL pointing to the Qwen3 embedding GGUF"]
    async fn concurrent_embedding_requests_use_real_model() {
        let path = std::env::var_os("CONTINUUM_TEST_EMBED_MODEL").expect("explicit model path");
        let backend = super::super::LlamaCppBackend::load(super::super::LlamaCppConfig {
            model_path: path.into(),
            n_gpu_layers: 0,
            ..Default::default()
        })
        .expect("load embedding model");
        let results = futures::future::join_all((0..4).map(|_| async {
            backend
                .admit_embedding()
                .await
                .expect("admission")
                .run(
                    vec!["The resident preserves her work across restart.".into()],
                    (),
                )
                .await
                .expect("embedding")
        }))
        .await;
        for result in &results {
            assert_eq!(result.len(), 1);
            assert!(!result[0].is_empty());
            assert!(result[0].iter().all(|x| x.is_finite()));
            let norm: f32 = result[0].iter().map(|x| x * x).sum();
            assert!((norm - 1.0).abs() < 0.001);
            assert_eq!(result, &results[0]);
        }
    }

    #[tokio::test]
    async fn queued_embedding_cancellation_cannot_admit_a_second_context() {
        let lane = EmbeddingLane::new();
        let first = lane.acquire().await.expect("first admitted");
        {
            let waiting = lane.acquire();
            tokio::pin!(waiting);
            assert!(futures::poll!(&mut waiting).is_pending());
        }
        assert_eq!(lane.0.available_permits(), 0);
        let result: Result<(), String> = first
            .run((), || Err("context allocation refused".into()))
            .await;
        assert_eq!(result, Err("context allocation refused".into()));
        let next = lane
            .acquire()
            .await
            .expect("failure returned context capacity");
        next.run((), || Ok(()))
            .await
            .expect("subsequent worker succeeds");
    }

    #[tokio::test]
    async fn cancelled_embedding_retains_context_and_memory_custody() {
        let lane = EmbeddingLane::new();
        let permit = lane.acquire().await.expect("first admitted");
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (dropped_tx, mut dropped_rx) = tokio::sync::oneshot::channel();
        struct Custody(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for Custody {
            fn drop(&mut self) {
                if let Some(tx) = self.0.take() {
                    let _ = tx.send(());
                }
            }
        }
        let task = tokio::spawn(permit.run(Custody(Some(dropped_tx)), move || {
            let _ = started_tx.send(());
            release_rx.recv().expect("release worker");
            Ok(())
        }));
        started_rx.await.expect("worker started");
        task.abort();
        let _ = task.await;
        let held = lane.0.available_permits() == 0;
        let custody_held = matches!(
            dropped_rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        );
        release_tx.send(()).expect("finish worker");
        let _next = lane
            .acquire()
            .await
            .expect("next admitted after completion");
        assert!(
            held,
            "caller cancellation released a running context's permit"
        );
        assert!(
            custody_held,
            "caller cancellation released the worker's memory claim"
        );
    }
}
