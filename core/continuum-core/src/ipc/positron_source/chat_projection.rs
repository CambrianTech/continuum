use super::*;

impl ChatProjection {
    /// Test / headless constructor: every room resolves to `"chat"`.
    #[cfg(test)]
    pub(super) fn new(substrate: Substrate) -> Self {
        Self::with_purpose(substrate, crate::experience::room_purpose::default_source())
    }

    /// ONE shared purpose resolver feeds BOTH the chat view's `purpose` field and
    /// the Experience manifest's recipe lookup — INJECTED, because the resolver
    /// that reads a room's recipe binding needs a live airc handle the projection
    /// has no business owning.
    ///
    /// It was constructed inline here as `default_source()` — "every room is a
    /// chat room" — which meant a benchmark room spawned from a recipe still
    /// projected as chat, with no scoreboard region, to every renderer and to the
    /// citizen standing in it. Injection is what lets
    /// [`crate::ipc::recipe_room_purpose`] answer instead.
    pub(super) fn with_purpose(
        substrate: Substrate,
        purpose_source: crate::experience::room_purpose::SharedRoomPurpose,
    ) -> Self {
        Self::with_rooms(substrate, purpose_source, None)
    }

    /// As [`Self::with_purpose`], plus the per-room stores every per-room envelope
    /// is mirrored into (#408). Experiences resolve from the EMBEDDED recipe set
    /// only — the right floor for tests and headless fixtures. Production goes
    /// through [`Self::with_experience`] so on-disk authored recipes resolve too
    /// (#432).
    pub(super) fn with_rooms(
        substrate: Substrate,
        purpose_source: crate::experience::room_purpose::SharedRoomPurpose,
        rooms: Option<std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>>,
    ) -> Self {
        let experience_source = RecipeExperienceSource::builtins(purpose_source.clone());
        Self::with_experience(substrate, purpose_source, rooms, experience_source)
    }

    /// As [`Self::with_rooms`], with the experience source INJECTED (#432).
    /// `spawn` builds it via `builtins_with_overlay` so a recipe authored on
    /// disk projects without recompiling — the module's own "new experience is
    /// a new recipe entry, zero code" promise, finally true in production.
    pub(super) fn with_experience(
        substrate: Substrate,
        purpose_source: crate::experience::room_purpose::SharedRoomPurpose,
        rooms: Option<std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>>,
        experience_source: RecipeExperienceSource,
    ) -> Self {
        Self {
            rooms,
            substrate,
            // The projection is the SOLE writer of the `chat` kind, so
            // its own standalone `Revisions` well is the authoritative
            // monotonic source for that kind.
            builder: StateBuilder::standalone(),
            room_id: None,
            explicit_focus: None,
            accums: HashMap::new(),
            vitals: HashMap::new(),
            loadout: HashMap::new(),
            genes: HashMap::new(),
            experience_source,
            experience_builder: StateBuilder::standalone(),
            roster_builder: StateBuilder::standalone(),
            purpose_source,
        }
    }

    /// Cold-boot adoption: the first event's room becomes the node mirror so
    /// a fresh session shows SOMETHING; after that only `apply_focus` moves it.
    pub(super) fn adopt_if_unfocused(&mut self, room: Uuid) {
        if self.room_id.is_none() {
            self.room_id = Some(room);
        }
    }

    /// Apply an EXPLICIT focus switch (the `nav/select` verb): point the node
    /// mirror at `room` and re-publish that room's OWN accumulated view. A
    /// quiet room (no events observed since boot) publishes the honest empty
    /// view — room id set, nothing else — never another room's leftovers.
    pub(super) fn apply_focus(&mut self, room: Uuid) {
        self.explicit_focus = Some(room);
        self.room_id = Some(room);
        // Clear the emit-on-change caches so the focus re-publishes to the
        // node substrate even when the room's content hasn't changed.
        let accum = self.accums.entry(room).or_default();
        accum.last_chat = None;
        accum.last_roster = None;
        accum.last_experience = None;
        self.store_for(room);
    }

