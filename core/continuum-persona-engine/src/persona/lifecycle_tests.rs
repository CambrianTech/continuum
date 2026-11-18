use super::*;
use futures::executor::block_on;

/// How a test part answers suspension: the three behaviours that exist, named (not two flags).
#[derive(Clone, Copy)]
enum Behaviour {
    Ready,
    PendingOnce,
    Fails,
}

struct Part {
    behaviour: Behaviour,
    attempts: usize,
}

impl Part {
    fn ready() -> Self {
        Self::with(Behaviour::Ready)
    }
    fn with(behaviour: Behaviour) -> Self {
        Self { behaviour, attempts: 0 }
    }
}

impl Lifecycle for Part {
    type Suspension = Preservation<u64, &'static str>;
    type Release = Releasing<u64, &'static str>;

    async fn suspend(&mut self) -> Self::Suspension {
        self.attempts += 1;
        match self.behaviour {
            Behaviour::Fails => Preservation::Failed("storage unavailable"),
            Behaviour::PendingOnce if self.attempts == 1 => Preservation::Pending(17),
            Behaviour::PendingOnce | Behaviour::Ready => Preservation::Preserved,
        }
    }

    async fn release(&mut self) -> Self::Release {
        self.attempts += 1;
        match self.behaviour {
            Behaviour::Fails => Releasing::Failed("free not proven"),
            Behaviour::PendingOnce if self.attempts == 1 => Releasing::Pending(17),
            Behaviour::PendingOnce | Behaviour::Ready => Releasing::Released,
        }
    }
}

// what this catches (Joel 2026-11-18: "persona::shutdown calls all persona sub objects"): a
// persona release that skips a part, or reports Released while one part is still pending or failed.
#[test]
fn a_persona_release_reaches_every_part_and_is_complete_only_when_all_are() {
    let mut parts = PersonaParts {
        blocks: Part::ready(),
        tokens: Part::ready(),
        genome: Part::ready(),
        learning: Part::with(Behaviour::PendingOnce),
        decode: Part::ready(),
    };
    parts.genome.behaviour = Behaviour::Fails;
    let mut persona = PersonaUnit::new(PersonaId(uuid::Uuid::nil()), Epoch(1), parts);
    let report = block_on(persona.release());
    assert_eq!(report.genome, Releasing::Failed("free not proven"));
    assert_eq!(report.learning, Releasing::Pending(17));
    assert!(report.blocks.is_complete() && report.tokens.is_complete() && report.decode.is_complete());
    assert!(!report.is_complete());
    assert!([&persona.parts().blocks, &persona.parts().decode].iter().all(|part| part.attempts == 1), "every part was asked");
}

#[test]
fn root_reports_pending_and_failed_children_and_keeps_custody_for_retry() {
    let mut parts = PersonaParts {
        blocks: Part::ready(),
        tokens: Part::ready(),
        genome: Part::ready(),
        learning: Part::with(Behaviour::PendingOnce),
        decode: Part::ready(),
    };
    parts.tokens.behaviour = Behaviour::Fails;
    let mut persona = PersonaUnit::new(PersonaId(uuid::Uuid::nil()), Epoch(1), parts);
    let report = block_on(persona.suspend());
    assert_eq!(report.tokens, Preservation::Failed("storage unavailable"));
    assert_eq!(report.learning, Preservation::Pending(17));
    assert!(report.decode.is_complete());
    assert!(!report.is_complete());

    let retried = block_on(persona.suspend());
    assert!(retried.learning.is_complete());
    assert!(!retried.is_complete());
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
        type Release = Releasing<(), oneshot::Canceled>;

        async fn suspend(&mut self) -> Self::Suspension {
            self.started.fetch_add(1, Ordering::SeqCst);
            match (&mut self.fence).await {
                Ok(()) => Preservation::Preserved,
                Err(error) => Preservation::Failed(error),
            }
        }

        async fn release(&mut self) -> Self::Release {
            Releasing::Released
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
        assert!(suspend.await.is_complete());
    });
}
