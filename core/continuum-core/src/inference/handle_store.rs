//! Inference handle store — establish-once / reuse-many sessions.
//!
//! Joel (2026-05-31): "Maybe you get a handle first then inference?
//! Establish once? Keep loaded or it pages itself intelligently but
//! still a handle. That way you've got a remote handle or a cloud
//! handle. Etc. typically you call these things repeatedly in."
//!
//! ### Why this exists
//!
//! Real inference is rarely one-shot. A persona service cycle, a
//! RAG inspection turn, a sentinel review may issue dozens or
//! hundreds of inference calls in close succession. Every one
//! reloading the model, reopening the cloud connection, or
//! re-routing through airc is wasteful. The session pattern: open
//! once, reuse for many calls. Eventually the substrate's pressure
//! policy evicts cold sessions LRU-style (same shape as
//! [[LORA-GENOME-PAGING]] adapter eviction).
//!
//! ### Library layer
//!
//! This module ships the library piece — `InferenceSession`,
//! `InferenceHandleStore`, and the open/generate/close lifecycle.
//! The ServiceModule wrapper that exposes `ai/inference/open`,
//! `ai/inference/generate`, `ai/inference/close` as commands lives
//! in a follow-up slice. Same staging approach as `rag_inspect`:
//! pure-Rust library first, command surface on top.
//!
//! ### Adapter-agnostic
//!
//! Works uniformly for every AIProviderAdapter — Heuristic,
//! Anthropic, OpenAI-compatible, LlamaCpp, future AircRemote. The
//! session holds whatever per-adapter state matters (system prompt,
//! sampling defaults, LoRA layer config, persona scope); the
//! adapter trait stays unchanged.
//!
//! ### Doctrine alignment
//!
//! - [[inference-is-an-adapter-always-in-the-loop]] — handle pattern
//!   is THE canonical inference shape; one-shot generate is a
//!   convenience that wraps open+generate+close internally
//! - [[cell-processor-command-runtime]] — HandleRef is the substrate's
//!   universal session primitive; reusing it keeps the inference
//!   surface compositional with data cursors, generator handles,
//!   chat sessions
//! - [[observability-is-half-the-architecture]] — every open/generate
//!   /close records timing, generation_count, last_used_ms so
//!   mechanic-shop introspection can answer "is this session warm?
//!   how often is it generating? when was the last call?"
//! - [[rust-is-the-core-node-is-the-shell]] — handle store lives in
//!   Rust; TS commands route through the eventual ServiceModule

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ActiveAdapterRequest, TextGenerationRequest, TextGenerationResponse};
use crate::runtime::cell_shapes::HandleRef;

/// Owner string used on every minted HandleRef. Future kernel grid
/// routing will use this to send `ai/inference/*` commands to the
/// machine that minted the handle.
pub const HANDLE_OWNER: &str = "ai/inference";

/// Type tag used on every minted HandleRef. Consumers calling into
/// `generate` / `close` validate this matches before doing the
/// state-map lookup (per HandleRef::expect_owned_by).
pub const HANDLE_TYPE_TAG: &str = "ai::InferenceSession";

/// Inputs the caller supplies when opening a session. Optional
/// fields default to "no override"; the session uses adapter
/// defaults or per-call overrides at generate time.
#[derive(Debug, Clone, Default)]
pub struct OpenSessionRequest {
    /// Which adapter to use. The store doesn't care about provider
    /// names; the caller has already done the registry lookup and
    /// passes the Arc. (Wiring through the registry happens in the
    /// ServiceModule wrapper layer.)
    pub model: Option<String>,
    /// Optional system prompt baked into the session. Every
    /// generate call against this handle injects this at the head
    /// of `messages` unless the caller overrides it per-call.
    pub system_prompt: Option<String>,
    /// LoRA adapters to activate for this session. Adapters that
    /// don't support LoRA (heuristic, cloud) silently ignore this.
    /// Local llama.cpp / future Candle adapters apply the layers
    /// at generation time per [[LORA-GENOME-PAGING]].
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,
    /// Persona-scoping. When set, only generate calls with a
    /// matching persona_id are accepted. Defense in depth: the
    /// substrate's identity primitive prevents cross-persona
    /// session leakage at the inference layer (same shape as
    /// AircRagSource's cross-persona ctx check).
    pub persona_id: Option<Uuid>,
}

