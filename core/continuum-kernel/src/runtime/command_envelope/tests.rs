#![cfg(test)]
    use super::*;
    use serde_json::json;

    // ── CommandRequest<P> ────────────────────────────────────────────

    #[derive(Debug, Clone, Deserialize, Serialize, PartialEq, TS)]
    struct StartParams {
        model: String,
        max_tokens: u32,
    }

    #[test]
    fn request_parses_flat_params_no_envelope_fields() {
        // Wire JSON without any envelope fields — pure command params.
        let value = json!({ "model": "qwen", "max_tokens": 512 });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.params.model, "qwen");
        assert_eq!(req.params.max_tokens, 512);
        assert!(
            req.handle.is_none()
                && req.session_id.is_none()
                && req.user_id.is_none()
                && req.context_id.is_none(),
            "envelope fields default to None when absent in the wire JSON"
        );
    }

    #[test]
    fn request_parses_envelope_fields_flat() {
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let handle_id = Uuid::new_v4();
        let value = json!({
            "model": "qwen",
            "max_tokens": 256,
            "sessionId": session_id.to_string(),
            "userId": user_id.to_string(),
            "handle": {
                "owner": "ai/inference",
                "id": handle_id.to_string(),
                "type_tag": "ai::InferenceSession",
                "created_at_ms": 1_700_000_000_000_u64
            }
        });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.params.model, "qwen");
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
        assert_eq!(req.handle.unwrap().id.as_uuid(), handle_id);
    }

    // what this catches (2026-09-17): a command whose params carry their OWN
    // `requestId: String` (every persona turn's TextGenerationRequest) must parse on
    // the responder — the envelope's integer counter and the command's uuid share one
    // flat key, and refusing the string killed five of six remote turns the hour
    // cross-grid inference first became addressable. The integer still lands in the
    // envelope; the string is the command's and the envelope reads no counter.
    #[test]
    fn a_commands_own_string_request_id_does_not_break_the_envelope() {
        let value = json!({ "model": "qwen", "max_tokens": 8, "requestId": "05acabe7-80ef-43ba-b954-9dd43784e037" });
        let req = CommandRequest::<StartParams>::from_value(value)
            .expect("a string requestId is the command's, not a type error"); // JUSTIFIED: the invariant under test
        assert_eq!(req.request_id, None, "the envelope has no counter on this hop");
        assert_eq!(req.params.model, "qwen");
        let value = json!({ "model": "qwen", "max_tokens": 8, "requestId": 7 });
        let req = CommandRequest::<StartParams>::from_value(value).expect("an integer is the counter"); // JUSTIFIED: the invariant under test
        assert_eq!(req.request_id, Some(7));
    }

    use crate::runtime::arguments::{FieldProblem, FieldRefusal, JsonKind, Segment};

    fn missing(value: Value) -> (Vec<Segment>, Option<Vec<String>>, Option<String>) {
        #[derive(Debug, Deserialize)]
        struct ShellParams {
            cmd: String,
        }
        match CommandRequest::<ShellParams>::from_value(value) {
            Ok(req) => panic!("expected a refusal, decoded cmd {}", req.params.cmd),
            Err(ArgumentsRefusal::Field(FieldRefusal { path, problem: FieldProblem::Missing { sent, sent_as } })) => (path.0, sent, sent_as),
            other => panic!("expected a missing-field refusal, got {other:?}"),
        }
    }

    // what this catches: a wrong-typed field refused as text (or with its name lost to serde's
    // flatten) instead of naming the field, what it needed and what she sent, as data.
    #[test]
    fn a_wrong_typed_field_is_refused_naming_the_field() {
        let value = json!({ "model": "qwen", "max_tokens": "not-a-number" });
        match CommandRequest::<StartParams>::from_value(value) {
            Err(ArgumentsRefusal::Field(FieldRefusal { path, problem: FieldProblem::WrongType { expected, sent } })) => {
                assert_eq!(path.0, vec![Segment::Key("max_tokens".into())]);
                assert_eq!(sent, JsonKind::String);
                assert_eq!(expected, "u32");
            }
            other => panic!("expected the max_tokens field refused, got {other:?}"),
        }
    }

    // what this catches: the most common tool-use failure there is — a NAME mismatch, reported
    // one-sidedly. serde says "missing field `cmd`" and never mentions that the caller sent
    // `command`, so the message reads as "you forgot something" when the truth is "you called
    // it something else". The correspondence is data now (`sent_as`), never read out of text.
    #[test]
    fn a_param_name_mismatch_names_what_was_sent_not_just_what_was_wanted() {
        let (path, sent, sent_as) = missing(json!({ "command": "ls", "timeout": 5 }));
        assert_eq!(path, vec![Segment::Key("cmd".into())]);
        assert_eq!(sent_as.as_deref(), Some("command"));
        assert_eq!(sent, Some(vec!["command".to_string(), "timeout".to_string()]));
    }

    // what this catches: a NEW envelope field being added to `CommandRequest` without being
    // added to the filter — the exact way `actorKind` got missed. The cost when it was missed,
    // measured 2026-09-05: both IntelMac personas sent NO parameters and were told to fix a
    // field the KERNEL had added. Unremovable, so they resent the identical call six-plus times.
    #[test]
    fn every_envelope_field_is_filtered_from_the_you_sent_list() {
        let mut call = serde_json::Map::new();
        for field in ENVELOPE_FIELDS {
            call.insert(field.to_string(), Value::Null);
        }
        let (_, sent, sent_as) = missing(Value::Object(call));
        assert_eq!(sent, Some(Vec::new()), "a call carrying ONLY envelope fields sent no parameters");
        assert_eq!(sent_as, None);
    }

    // what this catches: over-filtering. `command` is DELIBERATELY not an envelope field even
    // though the CLI adds it — for `code/shell` it is the single likeliest mis-name for `cmd`.
    #[test]
    fn command_stays_a_candidate_because_it_is_the_likeliest_misname() {
        let (_, sent, sent_as) = missing(json!({ "command": "ls", "actorKind": "agent" }));
        assert_eq!(sent_as.as_deref(), Some("command"));
        assert_eq!(sent, Some(vec!["command".to_string()]), "the envelope field alongside it stays filtered");
    }

    // what this catches: a confident WRONG rename is worse than none — it sends her to change
    // the wrong field. An unrelated name is listed as sent and never offered as the field.
    #[test]
    fn an_unrelated_param_gets_a_listing_never_an_invented_rename() {
        let (_, sent, sent_as) = missing(json!({ "colour": "red" }));
        assert_eq!(sent_as, None);
        assert_eq!(sent, Some(vec!["colour".to_string()]));
        let (_, sent, _) = missing(json!({}));
        assert_eq!(sent, Some(Vec::new()), "an empty call says so");
    }

    // what this catches: arguments that are not an object reaching serde's flatten (which
    // reports them as a struct mismatch) instead of a refusal saying what she sent.
    #[test]
    fn arguments_that_are_not_an_object_are_refused_by_kind() {
        assert_eq!(
            CommandRequest::<StartParams>::from_value(json!("qwen")).err(),
            Some(ArgumentsRefusal::NotAnObject { sent: JsonKind::String })
        );
    }

    #[test]
    fn request_builder_attaches_envelope_fields() {
        let handle = HandleRef::mint("ai/inference", "ai::InferenceSession");
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let req = CommandRequest::new(StartParams {
            model: "qwen".into(),
            max_tokens: 100,
        })
        .with_handle(handle.clone())
        .with_session(session_id)
        .with_user(user_id);
        assert_eq!(req.handle, Some(handle));
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
    }

    // what this catches: the THIRD ID tier (contextId) parses from the flat wire
    // envelope as a sibling of sessionId/userId, and the builder attaches it.
    // contextId is CLIENT-supplied (the SDK's scoped client stamps it), so unlike
    // session/user it must survive a round-trip from the caller's JSON. A drift
    // that drops it would silently un-scope every per-context handler (per-room
    // memory, per-thread recall, a persona's tool_context).
    #[test]
    fn request_parses_and_builds_context_id_third_tier() {
        let context_id = Uuid::new_v4();
        // From flat wire JSON (the caller stamped contextId alongside params).
        let value = json!({
            "model": "qwen",
            "max_tokens": 256,
            "contextId": context_id.to_string(),
        });
        let req = CommandRequest::<StartParams>::from_value(value).expect("parse must succeed");
        assert_eq!(req.context_id, Some(context_id));
        // Round-trips back out under the camelCase wire key.
        let back = serde_json::to_value(&req).expect("serialize");
        assert_eq!(back["contextId"], json!(context_id.to_string()));
        // And the builder attaches it.
        let built = CommandRequest::new(StartParams {
            model: "q".into(),
            max_tokens: 1,
        })
        .with_context(context_id);
        assert_eq!(built.context_id, Some(context_id));
    }

    // ── CommandResponse<T> ───────────────────────────────────────────

    #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, TS)]
    struct StartData {
        first_token: String,
        tokens_emitted: u32,
    }

    #[test]
    fn response_ok_serializes_flat_with_success_true() {
        let resp = CommandResponse::ok(StartData {
            first_token: "Hello".into(),
            tokens_emitted: 1,
        });
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], true);
        assert_eq!(json["first_token"], "Hello");
        assert_eq!(json["tokens_emitted"], 1);
        assert!(
            json.get("handle").is_none(),
            "handle is omitted when None — clean wire shape"
        );
        assert!(json.get("error").is_none(), "error is omitted when None");
    }

    #[test]
    fn response_with_handle_attaches_handle_at_top_level() {
        let id = Uuid::new_v4();
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", id, "ai::InferenceSession");
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], true);
        assert_eq!(json["handle"]["owner"], "ai/inference");
        assert_eq!(json["handle"]["id"], id.to_string());
        assert_eq!(json["handle"]["type_tag"], "ai::InferenceSession");
        // Data fields stay flat alongside the handle.
        assert_eq!(json["first_token"], "Hi");
    }

    #[test]
    fn response_err_serializes_with_success_false_and_message() {
        let resp = CommandResponse::<StartData>::err("model not found: 'qwen-99'");
        let json = serde_json::to_value(&resp).expect("serialize must succeed");
        assert_eq!(json["success"], false);
        assert_eq!(json["error"], "model not found: 'qwen-99'");
        // Default data fields still present (empty strings, 0 counts).
        assert_eq!(json["first_token"], "");
        assert_eq!(json["tokens_emitted"], 0);
    }

    #[test]
    fn response_into_command_result_yields_json_variant() {
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", Uuid::new_v4(), "ai::InferenceSession");
        let cr = resp
            .into_command_result()
            .expect("materialize must succeed");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["success"], true);
                assert_eq!(v["first_token"], "Hi");
                assert!(v["handle"].is_object());
            }
            other => panic!("expected CommandResult::Json, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_through_wire_preserves_envelope_fields() {
        // End-to-end: typed handler returns Response → serialize as
        // CommandResult → echo as string → deserialize on a "caller"
        // side. The caller-side gets a CommandRequest envelope back
        // (treating the result as the next call's input) — handle,
        // session, user all survive.
        let session_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let handle_id = Uuid::new_v4();

        // Build a response carrying a handle (the producer minted it).
        let resp = CommandResponse::ok(StartData {
            first_token: "Hi".into(),
            tokens_emitted: 1,
        })
        .with_handle("ai/inference", handle_id, "ai::InferenceSession");
        let wire_json = serde_json::to_value(&resp).unwrap();

        // Caller takes the result, builds a new request envelope using
        // the returned handle (+ their own session/user). The new
        // request's params type is a "poll" shape.
        #[derive(Debug, Clone, Deserialize, Serialize, TS)]
        struct PollParams {
            max_tokens: u32,
        }

        let mut next_call = json!({ "max_tokens": 64 });
        next_call["handle"] = wire_json["handle"].clone();
        next_call["sessionId"] = json!(session_id.to_string());
        next_call["userId"] = json!(user_id.to_string());

        let req = CommandRequest::<PollParams>::from_value(next_call)
            .expect("caller round-trips envelope cleanly");
        assert_eq!(req.params.max_tokens, 64);
        assert_eq!(req.session_id, Some(session_id));
        assert_eq!(req.user_id, Some(user_id));
        assert_eq!(req.handle.unwrap().id.as_uuid(), handle_id);
    }

    // ── CommandRequest::handle_id_or_legacy ─────────────────────────
    //
    // The single primitive shared by every additive migration of a
    // stringly-typed id to a typed handle. Distilled from data
    // module's first real consumer (PR #1490) so future migrations
    // don't reimplement the resolver. Each kink the data migration
    // discovered is pinned by a test here so the substrate
    // guarantees them centrally.

    #[derive(Debug, Clone, Default, Deserialize, Serialize, TS)]
    #[serde(rename_all = "camelCase")]
    struct CursorParams {
        #[serde(default)]
        query_id: Option<String>,
    }

    fn cursor_handle(id: Uuid) -> HandleRef {
        HandleRef::with_id("data", id, "data::QueryCursor")
    }

    #[test]
    fn handle_id_or_legacy_prefers_envelope_handle_when_both_present() {
        // When the envelope carries a handle AND a legacy field is
        // also present, the typed handle wins. Otherwise consumers
        // mid-migration would diverge from new consumers about which
        // id the resolver sees.
        let h_id = Uuid::new_v4();
        let req = CommandRequest::new(CursorParams {
            query_id: Some(Uuid::new_v4().to_string()), // legacy populated
        })
        .with_handle(cursor_handle(h_id));

        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect("envelope handle must win");
        assert_eq!(
            resolved,
            h_id.to_string(),
            "envelope handle MUST win when both shapes are present"
        );
    }

    #[test]
    fn handle_id_or_legacy_falls_back_to_legacy_string_when_no_handle() {
        let legacy = "11111111-2222-3333-4444-555555555555".to_string();
        let req = CommandRequest::new(CursorParams {
            query_id: Some(legacy.clone()),
        });

        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect("legacy fallback must succeed");
        assert_eq!(resolved, legacy, "legacy string returned as-is");
    }

    #[test]
    fn handle_id_or_legacy_errors_loud_when_neither_shape_provided() {
        let req = CommandRequest::new(CursorParams::default());
        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect_err("empty request must Err");
        assert!(
            err.contains("data/query-next"),
            "error must name the failing command surface: {err}"
        );
        assert!(
            err.contains("`handle`") && err.contains("`queryId`"),
            "error must name BOTH supported shapes so caller knows what to add: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_prepends_command_name_to_handle_validation_errors() {
        // Critical for diagnostics: when a wrong-owner handle reaches
        // this resolver, the error must name BOTH the failing command
        // (so the caller knows which surface) AND the
        // HandleRef-level mismatch (so the caller knows what to fix).
        let req = CommandRequest::new(CursorParams::default())
            .with_handle(HandleRef::mint("chat", "chat::MessageHandle"));

        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .expect_err("wrong-owner handle must Err");
        assert!(
            err.starts_with("data/query-next:"),
            "command name must prefix the error: {err}"
        );
        assert!(
            err.contains("owner mismatch"),
            "HandleRef's failure mode must propagate: {err}"
        );
        assert!(
            err.contains("\"chat\"") && err.contains("\"data\""),
            "both offender and expected named: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_propagates_type_mismatch_with_command_name() {
        let req = CommandRequest::new(CursorParams::default())
            .with_handle(HandleRef::mint("data", "data::Migration"));

        let err = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-close",
            )
            .expect_err("wrong-type handle must Err");
        assert!(
            err.starts_with("data/query-close:"),
            "command prefix: {err}"
        );
        assert!(
            err.contains("type mismatch"),
            "type mismatch propagates: {err}"
        );
        assert!(
            err.contains("data::Migration") && err.contains("data::QueryCursor"),
            "both offender and expected named: {err}"
        );
    }

    #[test]
    fn handle_id_or_legacy_uses_canonical_uuid_string_for_handle_path() {
        // The envelope path must produce the UUID's canonical string
        // form (not some other rendering), so downstream consumers
        // can use the resolved string as a stable cache key with
        // legacy-path values from the same migration window.
        let id = Uuid::new_v4();
        let req = CommandRequest::new(CursorParams::default()).with_handle(cursor_handle(id));
        let resolved = req
            .handle_id_or_legacy(
                "data",
                "data::QueryCursor",
                "queryId",
                &req.params.query_id,
                "data/query-next",
            )
            .unwrap();
        assert_eq!(
            resolved,
            id.to_string(),
            "canonical UUID string is the bridge format between handle and legacy paths"
        );
    }
    // what this catches (card ea28d2f6): a registered command whose PARAMS declare a wire
    // name the envelope owns. `params` is flattened into the same JSON object as
    // `handle` / `sessionId` / …, so serde hands the envelope its named fields first and
    // the params never see them. Measured through the real boundary, 2026-09-21:
    // `CommandRequest::<AgentStatusParams>::from_value({"handle":"agent-abc"})` →
    // `invalid type: string "agent-abc", expected struct HandleRef` — three public
    // agent verbs uncallable, a data/* string handle refused (hence `dbPath`), and four
    // renames-at-the-site before anyone wrote this. Walks every registered command's
    // params schema — PARAMS only: an outcome struct never traverses the request
    // envelope, so `JobCreateResult.handle` is correct and must not be flagged.
    #[test]
    fn no_registered_commands_params_schema_names_a_field_the_envelope_owns() {
        let mut collisions: Vec<String> = Vec::new();
        for d in crate::sdk_codegen::command_registry() {
            let Some(props) = d.params_schema.get("properties").and_then(|p| p.as_object()) else {
                continue;
            };
            for key in props.keys() {
                if is_envelope_field(key) {
                    collisions.push(format!("{} — params.{key}", d.name));
                }
            }
        }
        collisions.sort();
        assert!(
            collisions.is_empty(),
            "the envelope owns these names; a params field spelled the same is claimed by \
             CommandRequest and never reaches the handler. Rename the param (jobHandle, \
             agentHandle, dbPath — see #4326) or drop it. Collisions: {collisions:#?}"
        );
    }

