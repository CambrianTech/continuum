use super::*;

impl PersonaAircRuntime {

    /// Join a room UNDER THIS CITIZEN'S IDENTITY at runtime and bump the
    /// membership epoch so her live perception stream re-opens with the
    /// enlarged channel snapshot (P0 20b44763). This — not `airc().join`
    /// directly — is the ONE runtime-join path for a living persona:
    /// airc-lib's `subscribe_subscribed_filtered` collects the channel list
    /// once at subscribe time, so a bare join grants durable membership to a
    /// room whose events her existing stream will never carry (measured
    /// 2026-08-15: three benchmark rounds, 12 addressed kickoffs each, zero
    /// turns — every per-run room was born deaf).
    pub async fn join_room(&self, name: &str) -> Result<(), AircError> {
        self.airc.join(name).await?;
        self.membership_epoch.send_modify(|e| *e += 1);
        Ok(())
    }

    /// Belong to `name` WITHOUT moving the focus — the citizen's verb, where
    /// [`join_room`](Self::join_room) is the operator's.
    ///
    /// `Airc::join` is subscribe AND focus; this is subscribe alone. Use it for
    /// a room this peer should receive and be able to speak in, when some OTHER
    /// room is the one its short-shape commands (and its work cards) belong to.
    /// Bumps the same membership epoch as `join_room`, because a subscription
    /// change is a membership change however the focus lands — routing that off
    /// this epoch must not care which verb moved it.
    pub async fn subscribe_room(&self, name: &str) -> Result<airc_lib::Room, AircError> {
        let room = self.airc.subscribe_room(name).await?;
        self.membership_epoch.send_modify(|e| *e += 1);
        Ok(room)
    }

    /// Leave a room and refresh the same live subscription snapshot as joining.
    /// A rejected part (including an already-parted room) leaves the epoch alone.
    pub async fn leave_room(&self, name: Option<&str>) -> Result<airc_lib::Room, AircError> {
        let room = self.airc.part_channel(name).await?;
        self.membership_epoch.send_modify(|e| *e += 1);
        Ok(room)
    }

    /// Wrap an already-attached + already-joined `Arc<Airc>` into a
    /// `PersonaAircRuntime`. Skips the `bootstrap` join step —
    /// useful for the demo binary that joins by NAME (the canonical
    /// path) instead of by uuid-as-string (the bootstrap path's
    /// current shape — see #133-followup note on `bootstrap`).
    ///
    /// The caller is responsible for: (1) calling `attach_as` /
    /// `join` BEFORE handing the Arc in, (2) supplying the
    /// `RoomId` matching the room they joined.
    pub fn from_attached(
        persona_id: Uuid,
        agent_name: impl Into<String>,
        home: PathBuf,
        airc: Arc<Airc>,
        default_room: RoomId,
        source: crate::persona::identity_provider::PersonaIdentitySource,
    ) -> Self {
        // Slice 1B of #142: collapse persona_id := peer_id. Same
        // rationale as in `bootstrap` above. The input parameter is
        // retained for API continuity but its value is reseated to
        // the airc-lib peer_id (cryptographic ground truth).
        let peer_id_uuid = airc.peer_id().as_uuid();
        let agent_name = agent_name.into();
        if persona_id != peer_id_uuid {
            // Expected for fresh mints — see equivalent doc in
            // `bootstrap` above. `debug!` not `warn!` to avoid log
            // spam at every boot.
            tracing::debug!(
                input_persona_id = %persona_id,
                peer_id = %peer_id_uuid,
                agent_name = %agent_name,
                "PersonaAircRuntime::from_attached: persona_id parameter \
                 ignored — runtime collapses persona_id := peer_id per Slice \
                 1B of #142."
            );
        }
        Self {
            persona_id: peer_id_uuid,
            agent_name,
            home,
            airc,
            default_room,
            membership_epoch: tokio::sync::watch::channel(0u64).0,
            inbound_handle: None,
            // from_attached is sync + the pump install is async, so
            // we don't install at construction. Callers that want the
            // persona addressable for cross-grid commands invoke
            // `install_command_pump` separately. Bootstrap installs
            // automatically; this seam is for tests + demo binaries
            // that want fine-grained control.
            command_pump: None,
            executor: None,
            // Presence is opt-out here for the same reason as the command pump:
            // `from_attached` is sync and `start_agent_heartbeat` is async. The
            // live `bootstrap` path always starts the pump; test/demo callers
            // that want roster presence start it themselves after construction.
            _heartbeat: None,
            owned_tasks: std::sync::Mutex::new(Vec::new()),
            source,
        }
    }

