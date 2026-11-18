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