    /// Fold a persona's radiated vitals into the per-member map and re-store.
    /// The values are merged into the roster at `store` time (they outlive any
    /// single presence snapshot), so a persona breathing its energy updates the
    /// widget without touching the neutral presence path. A member that left the
    /// roster keeps no bearing here — `store` only merges vitals for members the
    /// current roster still lists.
    pub(super) fn apply_vitals(&mut self, update: PersonaVitalsUpdate) {
        // Loadout only OVERWRITES when the radiator reports one this tick — a
        // `None` means "not sampled" (the persona's cycle has no binding), never
        // "clear the last-known loadout". So a member keeps its resolved loadout
        // across ticks; it's a durable capability fact, not a per-tick meter.
        if let Some(loadout) = update.loadout {
            self.loadout.insert(update.member_id, loadout);
        }
        self.vitals.insert(update.member_id, update.vitals);
        // Genes REPLACE (not merge): an empty list is a real "paged out to
        // base" fact from the radiator, not an absence to preserve across.
        self.genes.insert(update.member_id, update.genes);
        // Member-truth overlay: re-store every room whose roster holds this
        // member — her meters breathe in each room she stands in.
        let rooms: Vec<Uuid> = self
            .accums
            .iter()
            .filter(|(_, a)| a.roster.iter().any(|slot| slot.member_id == update.member_id))
            .map(|(id, _)| *id)
            .collect();
        for room in rooms {
            self.store_for(room);
        }
    }

    /// Fold a posted message into the view and store the new snapshot.
    /// Idempotent on `message_id` AND on `(sender, content)`: a redelivered
    /// event (the bus is best-effort) is caught by the id check, but a
    /// **multi-hop replay** re-emits the same logical message with a FRESH
    /// `message_id` (#16) — which the id check alone cannot collapse, and which
    /// showed as doubled turns in every client. Content-identity is the same
    /// stance the cognition admission dedup already takes (`content_hash_sha256`,
    /// `inbox_admission.rs`): one `(sender, content)` is one message, however many
    /// ids the bus minted for it. Identity is resolved from the roster, never
    /// carried on the message (see `AircChatPosted`).
    pub(super) fn apply_message(&mut self, msg: AircChatPosted) {
        // Per-room fold: the message lands in ITS room's accumulator — the
        // focused view never wanders, and no room's store starves.
        self.adopt_if_unfocused(msg.room_id);
        let accum = self.accums.entry(msg.room_id).or_default();
        let is_duplicate = accum.messages.iter().any(|m| {
            m.id == msg.message_id || (m.sender_id == msg.sender_id && m.content == msg.content)
        });
        if is_duplicate {
            return;
        }
        let resolved = resolve_identity(&accum.roster, msg.sender_id);
        accum.messages.push_back(ChatMessageView {
            id: msg.message_id,
            room_id: msg.room_id,
            sender_id: msg.sender_id,
            sender_name: resolved.name,
            sender_kind: resolved.kind,
            integrations: resolved.integrations,
            provenance: resolved.provenance,
            content: msg.content,
            timestamp: msg.timestamp,
            heard_by: Vec::new(),
        });
        while accum.messages.len() > MAX_MESSAGES_PER_SNAPSHOT {
            accum.messages.pop_front();
        }
        self.store_for(msg.room_id);
    }

    /// Fold one executed tool act into the receipt ring and store — the act
    /// sibling of `apply_message` (#243). Same focus-pin and room-switch
    /// discipline; idempotent on `act_id` (bus redelivery). The producer's
    /// resolved `actor_name` is adopted as-is when the actor is NOT in the
    /// roster (a headless solve fork acting in a room its presence never
    /// joined); a rostered actor re-resolves so display identity stays
    /// single-sourced.
    pub(super) fn apply_act(&mut self, act: PersonaActUpdate) {
        // A nil room is not a room: headless solves radiate acts with
        // Uuid::nil() (guarded at the producer, kept here as defense in
        // depth) — a per-room fold keyed on nil would mint a phantom room.
        if act.room_id.is_nil() {
            return;
        }
        self.adopt_if_unfocused(act.room_id);
        let accum = self.accums.entry(act.room_id).or_default();
        if accum.acts.iter().any(|a| a.id == act.act_id) {
            return;
        }
        let name = accum
            .roster
            .iter()
            .find(|s| s.member_id == act.actor_id)
            .map(|s| s.display_name.clone())
            .unwrap_or(act.actor_name);
        accum.acts.push_back(ActReceiptView {
            id: act.act_id,
            room_id: act.room_id,
            actor_id: act.actor_id,
            actor_name: name,
            tool: act.tool,
            summary: act.summary,
            ok: act.ok,
            timestamp: act.timestamp,
            thought: act.thought,
        });
        while accum.acts.len() > MAX_ACTS_PER_SNAPSHOT {
            accum.acts.pop_front();
        }
        self.store_for(act.room_id);
    }

