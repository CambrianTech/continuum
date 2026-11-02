use super::*;

/// Adapter handle the chat module reads/writes against. `"main"` is the
/// kernel-wide convention for the primary continuum database — the
/// data module resolves it to either `$DATABASE_URL` (when set) or
/// `$HOME/.continuum/database/main.db` (the local SQLite default).
/// Centralized here so a future migration to per-room adapters is a
/// single-edit move.
pub(super) const CHAT_DATA_HANDLE: &str = "main";

/// The chat module. Owns the `chat/*` (and back-compat
/// `collaboration/chat/*`) command surface.
///
/// Stateless apart from an optional executor override used by tests to
/// inject a mocked dispatch chain — production wiring uses the global
/// kernel executor. The override lives behind an `RwLock<Option<...>>`
/// so it's set once at construction and read on the hot path; the
/// `RwLock` choice over `Mutex` is purely for read-side concurrency
/// when multiple commands fire concurrently.
pub struct ChatModule {
    /// Substrate-wide command executor. Installed by `start_server`
    /// via `install_executor` (the same path every other module uses).
    /// `with_executor` lets tests inject a custom chain without
    /// installing into the global registry.
    pub(super) executor_slot: Arc<LateBound<CommandExecutor>>,
}

/// Which wire leg a send takes after the data row is written. ONE leg per
/// line: the transcript is a log, and two rows for one utterance is a defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireLeg {
    /// Publish the chat-transcript envelope through `airc/realtime-publish`
    /// (the relay peer authors it; the sender rides `continuum.source_id`).
    Envelope,
    /// The caller speaks the line through the sender's own airc runtime
    /// (`publish_text_in_room`) — authored by the real peer, and the ONLY row.
    CallerSpeaks,
}

impl ChatModule {
    /// Construct a chat module. The executor is installed later by
    /// `start_server` via `ServiceModule::install_executor` (task #224).
    pub fn new() -> Self {
        Self {
            executor_slot: Arc::new(LateBound::new("chat::executor")),
        }
    }

    /// Test-only constructor — inject an explicit executor instance so
    /// the test owns its dispatch chain (commonly a registry with a
    /// stub DataModule). Lets the chat module's tests exercise the
    /// real cross-module call path without going through `start_server`.
    #[cfg(test)]
    pub fn with_executor(executor: Arc<CommandExecutor>) -> Self {
        let executor_slot = Arc::new(LateBound::new("chat::executor"));
        executor_slot.install(executor);
        Self { executor_slot }
    }

    /// Rebuild a chat module over an EXISTING late-bound executor slot.
    /// A `ChatModule` is stateless apart from that slot, so the typed
    /// `chat/*` [`ActionCommand`](crate::sdk_codegen::ActionCommand)s
    /// capture the slot at `commands()` time and reconstruct the module
    /// per call to reach the canonical [`poll`](Self::poll) /
    /// [`send`](Self::send) implementation — there is ONE body, shared by
    /// the command surface and the module's own tests. The slot is an
    /// `Arc`, so this is a pointer clone, and it stays the SAME slot
    /// `start_server` installs the executor into.
    pub(crate) fn from_slot(executor_slot: Arc<LateBound<CommandExecutor>>) -> Self {
        Self { executor_slot }
    }

    /// Resolve the executor for the current call. Returns a loud,
    /// contract-naming error if the executor was never installed — a
    /// boot-ordering bug (`start_server` must call
    /// `install_executor_on_all` BEFORE any chat command dispatches).
    /// Per [[no-fallbacks-ever]] the message still names the contract
    /// so the operator sees the real problem — but the blast radius is
    /// THIS request, not the whole core: a command that races boot
    /// fails loudly to its caller instead of `.expect()`-panicking the
    /// process and taking every other module down with it
    /// (#201 boot-race / #26 faculties degrade, never panic).
    pub(super) fn executor(&self) -> Result<Arc<CommandExecutor>, String> {
        self.executor_slot.cloned().ok_or_else(|| {
            "ChatModule: CommandExecutor not installed — \
             start_server must call install_executor_on_all \
             before any chat command can dispatch (task #224)"
                .to_string()
        })
    }