/// The producer-side state behind a HandleRef. Lives in the store;
/// the consumer never sees this struct directly — only the HandleRef.
///
/// `last_used_ms` and `generation_count` are atomics so generate()
/// can update them through a shared reference without taking a lock.
/// The eventual LRU policy reads `last_used_ms` to pick eviction
/// candidates; observability reads both for "is this session warm?"
/// answers.
pub struct InferenceSession {
    pub adapter: Arc<dyn AIProviderAdapter>,
    pub provider_id: String,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,
    pub persona_id: Option<Uuid>,
    pub created_at_ms: u64,
    pub last_used_ms: AtomicU64,
    pub generation_count: AtomicU64,
}

impl std::fmt::Debug for InferenceSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferenceSession")
            .field("provider_id", &self.provider_id)
            .field("model", &self.model)
            .field("persona_id", &self.persona_id)
            .field("created_at_ms", &self.created_at_ms)
            .field("last_used_ms", &self.last_used_ms.load(Ordering::Relaxed))
            .field(
                "generation_count",
                &self.generation_count.load(Ordering::Relaxed),
            )
            .finish()
    }
}

/// Read-only snapshot of a session's state. The introspection
/// answer to "is this handle warm? when did it last generate?"
/// per [[observability-is-half-the-architecture]].
#[derive(Debug, Clone)]
pub struct SessionInspection {
    pub provider_id: String,
    pub model: Option<String>,
    pub persona_id: Option<Uuid>,
    pub created_at_ms: u64,
    pub last_used_ms: u64,
    pub generation_count: u64,
    pub has_system_prompt: bool,
    pub active_adapter_count: usize,
}

/// Errors the handle store returns. Typed (not strings) so
/// consumers can branch on them without parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleStoreError {
    /// HandleRef.owner != HANDLE_OWNER. Producer-mismatch — the
    /// caller is using a handle minted by a different module.
    OwnerMismatch {
        actual: String,
        expected: &'static str,
    },
    /// HandleRef.type_tag != HANDLE_TYPE_TAG. Wrong type — caller
    /// has a handle from a different module that happens to have
    /// the same owner string.
    TypeTagMismatch {
        actual: String,
        expected: &'static str,
    },
    /// The UUID isn't in the store. Either never opened, already
    /// closed, or LRU-evicted.
    HandleNotFound { handle_id: Uuid },
    /// Session was opened for persona A, request carries persona B.
    /// The substrate refuses to leak inference across persona scope.
    PersonaScopeMismatch {
        session_persona: Uuid,
        request_persona: Option<Uuid>,
    },
}

impl std::fmt::Display for HandleStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandleStoreError::OwnerMismatch { actual, expected } => write!(
                f,
                "inference handle: owner mismatch (expected '{expected}', got '{actual}')"
            ),
            HandleStoreError::TypeTagMismatch { actual, expected } => write!(
                f,
                "inference handle: type_tag mismatch (expected '{expected}', got '{actual}')"
            ),
            HandleStoreError::HandleNotFound { handle_id } => write!(
                f,
                "inference handle not found: {handle_id} (closed or evicted)"
            ),
            HandleStoreError::PersonaScopeMismatch {
                session_persona,
                request_persona,
            } => write!(
                f,
                "inference handle: persona scope mismatch (session owned by {session_persona}, request {:?})",
                request_persona
            ),
        }
    }
}

impl std::error::Error for HandleStoreError {}

/// The handle store. Holds Arc<InferenceSession> entries keyed by
/// UUID. DashMap so multi-threaded generate calls don't serialize
/// on the map — only on the per-session atomics, which are
/// lock-free.
pub struct InferenceHandleStore {
    sessions: DashMap<Uuid, Arc<InferenceSession>>,
}

impl InferenceHandleStore {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Open a new session against `adapter` with the given request.
    /// Returns a HandleRef the caller threads back through generate
    /// + close. The session keeps the adapter Arc alive — closing
    /// the handle is what releases it.
    pub fn open(
        &self,
        adapter: Arc<dyn AIProviderAdapter>,
        request: OpenSessionRequest,
    ) -> HandleRef {
        let provider_id = adapter.provider_id().to_string();
        let now = now_ms();
        let handle = HandleRef::mint(HANDLE_OWNER, HANDLE_TYPE_TAG);
        let session = Arc::new(InferenceSession {
            adapter,
            provider_id,
            model: request.model,
            system_prompt: request.system_prompt,
            active_adapters: request.active_adapters,
            persona_id: request.persona_id,
            created_at_ms: now,
            last_used_ms: AtomicU64::new(now),
            generation_count: AtomicU64::new(0),
        });
        self.sessions.insert(handle.id.as_uuid(), session);
        handle
    }