    /// Replace the focused room's roster from a presence snapshot and
    /// store the new view. Presence is the room-name authority, so this
    /// (unlike a message) adopts the resolved name. It also **upgrades**
    /// any message whose sender was provisional (posted before its card
    /// arrived) now that the card is known.
    /// Fold a `chat:heard` receipt onto its row: the citizen joins `heard_by`
    /// once. A row not (yet, or any more) in the window is a no-op — the
    /// receipt decorates a line that exists, never a line of its own.
    pub(super) fn apply_heard(&mut self, heard: AircChatHeard) {
        if heard.room_id.is_nil() {
            return;
        }
        let Some(accum) = self.accums.get_mut(&heard.room_id) else { return };
        let Some(row) = accum.messages.iter_mut().find(|m| m.id == heard.message_id) else {
            return;
        };
        if row.heard_by.contains(&heard.persona_id) {
            return;
        }
        row.heard_by.push(heard.persona_id);
        self.store_for(heard.room_id);
    }

    pub(super) fn apply_presence(&mut self, update: AircPresenceUpdate) {
        // Per-room fold: presence lands in ITS room's accumulator — with
        // per-room emitters (#2606) every room's roster stays current and
        // none of them can yank the focused view (structural, not a guard).
        self.adopt_if_unfocused(update.room_id);
        let accum = self.accums.entry(update.room_id).or_default();
        accum.room_name = update.room_name;
        // The update's roster IS the neutral slot type — move it in directly,
        // no per-field copy through a twin struct.
        accum.roster = update.roster;
        reresolve_messages(accum);
        self.store_for(update.room_id);
    }

    /// Re-resolve each stored message's identity from the current roster.
    /// Only **upgrades** — a message whose sender is present in the roster
    /// adopts the card; a sender who has since left keeps its already-
    /// resolved identity (presence is authoritative for who is here NOW,
    /// not for who authored a past message). This is what makes the
    /// provisional-until-card projection settle to the truth in place.
    pub(super) fn store_for(&mut self, room_id: Uuid) {
        self.store_room(room_id);
    }