    /// Install the command inbound pump on this runtime (task #222).
    /// `bootstrap` calls this automatically; `from_attached` callers
    /// invoke it explicitly when they want the persona to become
    /// addressable for cross-grid commands.
    ///
    /// Idempotency: if a pump is already installed, this aborts the
    /// existing one before installing the new — per
    /// `[[no-fallbacks-ever]]` the alternative (silently keep two
    /// pumps fighting over the broadcast receiver) is the wrong
    /// shape.
    pub async fn install_command_pump(
        &mut self,
        executor: Arc<crate::runtime::command_executor::CommandExecutor>,
    ) -> Result<(), PersonaAircRuntimeError> {
        if let Some(existing) = self.command_pump.take() {
            existing.abort();
        }
        let grant_authorizer =
            crate::persona::command_inbound_pump::build_grant_authorizer(&self.airc, &self.home)
                .await
                .map_err(|source| PersonaAircRuntimeError::GrantAuthorizerBuild {
                    agent_name: self.agent_name.clone(),
                    source,
                })?;
        self.executor = Some(Arc::clone(&executor));
        let pump = crate::persona::command_inbound_pump::PersonaCommandInboundPump::spawn(
            self.persona_id,
            Arc::clone(&self.airc),
            executor,
            grant_authorizer,
            self.membership_epoch.subscribe(),
        )
        .await
        .map_err(|source| PersonaAircRuntimeError::CommandPumpInstall {
            agent_name: self.agent_name.clone(),
            source,
        })?;
        self.command_pump = Some(pump);
        Ok(())
    }

    /// The persona's stable continuum identifier.
    pub fn persona_id(&self) -> Uuid {
        self.persona_id
    }

    /// Where this citizen's identity came from — resumed vs minted.
    pub fn source(&self) -> crate::persona::identity_provider::PersonaIdentitySource {
        self.source
    }

    /// The persona's airc agent_name (matches what shows up in
    /// `airc peers` / `airc whois`).
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// The persona's airc home directory.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The persona's `Arc<Airc>` handle. Cognition + outbound paths
    /// hold this Arc to reach the persona's grid presence — for
    /// `say`, `publish`, `subscribe`, `peer_id`, etc. Direct access
    /// is intentional: there's no continuum-side wrapper between a
    /// persona and its own airc handle.
    pub fn airc(&self) -> &Arc<Airc> {
        &self.airc
    }

    /// Convenience: publish a plain text message in the persona's
    /// default room, signed under the persona's identity. Equivalent
    /// to `self.airc().say(text)`. Exists so the common case reads
    /// cleanly at the call site.
    pub async fn say(&self, text: &str) -> Result<EventId, AircError> {
        self.airc.say(text).await
    }

    /// The default room the persona joined at bootstrap.
    pub fn default_room(&self) -> RoomId {
        self.default_room
    }

}

// AircCitizen + AircTranscriptReader impls — substrate's universal
// airc-handle interface, satisfied by the production runtime. Per
// [[personas-are-citizens-airc-is-identity-provider]] the trait IS the
// citizen surface; the concrete runtime is one impl among future
// peers (human, web, browser). See `persona::airc_citizen` for the
// trait definition + rationale.
#[async_trait::async_trait]
impl crate::cognition::conversation::transcript_reader::AircTranscriptReader for PersonaAircRuntime {
    async fn page_recent(&self, limit: usize) -> Result<Vec<airc_lib::TranscriptEvent>, AircError> {
        // Route through the ONE kinds-filtered impl on `airc_lib::Airc`
        // (persona/airc_source.rs, #297) — never the raw inherent page.
        crate::cognition::conversation::transcript_reader::AircTranscriptReader::page_recent(&*self.airc, limit).await
    }

    async fn page_recent_in(
        &self,
        room: Option<airc_core::RoomId>,
        limit: usize,
    ) -> Result<Vec<airc_lib::TranscriptEvent>, AircError> {
        // Explicit forward (#367) — inheriting the trait default here would
        // silently drop the turn-room scope (#262-class regression).
        crate::cognition::conversation::transcript_reader::AircTranscriptReader::page_recent_in(&*self.airc, room, limit)
            .await
    }
}

#[async_trait::async_trait]
impl crate::persona::room_roster_source::AircRosterReader for PersonaAircRuntime {
    fn self_peer_id(&self) -> airc_core::PeerId {
        self.airc.peer_id()
    }