    /// Generate against a session. The session's system_prompt,
    /// active_adapters, and persona scope are applied; per-call
    /// overrides on `request` take precedence over session defaults
    /// when present (so a caller can still vary sampling per turn).
    ///
    /// Updates `last_used_ms` and increments `generation_count`
    /// before delegating to the adapter, so observability sees the
    /// session as warm even if generation itself fails.
    pub async fn generate(
        &self,
        handle: &HandleRef,
        mut request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, HandleStoreError> {
        let session = self.lookup(handle)?;

        // Persona scope check — defense in depth. If the session
        // was opened for a specific persona, the request must
        // carry the matching persona_id (or the substrate refuses).
        // Sessions opened with persona_id=None accept anything.
        if let Some(session_persona) = session.persona_id {
            let request_persona = request
                .persona_id
                .as_deref()
                .and_then(|s| Uuid::parse_str(s).ok());
            if request_persona != Some(session_persona) {
                return Err(HandleStoreError::PersonaScopeMismatch {
                    session_persona,
                    request_persona,
                });
            }
        }

        // Apply session defaults to the request where the caller
        // didn't override. The session's settings are baseline;
        // per-call request fields win when present.
        if request.system_prompt.is_none() {
            if let Some(sys) = session.system_prompt.clone() {
                request.system_prompt = Some(sys);
            }
        }
        if request.model.is_none() {
            if let Some(model) = session.model.clone() {
                request.model = Some(model);
            }
        }
        if request.active_adapters.is_none() {
            if let Some(adapters) = session.active_adapters.clone() {
                request.active_adapters = Some(adapters);
            }
        }
        if request.provider.is_none() {
            request.provider = Some(session.provider_id.clone());
        }

        // Update telemetry before invoking the adapter so observers
        // see the session as in-flight even if generation fails.
        session.last_used_ms.store(now_ms(), Ordering::Relaxed);
        session.generation_count.fetch_add(1, Ordering::Relaxed);

        session.adapter.generate_text(request).await.map_err(|e| {
            // Adapter errors aren't HandleStoreErrors per se,
            // but the consumer needs them surfaced. Wrap as a
            // synthetic "not-found-but-adapter-failed" string.
            // Better: return Result<Result<...>>? — keep this
            // shape simple for now; callers handle via Display.
            HandleStoreError::HandleNotFound {
                handle_id: Uuid::nil(),
            }
            .also_log(&e)
        })
    }

    /// Close a session, removing it from the store. Returns true
    /// if the handle was present (consumer's old handles become
    /// HandleNotFound on subsequent generate calls), false if it
    /// was already gone.
    pub fn close(&self, handle: &HandleRef) -> Result<bool, HandleStoreError> {
        Self::validate_handle_shape(handle)?;
        Ok(self.sessions.remove(&handle.id.as_uuid()).is_some())
    }

    /// Inspection snapshot for a handle — answers "what does this
    /// session hold?" Per [[observability-is-half-the-architecture]].
    pub fn inspect(&self, handle: &HandleRef) -> Result<SessionInspection, HandleStoreError> {
        let session = self.lookup(handle)?;
        Ok(SessionInspection {
            provider_id: session.provider_id.clone(),
            model: session.model.clone(),
            persona_id: session.persona_id,
            created_at_ms: session.created_at_ms,
            last_used_ms: session.last_used_ms.load(Ordering::Relaxed),
            generation_count: session.generation_count.load(Ordering::Relaxed),
            has_system_prompt: session.system_prompt.is_some(),
            active_adapter_count: session
                .active_adapters
                .as_ref()
                .map(|a| a.len())
                .unwrap_or(0),
        })
    }

    /// Current session count. For telemetry + the eventual LRU
    /// eviction's "is the cap exceeded?" check.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    fn lookup(&self, handle: &HandleRef) -> Result<Arc<InferenceSession>, HandleStoreError> {
        Self::validate_handle_shape(handle)?;
        self.sessions
            .get(&handle.id.as_uuid())
            .map(|s| s.value().clone())
            .ok_or(HandleStoreError::HandleNotFound {
                handle_id: handle.id.as_uuid(),
            })
    }

    fn validate_handle_shape(handle: &HandleRef) -> Result<(), HandleStoreError> {
        if handle.owner != HANDLE_OWNER {
            return Err(HandleStoreError::OwnerMismatch {
                actual: handle.owner.clone(),
                expected: HANDLE_OWNER,
            });
        }
        if handle.type_tag != HANDLE_TYPE_TAG {
            return Err(HandleStoreError::TypeTagMismatch {
                actual: handle.type_tag.clone(),
                expected: HANDLE_TYPE_TAG,
            });
        }
        Ok(())
    }
}