    /// Frame ONE room's accumulator as its `chat`/`roster`/`experience`
    /// envelopes and write them: the room's own store always; the node
    /// substrate only when this room is the focused mirror.
    pub(super) fn store_room(&mut self, room_id: Uuid) {
        let Some(accum) = self.accums.get(&room_id) else {
            return;
        };
        // Merge each member's latest radiated vitals into its neutral slot
        // (design B fold): presence owns the roster shape; vitals are the live
        // cognition overlay keyed by id. A member with no vitals entry keeps the
        // slot's empty map — no fabricated bars.
        let roster: Vec<RosterSlotView> = accum
            .roster
            .iter()
            .map(|slot| {
                let vitals = self.vitals.get(&slot.member_id);
                let loadout = self.loadout.get(&slot.member_id);
                let genes = self.genes.get(&slot.member_id);
                // No live overlay for this member → the presence slot, verbatim.
                if vitals.is_none() && loadout.is_none() && genes.is_none() {
                    return slot.clone();
                }
                RosterSlotView {
                    vitals: vitals.cloned().unwrap_or_else(|| slot.vitals.clone()),
                    loadout: loadout.cloned().or_else(|| slot.loadout.clone()),
                    genes: genes.cloned().unwrap_or_else(|| slot.genes.clone()),
                    ..slot.clone()
                }
            })
            .collect();

        // Publish the room's Experience manifest (the Join Contract) ALONGSIDE the
        // chat view, so a renderer/agent sees the whole room — regions, affordances,
        // and the live membership — not just the message stream. Membership is
        // projected from the SAME roster (kind-agnostic: human/persona/agent are all
        // Members). `Experience` is renderer-agnostic (no positron-core dep), so it
        // rides `session_raw` under its own `KIND`. No recipe for the room's purpose
        // → nothing published (fail-quiet on this OPTIONAL surface; chat still ships).
        let mirror_node = self.room_id == Some(room_id);
        let exp = self.build_experience(room_id, &roster);
        let accum = self.accums.get_mut(&room_id).expect("accum checked above");
        if let Some(exp) = exp {
            // Emit-on-change: skip re-publishing an identical manifest (messages don't
            // move membership/purpose). Only presence-driven changes reach the wire.
            if accum.last_experience.as_ref() != Some(&exp) {
                let payload = serde_json::to_value(&exp)
                    .expect("Experience must serialize — substrate bug, not a runtime error");
                let env = self.experience_builder.session_raw(Experience::KIND, payload);
                Self::sink(&self.rooms, &self.substrate, mirror_node, room_id, env);
                self.accums.get_mut(&room_id).expect("accum").last_experience = Some(exp);
            }
        }

        // Publish the rich roster under its OWN "roster" kind (path-3 per-region
        // ViewState): the Experience manifest's roster region binds to this for
        // names/kinds/vitals — the display data the manifest's minimal Member omits.
        // Emit-on-change (roster only shifts on presence, not per message).
        if self.accums[&room_id].last_roster.as_deref() != Some(roster.as_slice()) {
            let env = self.roster_builder.session(RosterViewState {
                room_id,
                roster: roster.clone(),
            });
            Self::sink(&self.rooms, &self.substrate, mirror_node, room_id, env);
            self.accums.get_mut(&room_id).expect("accum").last_roster = Some(roster.clone());
        }

        let accum = &self.accums[&room_id];
        let view = ChatViewState {
            room_id,
            room_name: accum.room_name.clone(),
            // The room's activity purpose — resolved through the `RoomPurposeSource`
            // seam (#6), NOT hardcoded. Today the default answers "chat" for every
            // room; when the recipe-backed resolver is injected, a foundry / scada /
            // academy room reports its own purpose and the client's Content primitive
            // dispatches on it (ACTIVITY-ROOM-PATTERNS.md) with no change here.
            purpose: self.purpose_source.purpose_for(room_id),
            messages: accum.messages.iter().cloned().collect(),
            acts: accum.acts.iter().cloned().collect(),
            roster,
        };
        // Emit-on-change: a vitals/presence tick that moved nothing in the
        // chat view must not re-serialize and re-broadcast 100 messages.
        if accum.last_chat.as_ref() == Some(&view) {
            return;
        }
        let env = self.builder.session(view.clone());
        Self::sink(&self.rooms, &self.substrate, mirror_node, room_id, env);
        self.accums.get_mut(&room_id).expect("accum").last_chat = Some(view);
    }

    /// Store one per-room envelope to BOTH sinks: the node substrate (what the
    /// focused-room web session reads today) and the room's own store (what a
    /// consumer that names its room reads).
    ///
    /// ONE place decides the dual-sink rule, so a future kind cannot be added to one
    /// sink and forgotten in the other — the compression law applied to a write path.
    pub(super) fn sink(
        rooms: &Option<std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>>,
        substrate: &Substrate,
        mirror_node: bool,
        room_id: Uuid,
        envelope: continuum_positron::StateEnvelope,
    ) {
        // One allocation, both sinks (2026-08-23 audit). The node substrate is
        // the FOCUSED room's mirror only — an unfocused room writes its own
        // store and never the center pane (the anti-wander rule, structural).
        let envelope = std::sync::Arc::new(envelope);
        if let Some(rooms) = rooms {
            rooms.for_room(room_id).store_shared(std::sync::Arc::clone(&envelope));
        }
        if mirror_node {
            substrate.store_shared(envelope);
        }
    }

    /// Assemble this room's [`Experience`] manifest: recipe (by the room's purpose)
    /// → static manifest, live roster → `membership`. Pure over `(room_id, roster)`
    /// so it's unit-testable without a live bus. `None` when no recipe matches the
    /// room's purpose (the manifest is an OPTIONAL surface; chat still ships).
    ///
    /// Membership is kind-agnostic — every present peer becomes a plain
    /// [`Standing::Member`]; structural-role overlay (examinee/owner) is a higher
    /// concern supplied by run/room context, not this presence projection.
    pub(super) fn build_experience(&self, room_id: Uuid, roster: &[RosterSlotView]) -> Option<Experience> {
        let membership: Vec<Member> = roster
            .iter()
            .map(|slot| Member {
                peer_id: crate::identity::PeerId::from_uuid(slot.member_id),
                standing: Standing::Member,
            })
            .collect();
        self.experience_source
            .experience_for(room_id)
            .map(|exp| exp.with_membership(membership))
    }
}

