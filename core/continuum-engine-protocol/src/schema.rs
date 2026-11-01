//! The JSON Schema of every message on the training seam, one file per message under
//! `protocol/schema/engine/`. `cargo test -p continuum-engine-protocol` writes them; CI's binding
//! drift guard fails when they differ from what is committed, and the engine fork's server tests
//! validate its requests and replies against them.

use schemars::schema::RootSchema;
use schemars::schema_for;

/// Every message on the seam, by the file name it is published under.
pub fn messages() -> Vec<(&'static str, RootSchema)> {
    vec![
        ("train-request", schema_for!(crate::TrainRequest)),
        ("train-status", schema_for!(crate::TrainStatus)),
        ("job-ref", schema_for!(crate::JobRef)),
        ("measure-request", schema_for!(crate::MeasureRequest)),
        ("measure-reply", schema_for!(crate::MeasureReply)),
        ("learn-request", schema_for!(crate::LearnRequest)),
        ("learn-outcome", schema_for!(crate::LearnOutcome)),
        ("error", schema_for!(crate::EngineErrorBody)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the engine fork validating against a schema that no longer matches the
    // types the core sends and parses. The files are regenerated on every test run; the drift
    // guard fails a PR that changes a type without committing its schema.
    #[test]
    fn the_published_schemas_are_the_types() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../protocol/schema/engine");
        std::fs::create_dir_all(&dir).expect("create protocol/schema/engine"); // test: a fixture step that must succeed
        for (name, schema) in messages() {
            let json = serde_json::to_string_pretty(&schema).expect("a schema serializes") + "\n"; // boundary: the published schema file on disk
            std::fs::write(dir.join(format!("{name}.schema.json")), json).expect("write the published schema"); // test: a fixture step that must succeed
        }
    }

    // what this catches: a wire body the engine really sends failing to parse, which the core
    // reads as silence (a lost status) and eventually as a lane gone. Bodies copied from the fork's
    // emitters (server-train.cpp status(), server-task.cpp learn_outcome::to_json).
    #[test]
    fn the_engines_own_bodies_parse() {
        let status: crate::TrainStatus = serde_json::from_str(
            r#"{"state":"starting","out":"probe-mmid-0.gguf","epochs":[],"window":256,"window_asked":256,"examples":1,
                "examples_truncated":0,"examples_skipped":0,"kept_indices":[0],"truncated_indices":[],"skipped_indices":[],
                "yielded_ms":0,"share_ppm":250000,"max_slowdown_ppm":0,"pause_requested":false,"paused":false,"waiting_for_serving":false}"#,
        )
        .expect("the M5's GET /train body at 02:00Z parses"); // test: a fixture step that must succeed
        assert_eq!(status.state, crate::TrainState::Starting);
        let none: crate::TrainStatus = serde_json::from_str(r#"{"state":"unavailable"}"#).expect("a server with no trainer"); // test: a fixture step that must succeed
        assert_eq!(none.state, crate::TrainState::Unavailable);
        let refused: crate::LearnOutcome =
            serde_json::from_str(r#"{"success":false,"loss":0.0,"n_labels":0,"t_ms":0.0,"failure":"no loaded adapter 3"}"#)
                .expect("a refused learn"); // test: a fixture step that must succeed
        assert!(!refused.success && refused.failure.as_deref() == Some("no loaded adapter 3"));
        let error: crate::EngineErrorBody = serde_json::from_str(
            r#"{"error":{"code":400,"message":"\"window\" must be an integer","type":"invalid_request_error"}}"#,
        )
        .expect("an engine refusal"); // test: a fixture step that must succeed
        assert_eq!(error.error.kind, crate::ErrorType::InvalidRequestError);
    }
}
