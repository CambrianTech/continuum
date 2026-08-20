//! Cell return shapes per [MODULE-ARCHITECTURE.md §5.1](../../../../../docs/architecture/MODULE-ARCHITECTURE.md).
//!
//! A command returns one of four cell shapes. Today's `CommandResult`
//! enum is the in-process Rust embodiment of those four shapes:
//!
//! | Cell shape (architecture) | `CommandResult` variant | Status |
//! |---|---|---|
//! | `Value<T>` (immediate typed result) | `Json(Value)` + `Binary { metadata, data }` | Mainline; back-compat |
//! | `Handle<T>` (typed ref to state owned by producer) | `Handle(HandleRef)` | **Lands in this PR** |
//! | `Stream<T>` (async sequence of values) | `Stream(StreamPlaceholder)` | Reserved variant; returning it errors until the wire protocol lands |
//! | `Lambda<P, T>` (callable returned by a command) | `Lambda(LambdaPlaceholder)` | Reserved variant; returning it errors until the lambda protocol lands |
//!
//! The Json + Binary variants ARE the Value cell shape under the
//! taxonomy; they're kept under their original names so the 300+
//! existing command handlers don't need to change. New code that
//! produces a plain typed result should still use `CommandResult::Json`
//! (or `CommandResult::json(&value)?`). The Value name in the
//! architecture doc is the categorical name; the implementation name
//! stays Json for back-compat.
//!
//! # Why Handle is the headline shape
//!
//! Handle is the cell answer to MODULE-ARCHITECTURE.md §13.1 (hot-path
//! cross-module state). A module produces a handle to its internal
//! state; downstream commands take the handle as a param; the kernel
//! routes those calls back to the producing module (whose handler
//! looks up the state under the handle's `id`). No state copy, no
//! lock contention across modules, same primitive locally as
//! cross-machine. The producer owns; consumers compose by reference.
//!
//! The kernel does NOT need a global handle registry — each producing
//! module manages the lifetime of its own handles internally (typed
//! state map under the handle's `id`). The kernel sees a Handle the
//! same as any other JSON payload; routing happens through the normal
//! `Commands.execute(target/op, { handle })` path. The Handle struct
//! is purely a data shape that travels through the existing primitive.
//!
//! # The canonical use cases (per Joel 2026-05-30)
//!
//! Handles are for **long-running stateful work** where the first call
//! produces a handle and subsequent calls operate on it:
//!
//! - **inference** — `ai/inference/start { model, prompt }` returns a
//!   handle; later `ai/inference/poll { handle }` and
//!   `ai/inference/cancel { handle }` operate on the running session.
//! - **training** — `training/run/start { recipe }` returns a handle;
//!   `training/run/progress { handle }`, `training/run/cancel { handle }`
//!   query and control the run.
//! - **hosting** — `live/room/join { roomId }` returns a handle;
//!   `live/audio/publish { handle, frame }` operates on the joined
//!   session.
//! - **ORM** — `data/transaction/begin` returns a handle;
//!   `data/transaction/exec { handle, query }` and
//!   `data/transaction/commit { handle }` thread the same transaction.
//!
//! All IDs are UUIDs. The producer mints a UUID, stores its state under
//! that UUID, returns the handle. Subsequent calls carry the UUID; the
//! producer's handler does an O(1) map lookup. The pattern works the
//! same whether the producer runs in-process, in a sibling module, or
//! on a remote peer over grid/airc.