/// Classify a bus event into a typed projection input, or `None` when the
/// event is not a chat/presence event this projection renders. Pure — no
/// substrate side effect — so it's unit-testable without a live bus.
pub(super) enum ProjectionInput {
    Message(AircChatPosted),
    Heard(AircChatHeard),
    Presence(AircPresenceUpdate),
    Vitals(PersonaVitalsUpdate),
    /// One executed tool act (`persona:act` — the receipt stream, #243).
    Act(PersonaActUpdate),
    /// An explicit focus switch (`chat:focused` — the `nav/select` verb).
    Focus(AircChatFocused),
}

pub(super) fn classify(name: &str, payload: &serde_json::Value) -> Option<ProjectionInput> {
    let body = bus_event_body(payload);
    match name {
        CHAT_HEARD => serde_json::from_value::<AircChatHeard>(body.clone())
            .ok()
            .map(ProjectionInput::Heard),
        CHAT_POSTED => serde_json::from_value::<AircChatPosted>(body.clone())
            .ok()
            .map(ProjectionInput::Message),
        PRESENCE_UPDATED => serde_json::from_value::<AircPresenceUpdate>(body.clone())
            .ok()
            .map(ProjectionInput::Presence),
        PERSONA_VITALS => serde_json::from_value::<PersonaVitalsUpdate>(body.clone())
            .ok()
            .map(ProjectionInput::Vitals),
        PERSONA_ACT => serde_json::from_value::<PersonaActUpdate>(body.clone())
            .ok()
            .map(ProjectionInput::Act),
        CHAT_FOCUSED => serde_json::from_value::<AircChatFocused>(body.clone())
            .ok()
            .map(ProjectionInput::Focus),
        _ => None,
    }
}

/// Subscribe the chat projection to the bus and run its consume loop on
/// `rt`. Holds a clone of the same [`Substrate`] the WS server serves, so
/// the projected view reaches subscribed thin clients. Runs for the
/// process lifetime.
///
/// Subscribes synchronously (before spawning) so no publish can race
/// ahead of the receiver — the same ordering discipline
/// `airc_bridge_directive::spawn_consumer` uses.
/// One stored `chat_messages` entity → the `chat:posted` wire payload shape
/// `classify` consumes. `None` for records missing the core facts (a malformed
/// row must not poison the seed). Stored timestamps are ISO strings; the wire
/// carries unix ms.
pub(super) fn entity_to_posted(entity: &serde_json::Value) -> Option<serde_json::Value> {
    let text = entity.get("content")?.get("text")?.as_str()?.to_string();
    let ts_ms = chrono::DateTime::parse_from_rfc3339(entity.get("timestamp")?.as_str()?)
        .ok()?
        .timestamp_millis();
    Some(serde_json::json!({
        "messageId": entity.get("id")?,
        "roomId": entity.get("roomId")?,
        "senderId": entity.get("senderId")?,
        "content": text,
        "timestamp": ts_ms,
    }))
}

