//! The private teacher borrower's teardown as an event: the serving owner waits for the child's
//! exit (announced after its custody is released), never a status polled on a cadence.

use super::PrivateTeacherLane;
use crate::sdk_codegen::CommandError;

impl PrivateTeacherLane {
    /// Tear the borrower down and return once its child exited and released its custody.
    pub async fn finished(&mut self) -> Result<(), CommandError> {
        self.adapter.take();
        self.lane
            .finished()
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}