use super::handle::Handle;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Typed reference to state owned by a specific module.
///
/// # Round-trip
///
/// 1. Producer command (e.g., `chat/send`) creates internal state
///    (a message buffer, a session, a render context). It allocates a
///    handle ID, stores the state under that ID in its own state map,
///    and returns `CommandResult::Handle(HandleRef { owner: "chat",
///    id, type_tag: "chat::MessageHandle", created_at_ms })`.
///
/// 2. Caller (Rust, TS, or remote) holds the HandleRef opaquely. It
///    serializes through any wire crossing (it's plain JSON via serde).
///
/// 3. Caller invokes a downstream command that takes the handle:
///    `Commands.execute("chat/message/get", { handle })`. The kernel
///    routes to the chat module (`chat/` prefix in the registry); the
///    chat module reads the handle's `id` from params and looks up its
///    state map.
///
/// 4. Cross-module: if a different module needs to operate on the
///    handle's underlying state, it asks the owner via a command:
///    `Commands.execute("chat/message/get", { handle })` — same call,
///    routed to the owner. The kernel doesn't care which module asked.
///
/// # `type_tag` discipline
///
/// Convention: `"<module>::<TypeName>"` matching the Rust type that
/// produced the handle. e.g., `"chat::MessageHandle"`, `"rag::Slice"`,
/// `"persona::InboxFrame"`. Lets typed callers cast safely on receipt
/// without round-tripping through the producer.
///
/// # Lifetime
///
/// Producer owns the lifetime. The handle is valid as long as the
/// producer's state map holds the ID. Producers may evict handles
/// after a TTL, on session end, on resource pressure, etc. A consumer
/// holding a stale handle gets a typed error from the producer's
/// command handler (`"handle not found"`); the kernel doesn't
/// participate in lifetime management. This is intentional — the
/// kernel stays minimal, and lifetime policy belongs to the producer.
///
/// # Cross-machine
///
/// Same primitive. A handle minted on machine A is meaningful only on
/// machine A. If a consumer on machine B calls a command taking that
/// handle, the kernel's grid interceptor routes the call back to A
/// (the handle's `owner` lives there). The handle ID never leaves A's
/// state map; the remote call carries the ID, A executes the op
/// locally, returns the result.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/HandleRef.ts"
)]
pub struct HandleRef {
    /// Module that owns the state behind this handle. Kernel routes
    /// any command taking this handle through the module's registered
    /// command prefix (e.g., `"chat"` → commands under `chat/`).
    pub owner: String,

    /// UUID the owner module uses to look up its state. Always UUID
    /// (per Joel 2026-05-30 — no string IDs at the cell-shape level);
    /// the producer mints via [`HandleRef::mint`] (kernel chooses) or
    /// passes a pre-allocated UUID via [`HandleRef::with_id`] (producer
    /// chooses). Wire format is the UUID's canonical string serialization
    /// so ts-rs sees it as `string`.
    #[ts(type = "string")]
    pub id: Handle,

    /// Type tag identifying the state shape. Convention:
    /// `"<module>::<TypeName>"`. Lets typed consumers cast safely
    /// without asking the owner.
    pub type_tag: String,

    /// Milliseconds since unix epoch when the handle was minted.
    /// Useful for TTL enforcement (producer's choice) and for
    /// diagnostic ordering.
    #[ts(type = "number")]
    pub created_at_ms: u64,
}