    /// Resolve a pagination anchor to its stored `timestamp` (the field
    /// both cursor directions filter on) via `data/query` (limit 1,
    /// filter on id). Fails loud when the anchor doesn't exist — silently
    /// returning unfiltered history would hand the caller the wrong page.
    pub(super) async fn anchor_timestamp(&self, anchor_id: Uuid) -> Result<String, String> {
        let anchor_query = json!({
            "dbPath": CHAT_DATA_HANDLE,
            "collection": CHAT_MESSAGES_COLLECTION,
            "filter": { "id": { "$eq": anchor_id.to_string() } },
            "limit": 1,
        });

        let anchor_result = self
            .executor()?
            .execute_json("data/query", anchor_query)
            .await
            .map_err(|e| format!("chat/poll: anchor lookup failed: {e}"))?;

        extract_first_record_field(&anchor_result, "timestamp").ok_or_else(|| {
            // Matches the TS impl's "Message not found" path.
            format!("chat/poll: anchor message not found: {anchor_id}")
        })
    }

    /// `chat/poll` — return recent messages, optionally filtered by
    /// room and anchored to a pagination cursor in EITHER direction.
    ///
    /// With no cursor: the newest `limit` rows. With `after_message_id` /
    /// `before_message_id`: the `limit` rows adjacent to that anchor, keyed by
    /// `(timestamp, id)` so consecutive pages neither overlap nor skip a row that
    /// shares the anchor's millisecond (see `keyset_page.rs`).
    /// Rows come back chronological whichever way the query ran.
    pub async fn poll(&self, params: ChatPollParams) -> Result<ChatPollResult, String> {
        let limit = params.limit.unwrap_or(DEFAULT_POLL_LIMIT);

        // The two anchors are opposite scroll directions — both at once
        // has no coherent ordering. Fail loud, never guess.
        let cursor = match (params.after_message_id, params.before_message_id) {
            (Some(_), Some(_)) => {
                return Err(
                    "chat/poll: afterMessageId and beforeMessageId are mutually exclusive — \
                     page one direction at a time"
                        .to_string(),
                )
            }
            (Some(anchor), None) => Some((anchor, PageDirection::Newer)),
            (None, Some(anchor)) => Some((anchor, PageDirection::Older)),
            (None, None) => None,
        };
        let mut messages = match cursor {
            Some((anchor, direction)) => self.keyset_page(params.room_id, anchor, direction, limit).await?,
            None => self.newest_page(params.room_id, limit).await?,
        };
        chronological(&mut messages);

        Ok(ChatPollResult {
            count: messages.len(),
            messages,
            after_message_id: params.after_message_id,
            before_message_id: params.before_message_id,
        })
    }

    /// `chat/send` — persist a chat message locally, then broadcast it.
    ///
    /// Two cross-module calls in sequence, NOT one merged write. The
    /// substrate has no built-in transaction across modules; this
    /// handler is the canonical demonstration of how to compose two
    /// effects with explicit partial-failure semantics.
    ///
    /// # Ordering: data first, airc second
    ///
    /// Local persistence is the ground truth. The reverse order would
    /// risk publishing a message to peers that this node doesn't know
    /// about — and a peer reading back that message would find no
    /// local record. With data-first, the worst case is *we have the
    /// message but peers don't* — a degradation, not a divergence.
    ///
    /// # Partial-failure semantics
    ///
    /// | data | airc | handler returns                                          |
    /// |------|------|----------------------------------------------------------|
    /// | ok   | ok   | `Ok(result with message_id + event_id)`                  |
    /// | ok   | fail | `Ok(result with message_id, event_id=None, warning=...)` |
    /// | fail | —    | `Err(...)` — no airc publish attempted                   |
    ///
    /// **An airc-only failure is NOT command-level failure.** The
    /// message IS stored locally; consumers see it via `chat/poll`.
    /// A future retry/sync mechanism heals the broadcast. Surfacing
    /// this as `Err` would tell the caller "your write didn't happen",
    /// which is wrong — half of the write did. The `warning` field is
    /// the right shape: degraded success.
    ///
    /// # Idempotency (known gap, deferred)
    ///
    /// A retried `chat/send` (network glitch on the caller side)
    /// currently produces two stored messages. This matches today's
    /// TS behavior and is out of scope for the first migration.
    /// Future PR can add a `client_dedup_id` param + a TTL'd map in
    /// the chat module; the substrate is ready for it (`HandleRef`
    /// could be the dedup id) but the design conversation is its
    /// own scope.
    pub async fn send(&self, params: ChatSendParams) -> Result<ChatSendResult, String> {
        self.send_with_wire(params, WireLeg::Envelope).await
    }

