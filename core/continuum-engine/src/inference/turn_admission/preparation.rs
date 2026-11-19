//! A preparing turn keeps its existing grant through physical paging and quote.
use super::TurnAdmission;
use crate::capacity::window_grant::{GrantLease, GrantReservation, ReserveRefused};

#[derive(Clone, Copy)]
pub(crate) struct Preparation {
    pub persona: uuid::Uuid,
    pub lease: GrantLease,
}

pub(crate) enum AdmissionFailure {
    Unavailable(String),
    Grant(ReserveRefused),
}

impl From<String> for AdmissionFailure {
    fn from(reason: String) -> Self { Self::Unavailable(reason) }
}

impl AdmissionFailure {
    pub(super) fn into_text(self) -> String {
        match self {
            Self::Unavailable(reason) => reason,
            Self::Grant(reason) => format!("grant admission refused: {reason:?}"),
        }
    }
}

impl From<AdmissionFailure> for crate::ai::inference_error::InferenceError {
    fn from(failure: AdmissionFailure) -> Self {
        match failure {
            AdmissionFailure::Unavailable(reason) => Self::Unavailable(reason),
            AdmissionFailure::Grant(ReserveRefused::Changed { leased, current }) =>
                Self::GrantChanged { rendered: leased.tokens, current: current.map(|lease| lease.tokens) },
            AdmissionFailure::Grant(ReserveRefused::Retiring { leased }) =>
                Self::GrantChanged { rendered: leased.tokens, current: None },
            AdmissionFailure::Grant(ReserveRefused::Over { count, grant }) =>
                Self::ContextExceeded { requested: count, available: grant },
        }
    }
}

impl TurnAdmission {
    pub(super) fn preparation_io_completed(&mut self) {
        if let Some(grant) = &mut self.preparation { grant.preparation_io_completed(); }
    }

    /// Called only after physical readiness, before paging can outlive its caller.
    pub(super) fn reserve_preparation(&mut self, preparation: Option<Preparation>) -> Result<(), AdmissionFailure> {
        let Some(Preparation { persona, lease }) = preparation else { return Ok(()); };
        if self.engine_incarnation().is_none()
            || self.engine_incarnation() != lease.engine.map(|engine| (engine.pid, engine.started_s)) {
            return Err(AdmissionFailure::Grant(ReserveRefused::Changed { leased: lease, current: None }));
        }
        // Zero is the as-yet unquoted request, not a zero-sized physical claim:
        // the reservation owner charges the entire lease until quote refinement.
        crate::capacity::window_grant::reserve_attempt(&mut self.preparation, persona, lease, 0)
            .map_err(AdmissionFailure::Grant)
    }

    /// Transfer without releasing: the exact quote refines this same authority.
    pub(crate) fn take_preparation(&mut self) -> Option<GrantReservation> {
        self.preparation.take()
    }
}
