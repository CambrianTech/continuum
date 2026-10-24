//! Records a convergence attempt only once the existing process owner starts a generation.
use super::{LlamaServerControl, ENGINE_CONVERGE_TRIED};
use crate::inference::slots::EngineGeneration;

pub(super) struct ConvergenceAttempt<'a, C: LlamaServerControl + ?Sized> {
    ctrl: &'a C,
    before: Option<EngineGeneration>,
    pair: Option<String>,
}

impl<'a, C: LlamaServerControl + ?Sized> ConvergenceAttempt<'a, C> {
    pub(super) fn new(ctrl: &'a C, pair: Option<String>) -> Self {
        Self { ctrl, before: ctrl.last_launch_generation(), pair }
    }
}

impl<C: LlamaServerControl + ?Sized> Drop for ConvergenceAttempt<'_, C> {
    fn drop(&mut self) {
        let Some(pair) = self.pair.take() else { return };
        let Some(after) = self.ctrl.last_launch_generation() else { return };
        if !self.before.as_ref().is_some_and(|before| before.same_engine(&after)) {
            *ENGINE_CONVERGE_TRIED.lock() = Some(pair);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::llama_server::LlamaServerProcess;

    // A pre-launch refusal leaves the pair retryable; cancellation after the
    // process owner's launch-stage receipt must still prevent a stale-build loop.
    #[tokio::test]
    async fn only_a_new_physical_generation_consumes_convergence() -> Result<(), String> {
        let root = format!("http://convergence-fixture-{}", uuid::Uuid::new_v4());
        let process = LlamaServerProcess::with_root(root.clone());
        let endpoint = crate::inference::slots::directory().endpoint(&root);
        let transition = endpoint.transition().await;
        let first = transition.start_generation()?;
        *process.last_started.lock() = Some(first.clone());
        let refused = format!("refused-{}", uuid::Uuid::new_v4());
        {
            let _attempt = ConvergenceAttempt::new(&process, Some(refused.clone()));
        }
        assert_ne!(ENGINE_CONVERGE_TRIED.lock().as_ref(), Some(&refused));
        first.observed_exit();
        let cancelled = format!("cancelled-{}", uuid::Uuid::new_v4());
        let (started, observed) = tokio::sync::oneshot::channel();
        {
            let operation = async {
                let _attempt = ConvergenceAttempt::new(&process, Some(cancelled.clone()));
                *process.last_started.lock() = Some(transition.start_generation()?);
                let _ = started.send(());
                std::future::pending::<Result<(), String>>().await
            };
            tokio::pin!(operation);
            tokio::select! {
                result = &mut operation => return Err(format!("unexpected completion: {result:?}")),
                signal = observed => signal.map_err(|e| e.to_string())?,
            }
            // Cancellation drops the pending operation, including its receipt guard.
        }
        assert_eq!(ENGINE_CONVERGE_TRIED.lock().as_ref(), Some(&cancelled));
        Ok(())
    }
}