    /// `send` with the wire leg chosen by the caller. A caller that will SPEAK
    /// the line through the sender's own airc runtime (`chat/send` for a
    /// citizen, the operator self-peer, the agent peer) passes
    /// [`WireLeg::CallerSpeaks`]: the envelope leg is skipped, because both legs
    /// land in the daemon transcript and every citizen's window then read the
    /// operator twice (measured 2026-09-03: two rows, same sender, same
    /// timestamp; Kira's grounding held the line as items 8 AND 9). The data row
    /// is written either way — it is the ground truth the web's own poll reads.
    pub async fn send_with_wire(
        &self,
        params: ChatSendParams,
        wire: WireLeg,
    ) -> Result<ChatSendResult, String> {
        let (message_id, now_ms) = self.persist(&params).await?;
        match wire {
            WireLeg::Envelope => self.broadcast_envelope(message_id, &params, now_ms).await,
            WireLeg::CallerSpeaks => Ok(ChatSendResult {
                message_id,
                event_id: None,
                warning: None,
            }),
        }
    }

    /// Step 1 of a send: persist the message row (ground truth). Returns the
    /// minted message id and the timestamp the row carries.
    pub(super) async fn persist(&self, params: &ChatSendParams) -> Result<(Uuid, u64), String> {
        let message_id = Uuid::new_v4();
        let now_ms = continuum_base::time::EpochMs::now().millis();
        // Ground truth first: the row and its arrival commit together, or the send fails and no
        // airc publish is attempted (broadcasting a line the store lacks would diverge).
        let line = super::line_store::PostedLine {
            message_id,
            room: airc_core::RoomId::from_uuid(params.room_id),
            author: airc_core::PeerId::from_uuid(params.sender_id),
            kind: super::arrival::ArrivalKind::Message,
            direct_to: None,
            occurred_at_ms: now_ms,
            text: params.text.clone(),
            retires_publication: false,
        };
        let content = json!({ "text": params.text, "media": params.media });
        self.commit_line(
            &line,
            super::line_store::LineWrite::New,
            params.reply_to_id,
            content,
        )
        .await
        .map_err(|e| format!("chat/send: {e}"))?;
        Ok((message_id, now_ms))
    }

    /// Step 2 of a send: the ENVELOPE wire leg — the chat transcript schema
    /// published as an airc realtime envelope through `airc/realtime-publish`
    /// (best-effort; a miss is named in `warning`, never silent). Public so
    /// `chat/send` can fall back to it when the sender's own say fails.
    pub async fn broadcast_envelope(
        &self,
        message_id: Uuid,
        params: &ChatSendParams,
        now_ms: u64,
    ) -> Result<ChatSendResult, String> {
        let executor = self.executor()?;

        // ── Step 2: broadcast (best-effort) ─────────────────────────
        //
        // Build an AIRC realtime envelope carrying the chat
        // transcript schema. Construction stays at the wire-shape
        // level (json!) rather than importing the airc-realtime
        // typed structs — chat depends on airc through the command
        // surface, not through internal types. If airc changes its
        // wire shape, its `airc/realtime-publish` handler will
        // surface a parse error and the test
        // `send_envelope_matches_airc_publish_wire_shape` will
        // catch the drift.
        let publish_envelope = crate::cognition::conversation::room_event::chat_transcript_envelope(
            message_id,
            &params.transcript_line(),
            now_ms,
        );

        // Record the publication obligation BEFORE publishing (Codex on #5696): the exact envelope's
        // digest, the room and the sender, under the message id. The inbound seam delegates the line
        // to this sender only on an exact match, so the envelope published is the typed one digested.
        self.record_publication(message_id, params).await?;
        let publish_params = json!({ "envelope": publish_envelope });

        // Partial failure path: data succeeded, airc failed. Return
        // success with a warning naming what happened. The caller can
        // surface a UI warning, retry, or just log.
        match executor
            .execute_json("airc/realtime-publish", publish_params)
            .await
        {
            Ok(publish_result) => {
                let event_id = publish_result
                    .get("eventId")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Ok(ChatSendResult {
                    message_id,
                    event_id,
                    warning: None,
                })
            }
            Err(airc_err) => Ok(ChatSendResult {
                message_id,
                event_id: None,
                warning: Some(format!(
                    "airc/realtime-publish failed: {airc_err}. Message stored locally (id={message_id}) but not broadcast to peers."
                )),
            }),
        }
    }