impl HandleRef {
    /// Construct a HandleRef from a pre-allocated UUID. Use this when
    /// the producer needs to know the UUID up front — e.g., when
    /// inserting state into its map under a specific key:
    ///
    /// ```ignore
    /// let id = Uuid::new_v4();
    /// self.sessions.insert(id, session_state);
    /// Ok(CommandResult::Handle(HandleRef::with_id("ai/inference", id, "ai::InferenceSession")))
    /// ```
    pub fn with_id(owner: impl Into<String>, id: Uuid, type_tag: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            id: id.into(),
            type_tag: type_tag.into(),
            created_at_ms: now_ms(),
        }
    }

    /// Construct a HandleRef with a fresh UUID. Convenience wrapper
    /// around [`Self::with_id`] for producers that don't need to know
    /// the UUID before they construct the handle:
    ///
    /// ```ignore
    /// let handle = HandleRef::mint("ai/inference", "ai::InferenceSession");
    /// self.sessions.insert(handle.id, session_state);
    /// Ok(CommandResult::Handle(handle))
    /// ```
    pub fn mint(owner: impl Into<String>, type_tag: impl Into<String>) -> Self {
        Self::with_id(owner, Uuid::new_v4(), type_tag)
    }

    /// Validate this handle's `owner` and `type_tag` match the values
    /// the consumer expects, returning the inner `Uuid` for the
    /// consumer's own state-map lookup.
    ///
    /// This is the canonical handle-validation entry point — every
    /// handler that consumes a `HandleRef` should call it before
    /// looking the id up in its state map, so:
    ///
    /// - A handle minted by a different module reaching the wrong
    ///   handler surfaces a typed "owner mismatch" error rather than
    ///   silently miss-looking-up in the wrong state map. The grid
    ///   interceptor is supposed to route by `owner` before dispatch
    ///   ever fires; an owner-mismatch reaching this far means the
    ///   routing misfired or a caller hand-crafted a bogus handle.
    ///
    /// - A handle for the wrong resource (right module, wrong type —
    ///   e.g. a `data::Migration` handle threaded through a cursor
    ///   handler) surfaces a typed "type mismatch" error rather than
    ///   miss-looking-up across handle shapes.
    ///
    /// Errors are formatted consistently across every module that
    /// uses handles, naming BOTH the offending value AND the expected
    /// value so the caller self-corrects without grepping source.
    /// Consumers typically prepend their command name via `map_err`:
    ///
    /// ```ignore
    /// let cursor_id = handle.expect_owned_by("data", "data::QueryCursor")
    ///     .map_err(|e| format!("data/query-next: {e}"))?;
    /// ```
    ///
    /// For dual-shape resolvers that accept EITHER a typed handle
    /// (envelope) OR a legacy string field (back-compat during
    /// migration), prefer
    /// [`crate::runtime::CommandRequest::handle_id_or_legacy`] which
    /// composes this method with the legacy fallback path and the
    /// command-name prefix in a single call.
    pub fn expect_owned_by(
        &self,
        expected_owner: &str,
        expected_type_tag: &str,
    ) -> Result<Uuid, String> {
        if self.owner != expected_owner {
            return Err(format!(
                "handle owner mismatch — got owner={:?}, expected {:?}. \
                 Handles must be minted by the same module that consumes them, \
                 OR the grid interceptor must route the command back to the owner \
                 before local dispatch.",
                self.owner, expected_owner
            ));
        }
        if self.type_tag != expected_type_tag {
            return Err(format!(
                "handle type mismatch — got type_tag={:?}, expected {:?}. \
                 This handler operates only on handles of the expected type; \
                 threading a different handle shape here is a programming error.",
                self.type_tag, expected_type_tag
            ));
        }
        Ok(self.id.as_uuid())
    }
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Reserved: streaming result. **Returning a Stream result today is a
/// runtime error.** The variant exists so the enum's shape is fixed
/// before handlers begin migrating; the wire protocol (frame format,
/// correlation IDs, backpressure, cancellation) is the open piece.
///
/// When the protocol lands, `correlation_id` will tie incoming stream
/// frames to this stream so the consumer can match. The struct is
/// `#[non_exhaustive]` so adding fields later is non-breaking for
/// external code; internal code uses [`StreamPlaceholder::new`] to
/// construct rather than the field-init shorthand.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/StreamPlaceholder.ts"
)]
#[non_exhaustive]
pub struct StreamPlaceholder {
    /// Correlation ID a future wire protocol will use to tie incoming
    /// stream frames to this stream handle. Today: unused; reserved.
    pub correlation_id: String,
}

impl StreamPlaceholder {
    /// Construct a placeholder. The kernel and consumer will use
    /// `correlation_id` once the streaming protocol is designed; until
    /// then, callers should NOT return this variant — the executor
    /// rejects it via [`super::CommandResult::stream_protocol_error`].
    pub fn new(correlation_id: impl Into<String>) -> Self {
        Self {
            correlation_id: correlation_id.into(),
        }
    }
}

