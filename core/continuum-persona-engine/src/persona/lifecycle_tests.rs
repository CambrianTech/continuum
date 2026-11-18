use super::*;
use futures::executor::block_on;

struct Part {
    release_after_retry: bool,
    failure: bool,
    attempts: usize,
}

impl Part {
    fn ready() -> Self {
        Self { release_after_retry: false, failure: false, attempts: 0 }
    }
}

impl Lifecycle for Part {
    type Suspension = Preservation<u64, &'static str>;

    async fn suspend(&mut self) -> Self::Suspension {
        self.attempts += 1;
        if self.failure {
            Preservation::Failed("storage unavailable")
        } else if self.release_after_retry && self.attempts == 1 {
            Preservation::Pending(17)
        } else {
            Preservation::Preserved
        }
    }
}

#[test]
fn root_reports_pending_and_failed_children_and_keeps_custody_for_retry() {
    let mut parts = PersonaParts {
        blocks: Part::ready(),
        tokens: Part::ready(),
        genome: Part::ready(),
        learning: Part { release_after_retry: true, ..Part::ready() },
        decode: Part::ready(),
    };
    parts.tokens.failure = true;
    let mut persona = PersonaUnit::new(PersonaId(uuid::Uuid::nil()), Epoch(1), parts);
    let report = block_on(persona.suspend());
    assert_eq!(report.tokens, Preservation::Failed("storage unavailable"));
    assert_eq!(report.learning, Preservation::Pending(17));
    assert!(report.decode.is_preserved());
    assert!(!report.is_preserved());

    let retried = block_on(persona.suspend());
    assert!(retried.learning.is_preserved());
    assert!(!retried.is_preserved());
    assert_eq!(persona.parts().learning.attempts, 2);
}

#[test]
fn one_pending_fence_leaves_all_siblings_free_to_start() {
    use futures::channel::oneshot;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    struct Gated {
        started: Arc<AtomicUsize>,
        fence: oneshot::Receiver<()>,
    }
    impl Lifecycle for Gated {
        type Suspension = Preservation<(), oneshot::Canceled>;

        async fn suspend(&mut self) -> Self::Suspension {
            self.started.fetch_add(1, Ordering::SeqCst);
            match (&mut self.fence).await {
                Ok(()) => Preservation::Preserved,
                Err(error) => Preservation::Failed(error),
            }
        }
    }

    let started = Arc::new(AtomicUsize::new(0));
    let mut signals = Vec::new();
    let mut part = || {
        let (signal, fence) = oneshot::channel();
        signals.push(signal);
        Gated { started: started.clone(), fence }
    };
    let mut persona = PersonaUnit::new(PersonaId(uuid::Uuid::nil()), Epoch(1), PersonaParts {
        blocks: part(), tokens: part(), genome: part(), learning: part(), decode: part(),
    });
    block_on(async {
        let mut suspend = Box::pin(persona.suspend());
        assert!(futures::poll!(&mut suspend).is_pending());
        assert_eq!(started.load(Ordering::SeqCst), 5);
        for signal in signals.into_iter().rev() {
            assert!(signal.send(()).is_ok());
        }
        assert!(suspend.await.is_preserved());
    });
}