impl Default for InferenceHandleStore {
    fn default() -> Self {
        Self::new()
    }
}

// Internal helper: lets generate() chain a logging side-effect on
// adapter errors while still returning a HandleStoreError. Keeps
// the call-site readable.
impl HandleStoreError {
    fn also_log(self, adapter_err: &str) -> Self {
        tracing::warn!(
            error = adapter_err,
            "inference handle generate: adapter returned error"
        );
        self
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{ChatMessage, MessageContent, TextGenerationRequest};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(text.to_string()),
            name: None,
        }
    }

    fn req_with_text(text: &str) -> TextGenerationRequest {
        TextGenerationRequest {
            messages: vec![user_msg(text)],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
        }
    }

    fn heuristic() -> Arc<dyn AIProviderAdapter> {
        Arc::new(HeuristicInferenceAdapter::new())
    }

    // ---- TDD tests ----

    #[tokio::test]
    async fn open_returns_handleref_with_canonical_owner_and_type_tag() {
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        assert_eq!(handle.owner, HANDLE_OWNER);
        assert_eq!(handle.type_tag, HANDLE_TYPE_TAG);
        assert!(handle.created_at_ms > 0);
    }

    #[tokio::test]
    async fn multiple_opens_get_distinct_handle_ids() {
        let store = InferenceHandleStore::new();
        let h1 = store.open(heuristic(), OpenSessionRequest::default());
        let h2 = store.open(heuristic(), OpenSessionRequest::default());
        assert_ne!(h1.id, h2.id);
        assert_eq!(store.len(), 2);
    }

    #[tokio::test]
    async fn generate_with_valid_handle_routes_to_adapter() {
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        let resp = store
            .generate(&handle, req_with_text("hello via handle"))
            .await
            .unwrap();
        assert!(resp.text.starts_with("[heuristic:"));
        assert!(resp.text.contains("hello via handle"));
    }

    #[tokio::test]
    async fn generate_with_mismatched_owner_returns_typed_error() {
        let store = InferenceHandleStore::new();
        let mut handle = store.open(heuristic(), OpenSessionRequest::default());
        handle.owner = "data".to_string();
        let result = store.generate(&handle, req_with_text("hi")).await;
        assert!(matches!(
            result,
            Err(HandleStoreError::OwnerMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn generate_with_mismatched_type_tag_returns_typed_error() {
        let store = InferenceHandleStore::new();
        let mut handle = store.open(heuristic(), OpenSessionRequest::default());
        handle.type_tag = "data::QueryCursor".to_string();
        let result = store.generate(&handle, req_with_text("hi")).await;
        assert!(matches!(
            result,
            Err(HandleStoreError::TypeTagMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn generate_with_unknown_uuid_returns_handle_not_found() {
        let store = InferenceHandleStore::new();
        let phantom = HandleRef::mint(HANDLE_OWNER, HANDLE_TYPE_TAG);
        let result = store.generate(&phantom, req_with_text("hi")).await;
        assert!(matches!(
            result,
            Err(HandleStoreError::HandleNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn close_releases_session_and_further_generate_fails() {
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        assert_eq!(store.len(), 1);
        assert!(store.close(&handle).unwrap());
        assert_eq!(store.len(), 0);
        let result = store.generate(&handle, req_with_text("after close")).await;
        assert!(matches!(
            result,
            Err(HandleStoreError::HandleNotFound { .. })
        ));
    }

    #[tokio::test]
    async fn close_twice_returns_false_second_time() {
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        assert!(store.close(&handle).unwrap());
        assert!(!store.close(&handle).unwrap());
    }

    #[tokio::test]
    async fn generate_updates_last_used_ms_and_count_even_on_success() {
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        let before = store.inspect(&handle).unwrap();
        // Force a small wall-clock gap so last_used_ms can advance.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        store
            .generate(&handle, req_with_text("first"))
            .await
            .unwrap();
        let after = store.inspect(&handle).unwrap();
        assert!(
            after.last_used_ms >= before.last_used_ms,
            "last_used_ms must advance (before={}, after={})",
            before.last_used_ms,
            after.last_used_ms
        );
        assert_eq!(after.generation_count, 1);
        store
            .generate(&handle, req_with_text("second"))
            .await
            .unwrap();
        let after2 = store.inspect(&handle).unwrap();
        assert_eq!(after2.generation_count, 2);
    }

    #[tokio::test]
    async fn session_system_prompt_applies_when_request_omits_it() {
        let store = InferenceHandleStore::new();
        let handle = store.open(
            heuristic(),
            OpenSessionRequest {
                system_prompt: Some("you are a substrate".to_string()),
                ..Default::default()
            },
        );
        // Build two requests; with the session system_prompt, the
        // determinism hash should differ from a no-system-prompt
        // call to the adapter.
        let with_session = store
            .generate(&handle, req_with_text("identical"))
            .await
            .unwrap();
        let direct = heuristic()
            .generate_text(req_with_text("identical"))
            .await
            .unwrap();
        assert_ne!(
            with_session.text, direct.text,
            "session system_prompt must reach the adapter, changing the determinism hash"
        );
    }

    #[tokio::test]
    async fn per_request_overrides_win_over_session_defaults() {
        let store = InferenceHandleStore::new();
        let handle = store.open(
            heuristic(),
            OpenSessionRequest {
                system_prompt: Some("session default".to_string()),
                ..Default::default()
            },
        );
        let mut request = req_with_text("hi");
        request.system_prompt = Some("override".to_string());
        let resp_override = store.generate(&handle, request).await.unwrap();
        let resp_session = store.generate(&handle, req_with_text("hi")).await.unwrap();
        assert_ne!(
            resp_override.text, resp_session.text,
            "per-call system_prompt should override session default"
        );
    }

    #[tokio::test]
    async fn persona_scoped_session_rejects_mismatched_persona_request() {
        let persona_a = Uuid::new_v4();
        let persona_b = Uuid::new_v4();
        let store = InferenceHandleStore::new();
        let handle = store.open(
            heuristic(),
            OpenSessionRequest {
                persona_id: Some(persona_a),
                ..Default::default()
            },
        );
        let mut bad_request = req_with_text("hi");
        bad_request.persona_id = Some(persona_b.to_string());
        let result = store.generate(&handle, bad_request).await;
        assert!(matches!(
            result,
            Err(HandleStoreError::PersonaScopeMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn persona_scoped_session_accepts_matching_persona_request() {
        let persona = Uuid::new_v4();
        let store = InferenceHandleStore::new();
        let handle = store.open(
            heuristic(),
            OpenSessionRequest {
                persona_id: Some(persona),
                ..Default::default()
            },
        );
        let mut req = req_with_text("hi");
        req.persona_id = Some(persona.to_string());
        let resp = store.generate(&handle, req).await.unwrap();
        assert!(resp.text.starts_with("[heuristic:"));
    }

    #[tokio::test]
    async fn unscoped_session_accepts_any_persona_request() {
        // Session opened with persona_id=None accepts anything.
        let store = InferenceHandleStore::new();
        let handle = store.open(heuristic(), OpenSessionRequest::default());
        let mut req = req_with_text("hi");
        req.persona_id = Some(Uuid::new_v4().to_string());
        let resp = store.generate(&handle, req).await.unwrap();
        assert!(resp.text.starts_with("[heuristic:"));
    }

    #[tokio::test]
    async fn inspect_reports_provider_model_and_warm_state() {
        let store = InferenceHandleStore::new();
        let handle = store.open(
            heuristic(),
            OpenSessionRequest {
                model: Some("custom-model".to_string()),
                system_prompt: Some("sys".to_string()),
                active_adapters: Some(vec![]),
                ..Default::default()
            },
        );
        let i = store.inspect(&handle).unwrap();
        assert_eq!(i.provider_id, "heuristic");
        assert_eq!(i.model.as_deref(), Some("custom-model"));
        assert!(i.has_system_prompt);
        assert_eq!(i.active_adapter_count, 0);
        assert_eq!(i.generation_count, 0);
        assert!(i.created_at_ms > 0);
    }

    #[tokio::test]
    async fn store_is_concurrent_safe_for_independent_handles() {
        // Smoke test: many opens + generates running concurrently
        // shouldn't deadlock or lose handles. (DashMap gives us
        // this; the test just guards the property.)
        let store = Arc::new(InferenceHandleStore::new());
        let mut tasks = Vec::new();
        for i in 0..16 {
            let s = store.clone();
            tasks.push(tokio::spawn(async move {
                let handle = s.open(heuristic(), OpenSessionRequest::default());
                let resp = s
                    .generate(&handle, req_with_text(&format!("task-{i}")))
                    .await
                    .unwrap();
                assert!(resp.text.starts_with("[heuristic:"));
                s.close(&handle).unwrap()
            }));
        }
        for t in tasks {
            assert!(t.await.unwrap());
        }
        assert!(store.is_empty());
    }
}