/// Reserved: lambda (callable returned by a command). **Returning a
/// Lambda result today is a runtime error.** Same status as
/// [`StreamPlaceholder`]: variant exists, in-process + wire shapes are
/// deferred.
///
/// When the protocol lands, a Lambda will be a curried command — name
/// + bound params + callsite metadata — that the caller invokes later
/// with remaining params via the kernel. Useful for setup commands
/// that prepare a context and return "now call THIS with the rest of
/// your input."
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/LambdaPlaceholder.ts"
)]
#[non_exhaustive]
pub struct LambdaPlaceholder {
    /// Name of the curried command the lambda will dispatch when
    /// invoked. e.g., `"ai/generate"`.
    pub command: String,
    /// Params already bound by the producer. The caller provides the
    /// remaining params; the kernel merges then dispatches.
    #[ts(type = "Record<string, unknown>")]
    pub bound_params: serde_json::Value,
}

impl LambdaPlaceholder {
    /// Construct a placeholder. Until the lambda protocol lands,
    /// callers should NOT return this variant — the executor rejects
    /// it via [`super::CommandResult::lambda_protocol_error`].
    pub fn new(command: impl Into<String>, bound_params: serde_json::Value) -> Self {
        Self {
            command: command.into(),
            bound_params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_ref_with_id_preserves_uuid() {
        let id = Uuid::new_v4();
        let h = HandleRef::with_id("ai/inference", id, "ai::InferenceSession");
        assert_eq!(
            h.id.as_uuid(),
            id,
            "with_id must preserve the producer-allocated UUID"
        );
        assert_eq!(h.owner, "ai/inference");
        assert_eq!(h.type_tag, "ai::InferenceSession");
        assert!(h.created_at_ms > 0, "constructor must capture a timestamp");
    }

    #[test]
    fn handle_ref_mint_generates_fresh_uuid() {
        let a = HandleRef::mint("ai/inference", "ai::InferenceSession");
        let b = HandleRef::mint("ai/inference", "ai::InferenceSession");
        assert_ne!(a.id, b.id, "mint must produce distinct UUIDs across calls");
    }

    #[test]
    fn handle_ref_roundtrips_through_json() {
        let h = HandleRef::mint("chat", "chat::MessageHandle");
        let json = serde_json::to_string(&h).expect("HandleRef must serialize");
        let back: HandleRef = serde_json::from_str(&json).expect("HandleRef must deserialize");
        assert_eq!(h, back);
        // Spot-check the UUID survives the round-trip.
        assert_eq!(
            h.id, back.id,
            "UUID must round-trip byte-identical through JSON"
        );
    }

    #[test]
    fn handle_ref_id_serializes_as_string() {
        // Per the ts-rs binding (`#[ts(type = "string")]`), the wire
        // form of `id` is the UUID's canonical string. Pin that
        // serde matches — ts-rs and serde agree on the shape so
        // TypeScript consumers can echo handles back as strings.
        let id = Uuid::new_v4();
        let h = HandleRef::with_id("chat", id, "chat::MessageHandle");
        let json: serde_json::Value = serde_json::to_value(&h).expect("HandleRef must serialize");
        let id_field = json.get("id").expect("id field present");
        assert!(
            id_field.is_string(),
            "id must serialize as JSON string (ts-rs sees it as `string`), got {id_field:?}"
        );
        assert_eq!(id_field.as_str().unwrap(), id.to_string());
    }

    #[test]
    fn handle_ref_owns_distinct_state() {
        // Two handles with the same owner + type but different UUIDs
        // represent different state — pin that they don't compare equal.
        let a = HandleRef::mint("chat", "chat::MessageHandle");
        let b = HandleRef::mint("chat", "chat::MessageHandle");
        assert_ne!(a, b, "handles with different UUIDs must not be equal");
    }

    #[test]
    fn stream_placeholder_roundtrips() {
        let s = StreamPlaceholder::new("corr-001");
        let json = serde_json::to_string(&s).expect("StreamPlaceholder must serialize");
        let back: StreamPlaceholder =
            serde_json::from_str(&json).expect("StreamPlaceholder must deserialize");
        assert_eq!(s, back);
        assert_eq!(back.correlation_id, "corr-001");
    }

    #[test]
    fn lambda_placeholder_roundtrips() {
        let l = LambdaPlaceholder::new("ai/generate", serde_json::json!({ "model": "qwen" }));
        let json = serde_json::to_string(&l).expect("LambdaPlaceholder must serialize");
        let back: LambdaPlaceholder =
            serde_json::from_str(&json).expect("LambdaPlaceholder must deserialize");
        assert_eq!(l, back);
        assert_eq!(back.command, "ai/generate");
        assert_eq!(back.bound_params["model"], "qwen");
    }

    // ── HandleRef::expect_owned_by ───────────────────────────────────
    //
    // The canonical validation entry point distilled from the data
    // module's first real HandleRef consumer (PR #1490). Every future
    // handler that consumes a HandleRef should reach for this method
    // rather than reimplementing the owner/type checks inline.

    #[test]
    fn expect_owned_by_returns_uuid_when_owner_and_type_match() {
        let id = Uuid::new_v4();
        let h = HandleRef::with_id("data", id, "data::QueryCursor");
        let resolved = h
            .expect_owned_by("data", "data::QueryCursor")
            .expect("matched handle must validate");
        assert_eq!(
            resolved, id,
            "expect_owned_by must return the inner UUID, not a string-rendered copy"
        );
    }

    #[test]
    fn expect_owned_by_rejects_wrong_owner_with_both_values_named() {
        let h = HandleRef::mint("chat", "chat::MessageHandle");
        let err = h
            .expect_owned_by("data", "data::QueryCursor")
            .expect_err("wrong owner must Err");
        assert!(
            err.contains("owner mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("\"chat\"") && err.contains("\"data\""),
            "error must name BOTH offender AND expected so caller self-corrects: {err}"
        );
    }

    #[test]
    fn expect_owned_by_rejects_wrong_type_tag_with_both_values_named() {
        let h = HandleRef::mint("data", "data::Migration");
        let err = h
            .expect_owned_by("data", "data::QueryCursor")
            .expect_err("wrong type must Err");
        assert!(
            err.contains("type mismatch"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("data::Migration") && err.contains("data::QueryCursor"),
            "error must name BOTH offender AND expected: {err}"
        );
    }

    #[test]
    fn expect_owned_by_checks_owner_first_then_type() {
        // Pin the order: owner mismatch should surface even when the
        // type tag is ALSO wrong. The owner-first check matters
        // because owner determines routing — type is a secondary
        // within-module discriminator.
        let h = HandleRef::mint("chat", "chat::MessageHandle");
        let err = h
            .expect_owned_by("data", "data::QueryCursor")
            .expect_err("both fields wrong must Err on the routing one first");
        assert!(
            err.contains("owner mismatch") && !err.contains("type mismatch"),
            "owner mismatch must take precedence over type mismatch: {err}"
        );
    }

    #[test]
    fn expect_owned_by_error_includes_routing_hint() {
        // The owner-mismatch error explicitly points consumers at the
        // grid interceptor's responsibility to route by owner — that's
        // the hint that turns "weird error" into "ah, the interceptor
        // is misconfigured" or "ah, this caller built a bogus handle".
        let h = HandleRef::mint("chat", "data::QueryCursor");
        let err = h
            .expect_owned_by("data", "data::QueryCursor")
            .expect_err("wrong owner must Err");
        assert!(
            err.contains("grid interceptor") || err.contains("route"),
            "owner-mismatch error must hint at routing semantics: {err}"
        );
    }
}