    async fn room_roster(
        &self,
        within: std::time::Duration,
        window: usize,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_lib::RoomMember>, AircError> {
        crate::persona::room_roster_source::AircRosterReader::room_roster(
            self.airc.as_ref(),
            within,
            window,
            room,
        )
        .await
    }
}

#[async_trait::async_trait]
impl crate::persona::room_doctrine_source::AircDoctrineReader for PersonaAircRuntime {
    async fn room_doctrine(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<Option<airc_core::doctrine::RoomDoctrinePublished>, AircError> {
        crate::persona::room_doctrine_source::AircDoctrineReader::room_doctrine(
            self.airc.as_ref(),
            room,
        )
        .await
    }
}

#[async_trait::async_trait]
impl crate::persona::wall_source::WallReader for PersonaAircRuntime {
    async fn wall_posts(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<Vec<airc_core::doctrine::WallPostPublished>, AircError> {
        // Whole board (all categories); the source filters/labels per post.
        // UFCS for the same reason as AircHandleAdapter: the inherent
        // `Airc::wall_posts` takes a CATEGORY filter, so a plain call would
        // compile and quietly drop the bound room.
        <airc_lib::Airc as crate::persona::wall_source::WallReader>::wall_posts(
            self.airc.as_ref(),
            room,
        )
        .await
    }

    /// The cards of the wall's room, from her board index: no board read while the room's
    /// board is unchanged (it was one full board read per act).
    async fn card_labels(&self, room: Option<uuid::Uuid>) -> Option<crate::persona::wall_source::CardLabels> {
        let board =
            crate::persona::work_board::board_index::room_board(self.airc.as_ref(), Some(room?)).await.ok()?;
        Some(board.cards.iter().map(|card| (card.card_id.as_uuid(), (card.title.clone(), card.state))).collect())
    }
}

#[async_trait::async_trait]
impl crate::persona::held_work::AircWorkReader for PersonaAircRuntime {
    /// This persona's claimed cards, read from airc's work roster and filtered to
    /// its own peer — grounding reflects what IT owns, board-wide (all rooms).
    /// Her live claims, BOARD-TRUE. The roster projection lists every claim whose
    /// lease has not expired — including one on a card an operator moved back to
    /// Open, or that a pause returned to the deck. 2026-09-12 09:58–12:24Z: three
    /// coders kept working cards that read Open/no-owner on the board for two and a
    /// half hours (78 acts) because this read still called them held. A claim is a
    /// hold only while the board's own card says so (`hold_of` = Held); the rest are
    /// history, not work.
    async fn active_claims(&self) -> Result<Vec<airc_lib::WorkCard>, AircError> {
        // THE BOARD IS THE TRUTH ABOUT WHO HOLDS WHAT. This read used to start from the
        // work ROSTER (the scope's claim ledger) and intersect it with the board. The
        // roster is rebuilt after a boot, so for the first seconds of a fresh runtime it
        // listed nothing — 2026-09-13 08:40:50Z, 13 s after the core answered: Atlas's
        // `persona.work.gate held=0` while the board showed her live claim on
        // django-15467, so the pull took her a SECOND card (matplotlib-21568) while a
        // teammate sat with none. The board carries owner, lease and claim id; nothing
        // the roster adds is needed to know her held work.
        crate::persona::work_board::board_index::board_held_by(self.airc.as_ref()).await
    }

    /// The board's own row for one card, from every room she is in, through her board
    /// index: a wake's "what became of my card" reads the truth the renewal loop reads.
    async fn card(&self, card_id: uuid::Uuid) -> Option<airc_lib::WorkCard> {
        crate::persona::work_board::board_index::card(
            self.airc(),
            airc_work::WorkCardId::from_uuid(card_id),
        )
        .await
        .map(|(_, card)| card)
    }
}

#[async_trait::async_trait]
impl crate::persona::room_board_source::RoomBoardReader for PersonaAircRuntime {
    /// A room's WHOLE work board, every card/column/owner, through her board index: read
    /// from airc only when that room's board changed since the last read.
    async fn work_board(
        &self,
        room: Option<uuid::Uuid>,
    ) -> Result<airc_work::BoardSnapshot, AircError> {
        let board = crate::persona::work_board::board_index::room_board(self.airc.as_ref(), room).await?;
        Ok(airc_work::BoardSnapshot::clone(&board))
    }