/// Fetch the seed room's stored tail (the same `chat_messages` query
/// `chat/poll` serves — latest 50, returned chronological). The data module
/// may still be initializing at projection spawn, so retry briefly; a seed
/// that never comes degrades to the wire-fed-only projection (the pre-seed
/// behavior) — logged loud, never a crash.
pub(super) async fn fetch_seed_messages(
    executor: &crate::runtime::CommandExecutor,
    room: Uuid,
) -> Vec<serde_json::Value> {
    for attempt in 0..5u32 {
        let query = serde_json::json!({
            "dbPath": "main",
            "collection": "chat_messages",
            "filter": { "roomId": { "$eq": room.to_string() } },
            "sort": [{ "field": "timestamp", "direction": "desc" }],
            "limit": MAX_MESSAGES_PER_SNAPSHOT,
        });
        match executor.execute_json("data/query", query).await {
            Ok(result) => {
                let mut entities: Vec<serde_json::Value> = result
                    .get("data")
                    .and_then(|d| d.as_array())
                    .map(|records| {
                        records
                            .iter()
                            .filter_map(|r| r.get("data").cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                entities.reverse(); // desc query → chronological apply order
                return entities.iter().filter_map(entity_to_posted).collect();
            }
            Err(error) => {
                tracing::warn!(
                    "chat projection seed attempt {attempt} failed (data module warming?): {error}"
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
    tracing::warn!("chat projection seed unavailable — projection starts wire-fed only");
    Vec::new()
}

/// `seed` is the durable-hydration source (invalid-positron fix, 2026-07-30):
/// the chat projection is THE positron read surface, and it must never present
/// an empty room while the durable transcript holds history — no client may
/// need its own hydration logic ([[positron-durable-state-is-sdk-inherent]]).
/// Before folding live events, the projector hydrates its accumulator with the
/// seed room's stored tail, pushed through the SAME `classify` path as wire
/// events — one message semantics, two sources. `None` (tests, headless
/// fixtures, no bootstrap room) = wire-fed only, the prior behavior.
///
/// `purpose` resolves each room to its activity nature. Pass the live
/// [`crate::ipc::recipe_room_purpose::RecipeRoomPurpose`] so a recipe-spawned
/// room projects as what it IS; `room_purpose::default_source()` (every room →
/// chat) is the honest headless fallback.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    bus: Arc<MessageBus>,
    substrate: Substrate,
    seed: Option<(Arc<crate::runtime::CommandExecutor>, Uuid)>,
    purpose: crate::experience::room_purpose::SharedRoomPurpose,
    rooms: Option<Arc<continuum_positron::scoping::PerRoomSubstrates>>,
) {
    let mut rx = bus.receiver();
    // Demand the current roster now (#118): the presence emitter dedups and
    // may have already fired for a stable roster before this projection
    // subscribed. Without the cue a late/restarted chat projection holds a
    // roster-empty view until presence next changes. `rx` is subscribed
    // above, so the emitter's re-publish lands in our buffer.
    crate::ipc::positron_presence::request_presence_resync(&bus);
    // #432: production resolves experiences from the embedded floor PLUS the
    // on-disk overlay (`<continuum_root>/recipes`) — authoring an activity is a
    // JSON file, zero code, the module's own promise. A malformed overlay file
    // is REFUSED loudly here, and the projection keeps the embedded floor so
    // chat itself never goes dark over one bad recipe; the author gets the
    // second, harder surface at `activity/spawn`, whose validation reads the
    // SAME directory and returns the parse error naming the file.
    let overlay_dir = RecipeExperienceSource::overlay_dir(
        &crate::paths::continuum_root(),
    );
    let experience_source = match RecipeExperienceSource::builtins_with_overlay(
        purpose.clone(),
        &overlay_dir,
    ) {
        Ok(source) => source,
        Err(e) => {
            tracing::error!(
                error = %e,
                dir = %overlay_dir.display(),
                "recipe overlay REFUSED — an authored recipe failed to load; \
                 serving EMBEDDED recipes only until the named file is fixed \
                 or removed (#432)"
            );
            RecipeExperienceSource::builtins(purpose.clone())
        }
    };
    rt.spawn(async move {
        let mut projection =
            ChatProjection::with_experience(substrate, purpose, rooms, experience_source);
        if let Some((executor, room)) = seed {
            for payload in fetch_seed_messages(&executor, room).await {
                if let Some(ProjectionInput::Message(m)) = classify(CHAT_POSTED, &payload) {
                    projection.apply_message(m);
                }
            }
        }
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Some(input) = classify(&event.name, &event.payload) {
                        match input {
                            ProjectionInput::Message(m) => projection.apply_message(m),
                            ProjectionInput::Presence(p) => projection.apply_presence(p),
                            ProjectionInput::Vitals(v) => projection.apply_vitals(v),
                            ProjectionInput::Act(a) => projection.apply_act(a),
                            ProjectionInput::Heard(h) => projection.apply_heard(h),
                            ProjectionInput::Focus(f) => projection.apply_focus(f.room_id),
                        }
                    }
                }
                // Fell behind the broadcast buffer. The projection is a
                // last-good cache of a live stream, not guaranteed
                // delivery — skip the gap and keep folding. The next
                // event re-establishes a coherent snapshot.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => break,
            }
        }
    });
}