    /// #265: seed the repetition-perception speech rings from the durable
    /// transcript at boot. The rings (`record_room_speech` /
    /// `record_own_speech`) are in-process statics, so a reboot wipes them at
    /// the exact moment they're most needed — the post-boot wake round, where
    /// every persona re-emits its greeting 2+ times with the repetition and
    /// [settled] facts blind (observed live 2026-07-30, twice). The durable
    /// store (#140) already holds the history; this replays the latest
    /// lines of one `chat/poll` page (chronological, all rooms — each ring keeps
    /// only its own bounded tail) so the facts have priors from the first tick.
    ///
    /// Waits boundedly for the executor slot — this runs from the persist
    /// listener spawned in `initialize`, which can race
    /// `install_executor_on_all` (the #201 ordering); a missing executor here
    /// is a not-yet, not a bug, so we poll the slot instead of panicking.
    pub async fn hydrate_speech_rings(&self) -> Result<(), String> {
        const EXECUTOR_WAIT: std::time::Duration = std::time::Duration::from_secs(15);
        const EXECUTOR_POLL: std::time::Duration = std::time::Duration::from_millis(250);

        let deadline = std::time::Instant::now() + EXECUTOR_WAIT;
        while self.executor_slot.cloned().is_none() {
            if std::time::Instant::now() >= deadline {
                return Err(
                    "executor not installed within boot window — rings start cold".to_string(),
                );
            }
            tokio::time::sleep(EXECUTOR_POLL).await;
        }

        let result = self
            .poll(ChatPollParams {
                room_id: None,
                after_message_id: None,
                before_message_id: None,
                // One page, the poll's own (each ring keeps only its own bounded tail).
                limit: None,
            })
            .await?;

        let mut seeded = 0usize;
        for msg in &result.messages {
            // Stored ChatMessageEntity shape (see `line_store::commit_line`): roomId,
            // senderId, content.text. Malformed rows are skipped, not fatal —
            // hydration is best-effort priors, never a boot gate.
            let room = msg
                .get("roomId")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok());
            let text = msg
                .get("content")
                .and_then(|c| c.get("text"))
                .and_then(Value::as_str);
            let (Some(room), Some(text)) = (room, text) else {
                continue;
            };
            let speech_sender = msg
                .get("senderId")
                .and_then(Value::as_str)
                .and_then(|s| uuid::Uuid::parse_str(s).ok());
            crate::cognition::deliberation_budget::record_room_speech(room, speech_sender, text);
            if let Some(sender) = msg
                .get("senderId")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            {
                crate::cognition::deliberation_budget::record_own_speech(
                    crate::identity::PeerId::from_uuid(sender),
                    room,
                    text,
                );
            }
            seeded += 1;
        }
        continuum_base::probe!(
            class = "speech.rings.hydrated",
            seeded = seeded,
            scanned = result.count,
            "repetition-fact rings seeded from durable transcript (#265)"
        );
        Ok(())
    }
}

impl Default for ChatModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Install the chat module as the inbound seam's line importer and seed the repetition-fact
/// speech rings from the durable transcript (#265). The importer is the durable-transcript writer
/// now: the inbound seam AWAITS each line's import (`line_store::import`), where it used to ride
/// a lossy broadcast receiver whose lag silently dropped lines from the store.
///
/// Spawned from `ChatModule::initialize` on the ModuleContext's runtime HANDLE (registration runs
/// on a non-tokio thread; a bare `tokio::spawn` there panics the boot, 2026-07-16).
pub fn install_line_importer(handle: &tokio::runtime::Handle, module: Arc<ChatModule>) {
    super::line_store::install_importer(Arc::clone(&module));
    handle.spawn(async move {
        if let Err(error) = module.hydrate_speech_rings().await {
            tracing::warn!(
                error,
                "speech-ring hydration failed — repetition facts start cold this boot (#265)"
            );
        }
    });
}

// ── time helpers ─────────────────────────────────────────────────────
//
// Wall-clock reads centralized here so chat's handlers stay free of
// `SystemTime` calls scattered through their bodies. Both use the same
// epoch instant so a stored timestamp and an airc envelope's
// `createdAtMs` from the same `send()` call agree by construction
// (rather than risking a tiny skew between two separate reads).
pub(super) fn now_iso(unix_ms: u64) -> String {
    // The TS ChatMessageEntity carries `timestamp` as an ISO-8601
    // string (matches how the TS impl writes it via
    // `new Date().toISOString()`). Format it from the same epoch we
    // pass to the airc envelope so the two surfaces agree on the
    // same moment.
    let secs = (unix_ms / 1000) as i64;
    let nsec_part = ((unix_ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nsec_part)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_string())
}