    /// Published display names for card owners — delegates to the inner airc
    /// handle's alias store. This is the LIVE persona path: without it, every
    /// teammate's card renders as an 8-hex prefix she cannot recognize or
    /// reach out to.
    async fn peer_names(
        &self,
        peers: &[airc_core::PeerId],
    ) -> std::collections::HashMap<airc_core::PeerId, String> {
        crate::persona::room_board_source::RoomBoardReader::peer_names(self.airc.as_ref(), peers)
            .await
    }
}

#[async_trait::async_trait]
impl crate::persona::airc_citizen::AircCitizen for PersonaAircRuntime {
    fn own_task(&self, handle: JoinHandle<()>) {
        match self.owned_tasks.lock() {
            Ok(mut owned) => owned.push(handle),
            Err(_) => handle.abort(), // a poisoned list cannot own; never leave the task running
        }
    }

    fn peer_id(&self) -> Uuid {
        self.airc.peer_id().as_uuid()
    }

    async fn subscribe_all_rooms(&self) -> Result<airc_lib::FilteredEventStream, AircError> {
        crate::routing::airc_subscription::subscribe_every_room(&self.airc).await
    }

    fn membership_epoch(&self) -> tokio::sync::watch::Receiver<u64> {
        self.membership_epoch.subscribe()
    }

    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<EventId, AircError> {
        crate::persona::airc_citizen::publish_text_in_room(&self.airc, room_id, text).await
    }

    /// #170: delegate to airc-lib's ephemeral stream-chunk publish.
    async fn publish_stream_chunk(&self, chunk: &airc_lib::StreamChunk) -> Result<(), AircError> {
        self.airc.publish_stream_chunk(chunk).await.map(|_| ())
    }

    /// Route the WRITE through the ONE shared card-lifecycle path
    /// (`work::advance_card_state`) so the change + the in-process
    /// grade/lifecycle emit fire exactly as the `work/state` verb's do — the
    /// held-work settle edge and the verb are one transition, not two.
    async fn advance_card_to(
        &self,
        card_id: airc_lib::WorkCardId,
        state: airc_lib::CardState,
    ) -> Result<(), String> {
        crate::persona::work_board::advance::advance_card_state(
            &self.airc,
            card_id,
            state,
            "held-work-settle",
            Some(self.persona_id),
        )
        .await
    }

    async fn release_card(
        &self,
        card_id: airc_lib::WorkCardId,
        claim_id: airc_lib::ClaimId,
        reason: &str,
    ) -> Result<(), String> {
        // ONE release path, same as the claim: `work/release` as this citizen.
        let Some(executor) = self.executor.as_ref() else {
            return Err("no command executor installed on this runtime yet".to_string());
        };
        let caller = crate::routing::CallerIdentity::airc(crate::identity::PeerId::from_uuid(
            self.persona_id,
        ));
        let params = serde_json::json!({
            "card_id": card_id.as_uuid().to_string(),
            "claim_id": claim_id.to_string(),
            "reason": reason,
        });
        executor
            .execute_with_caller("work/release", params, Some(caller))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn subscribed_rooms(&self) -> Result<Vec<Uuid>, AircError> {
        let set = self.airc.subscription_set().await?;
        Ok(set
            .all()
            .map(|sub| sub.as_room().channel.as_uuid())
            .collect())
    }

    async fn held_cards(&self) -> Result<Vec<(Uuid, airc_lib::WorkCard)>, AircError> {
        Ok(crate::persona::work_board::board_index::scoped_board_held_by(self.airc.as_ref())
            .await?
            .into_iter()
            .map(|(room, card)| (room.channel.as_uuid(), card))
            .collect())
    }
}

impl Drop for PersonaAircRuntime {
    fn drop(&mut self) {
        if let Some(handle) = self.inbound_handle.take() {
            handle.abort();
        }
        if let Some(pump) = self.command_pump.take() {
            // Fire-and-forget abort. Drop is sync; the pump's async
            // `shutdown` (which awaits the JoinHandle) is what
            // explicit-shutdown callers use. Tokio reaps the
            // aborted task.
            pump.abort();
        }
        if let Ok(mut owned) = self.owned_tasks.lock() {
            for handle in owned.drain(..) {
                handle.abort();
            }
        }
        // Arc<Airc> drops alongside the runtime; airc-lib handles
        // its own cleanup (daemon connection close, identity state
        // flush).
        warn!(
            persona_id = %self.persona_id,
            agent_name = %self.agent_name,
            "PersonaAircRuntime dropped — persona left the grid"
        );
    }
}
