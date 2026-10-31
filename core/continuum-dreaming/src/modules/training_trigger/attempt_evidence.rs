//! What the attempt ledger proves about an interrupted dispatch (card 55d4b9e3).
//!
//! Recovery used to ask only the job board, which learns of a job after `create_job` returns,
//! so "not on the board" proved nothing and a stranded intent waited forever (IntelMac 10-24,
//! eight buckets). An intent that names its attempt now asks `genome/job-attempt-reconcile`,
//! whose ledger is written before the trainer runs and closes the attempt before it answers
//! "not created".

use serde::Deserialize;
use uuid::Uuid;

use super::TrainingTriggerState;
use crate::genome::fine_tuning::types::JobHandle;

/// The proven outcome of one attempt.
#[derive(Debug, PartialEq)]
pub(crate) enum AttemptEvidence {
    Created(JobHandle),
    /// Closed by the ledger: nothing exists or can start under it, so a new attempt is safe.
    NotCreated,
    /// Undecided; the reason is kept on the intent's recovery receipt.
    Unknown(String),
}

#[derive(Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
enum Outcome {
    Created {
        #[serde(rename = "jobHandle")]
        handle: JobHandle,
    },
    NotCreated,
    Unknown {
        reason: String,
    },
}

impl AttemptEvidence {
    /// Decode the command's typed answer. Anything unreadable is Unknown, never NotCreated.
    pub(crate) fn decode(answer: serde_json::Value) -> Self {
        match serde_json::from_value::<Outcome>(answer) { // boundary: the command's typed reconcile outcome
            Ok(Outcome::Created { handle }) => Self::Created(handle),
            Ok(Outcome::NotCreated) => Self::NotCreated,
            Ok(Outcome::Unknown { reason }) => Self::Unknown(reason),
            Err(e) => Self::Unknown(format!("unreadable reconcile answer: {e}")),
        }
    }
}

impl TrainingTriggerState {
    /// Ask the attempt ledger what `attempt` did. A command that cannot be reached is Unknown.
    pub(crate) async fn reconcile_attempt(&self, attempt: Uuid) -> AttemptEvidence {
        let executor = match self.executor.require() {
            Ok(executor) => executor,
            Err(e) => return AttemptEvidence::Unknown(format!("no command executor: {e}")),
        };
        match executor
            .execute_json(
                "genome/job-attempt-reconcile",
                serde_json::json!({ "attempt": attempt }),
            )
            .await
        {
            Ok(answer) => AttemptEvidence::decode(answer),
            Err(e) => AttemptEvidence::Unknown(format!("genome/job-attempt-reconcile: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: only an explicit typed answer decides an attempt. A malformed or
    // foreign answer is Unknown, never read as "not created" (which would retry a job that
    // may be running).
    #[test]
    fn only_a_typed_answer_decides_an_attempt() {
        use serde_json::json;
        assert_eq!(AttemptEvidence::decode(json!({"outcome": "notCreated"})), AttemptEvidence::NotCreated);
        let id = Uuid::new_v4();
        assert_eq!(
            AttemptEvidence::decode(json!({"outcome": "created", "jobHandle": {
                "providerId": "engine-local", "providerJobId": id.to_string(), "localId": id
            }})),
            AttemptEvidence::Created(JobHandle {
                provider_id: "engine-local".into(),
                provider_job_id: id.to_string(),
                local_id: id
            })
        );
        for answer in [json!({}), json!({"outcome": "NotCreated"}), json!({"success": true}), json!(null)] {
            assert!(matches!(AttemptEvidence::decode(answer), AttemptEvidence::Unknown(_)));
        }
        assert!(matches!(
            AttemptEvidence::decode(json!({"outcome": "unknown", "reason": "admitted, never settled"})),
            AttemptEvidence::Unknown(r) if r == "admitted, never settled"
        ));
    }
}
