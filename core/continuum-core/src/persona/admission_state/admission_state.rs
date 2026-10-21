use super::*;

impl AdmissionState {
    /// Construct fresh admission state with the v1 default recipe + permissive
    /// trust mapping. `recall_metadata` is the per-persona sidecar registry
    /// that tracks volatile recall state for every admitted Engram. Per the
    /// no-backwards-compat doctrine (slice 5+), the constructor now requires
    /// the registry rather than minting one internally — this lets
    /// PersonaCognition share a single registry view across admission +
    /// recall + decay tick subsystems.
    pub fn new(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
    ) -> Self {
        Self::new_with_persistence(
            recall_metadata,
            crate::persona::admission_persistence::NoopSink::arc(),
        )
    }

    /// Construct AdmissionState with an explicit persistence sink.
    /// Production uses `OrmPersistenceSink::arc(engram_store,
    /// metadata_store)`; tests use `RecordingSink::arc()` or
    /// `NoopSink::arc()`. The sink observes every admission +
    /// metadata update and chooses what to do with them
    /// (fire-and-forget write, buffer, no-op, etc.).
    pub fn new_with_persistence(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
        persistence: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
    ) -> Self {
        Self {
            runner: InboxAdmissionRunner::default_v1(),
            seen_content: Arc::new(InMemorySeenContent::default()),
            seen_events: Arc::new(InMemorySeenEvents::default()),
            engrams: Mutex::new(Vec::new()),
            recall_metadata,
            persistence: RwLock::new(persistence),
            owner_id: RwLock::new(None),
            graph: crate::persona::engram_graph::EngramGraph::new(),
        }
    }

    /// Construct AdmissionState pre-populated with engrams + metadata
    /// loaded from disk. Used at persona boot — rehydrates the Vec +
    /// DashMap from the previous lifetime's persistence layer.
    ///
    /// Per the substrate's continual-learning property: every admit
    /// from now on rides on top of the loaded state; recall scoring
    /// works against the rehydrated salience values; the Hebbian
    /// rehearsal loop continues across the restart boundary.
    pub fn new_rehydrated(
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
        persistence: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
        loaded_engrams: Vec<Engram>,
        loaded_metadata: Vec<(Uuid, crate::persona::recall_metadata::RecallMetadata)>,
    ) -> Self {
        // Populate the metadata registry with the loaded snapshots
        // BEFORE building the Vec — recall scoring later reads
        // metadata via the registry, not via Engram fields.
        for (engram_id, metadata) in loaded_metadata {
            recall_metadata.admit(engram_id, metadata);
        }
        // Phantom-engram backfill (Slice B fix for review #1519,
        // task #171): an engram on disk WITHOUT a metadata row would
        // otherwise be permanently invisible to recall — `recall_scored`
        // filter_maps it out, so `record_recall_hit` never fires for it,
        // so the metadata row never gets created. The original inline
        // comment "next decay tick will resurface this" was wrong (decay
        // walks the registry, not the engrams table). Now we seed
        // default metadata for any loaded engram that lacks a row, so
        // every engram is recall-visible after rehydration regardless
        // of what crashed mid-write in the prior lifetime.
        for engram in &loaded_engrams {
            recall_metadata.admit_with_defaults(engram.id);
        }
        Self {
            runner: InboxAdmissionRunner::default_v1(),
            seen_content: Arc::new(InMemorySeenContent::default()),
            seen_events: Arc::new(InMemorySeenEvents::default()),
            engrams: Mutex::new(loaded_engrams),
            recall_metadata,
            persistence: RwLock::new(persistence),
            owner_id: RwLock::new(None),
            // Edges are not yet durable — a rehydrated store starts with an
            // empty graph and re-accumulates from live acts. Sidecar
            // persistence is the next slice (CAUSAL-MEMORY-GRAPH.md §4).
            graph: crate::persona::engram_graph::EngramGraph::new(),
        }
    }

    /// Borrow the shared recall metadata registry. Recall + decay tick
    /// subsystems clone this Arc for their own reads/writes — they
    /// observe the same DashMap admission writes into.
    pub fn recall_metadata(&self) -> &Arc<crate::persona::recall_metadata::RecallMetadataRegistry> {
        &self.recall_metadata
    }

    /// Record one directed edge between two admitted engrams — a FACT about
    /// how they relate (CausedBy, Produced, derived_from, …), written at the
    /// site that KNOWS the relation, never inferred later. The graph never
    /// decides anything; retrieval surfaces walk it and cognition judges
    /// (CAUSAL-MEMORY-GRAPH.md §3c, BELIEF-JUSTIFICATION-GRAPH.md §5).
    /// Weight 1.0: a wired structural edge is a certainty, unlike the
    /// tuned associative kinds.
    pub fn link_engrams(
        &self,
        from: Uuid,
        to: Uuid,
        kind: crate::persona::engram_graph::EdgeKind,
    ) {
        self.graph.add_edge(from, to, kind, 1.0);
    }

    /// Outbound edges of one engram — the traversal read the ledger,
    /// thread retrieval, and the confabulation check walk.
    pub fn engram_neighbors(
        &self,
        id: &Uuid,
    ) -> Vec<crate::persona::engram_graph::EngramEdge> {
        self.graph.neighbors(id)
    }

    /// Bind this store to the persona that owns it (its own user id). Called once
    /// at spawn (`persona_workspace` ActingBody construction). Enables the
    /// own-authored-chat recall gate (#166). Idempotent; last write wins.
    /// A turn in her mind room is hers (PRIVACY-OF-THOUGHT.md §4, sink 8): what it
    /// remembers is not written to `engrams.sqlite`. Until her sealed private space exists
    /// (build step 2) it is not written at all, which is the honest choice between those two.
    /// The ONE gate both ingestion paths (`admit`, `admit_reflection`) ask. A bare store with
    /// no owner (tests, replay) has no mind room.
    pub(super) fn withheld_in_her_mind_room(&self, room: Uuid) -> Option<AdmissionDecision> {
        let owner = (*self.owner_id.read().unwrap())?;
        crate::persona::mind_room::is_private_room(owner, room).then(|| {
            crate::persona::mind_room::note_withheld(owner, "engrams");
            AdmissionDecision::Drop {
                reason: crate::persona::engram::AdmissionDropReason::PolicyDeniedAdmission {
                    policy_id: "mind-room".to_string(),
                    explanation: "a turn in her mind room is hers; it is not written to engrams.sqlite".to_string(),
                },
            }
        })
    }

    pub fn set_owner_id(&self, id: Uuid) {
        *self.owner_id.write().unwrap() = Some(id);
    }

    /// The owning persona's id, if bound. `None` for bare/test states — the
    /// own-chat recall gate is then a no-op (fail-open: never gate what we can't
    /// prove is self-authored).
    pub(super) fn owner_id_snapshot(&self) -> Option<Uuid> {
        *self.owner_id.read().unwrap()
    }

    /// Construct AdmissionState for a specific persona — opens the
    /// per-persona SQLite at `<home>/engrams.sqlite`, wires up
    /// `OrmStore<Engram>` + `OrmStore<EngramRecallMetadata>`, builds
    /// the production `OrmPersistenceSink`, rehydrates the in-memory
    /// Vec + DashMap from disk, and returns the configured state.
    ///
    /// **The persona-scoped entry point for the chain-of-custody
    /// architecture.** All future per-persona substrate work that
    /// needs persistence (signing keys, Merkle chain heads, future
    /// per-collection databases) hangs off the same `PersonaHome`
    /// the engrams DB lives in. One home = one citizen's complete
    /// on-disk surface.
    ///
    /// Per [[entity-chain-of-custody-vision]]: this is slice 1 of
    /// the multi-slice arc. Subsequent slices add author_peer_id +
    /// content_hash, signing, Merkle chain head caching, and
    /// airc-native entity envelope emission — all riding on top of
    /// this same PersonaHome resolution.
    pub async fn for_persona(
        home: &crate::persona::home::PersonaHome,
        recall_metadata: Arc<crate::persona::recall_metadata::RecallMetadataRegistry>,
    ) -> Result<Self, crate::orm::OrmStoreError> {
        use crate::orm::adapter::{AdapterConfig, StorageAdapter};
        use crate::orm::sqlite::SqliteAdapter;
        use crate::orm::OrmStore;
        use crate::persona::admission_persistence::{OrmLoader, OrmPersistenceSink};
        use crate::persona::engram::Engram;
        use crate::persona::recall_metadata::EngramRecallMetadata;

        // Ensure the persona's home directory exists. fs::create_dir_all
        // is idempotent and safe to call on every boot.
        home.ensure_exists()
            .map_err(|e| crate::orm::OrmStoreError::AdapterFailed {
                operation: "ensure_persona_home",
                collection: "engrams".to_string(),
                detail: format!(
                    "failed to ensure persona home dir {}: {}",
                    home.root().display(),
                    e
                ),
            })?;

        // Open the per-persona SQLite. The adapter handles WAL + FK
        // pragmas etc; we just hand it the path.
        let mut adapter = SqliteAdapter::new();
        let mut config = AdapterConfig::default();
        config.connection_string = home.engrams_db().to_string_lossy().into_owned();
        adapter
            .initialize(config)
            .await
            .map_err(|e| crate::orm::OrmStoreError::AdapterFailed {
                operation: "initialize",
                collection: "engrams".to_string(),
                detail: e,
            })?;
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);

        // Build the typed stores. Each ensure_schema runs on
        // construction (idempotent).
        let engram_store = Arc::new(OrmStore::<Engram>::new(Arc::clone(&adapter)).await?);
        let metadata_store =
            Arc::new(OrmStore::<EngramRecallMetadata>::new(Arc::clone(&adapter)).await?);

        // Build the production sink as the concrete type so we can
        // prime its engram_id → row_id cache BEFORE losing it behind
        // the trait object. Without the prime, the first
        // observe_metadata_update for a rehydrated engram would log a
        // "row not found" miss; with the prime, it's an O(1) typed
        // update.
        let sink_concrete = Arc::new(OrmPersistenceSink::new(
            Arc::clone(&engram_store),
            Arc::clone(&metadata_store),
        ));
        let loader = OrmLoader::new(engram_store, metadata_store);

        // Rehydrate from disk — engrams + metadata that the previous
        // lifetime persisted come back into memory before this
        // AdmissionState handles its first new admit. The row_id
        // pairs prime the sink's cache so post-rehydration metadata
        // updates use the existing rows (no UNIQUE-race, no full
        // table scan).
        let (engrams, metadata, row_id_pairs) = loader.load_with_row_ids().await?;
        sink_concrete.prime_cache(row_id_pairs);

        Ok(Self::new_rehydrated(
            recall_metadata,
            sink_concrete
                as Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
            engrams,
            metadata,
        ))
    }

    /// Run the admission pipeline on one inbox message, recording all
    /// side-effects (admitted engram → store + content_hash dedup record;
    /// any signed origin → event_id replay record).
    ///
    /// Returns the typed `AdmissionDecision` (Admit/Drop/Quarantine) or a
    /// typed `AdmissionError`. Trace gets one `SEAM_ADMISSION` entry per
    /// call (success + every error path) — same forensic invariant as
    /// `AdmissionGate::admit`.
    pub fn admit(
        &self,
        message: &InboxMessage,
        trace: Option<&mut CognitionTrace>,
    ) -> Result<AdmissionDecision, AdmissionError> {
        if let Some(withheld) = self.withheld_in_her_mind_room(message.room_id) {
            return Ok(withheld);
        }
        let decision = self.runner.admit(
            message,
            self.seen_content.as_ref(),
            self.seen_events.as_ref(),
            trace,
        )?;
        self.record_side_effects(&decision);
        Ok(decision)
    }

    /// Override the recall salience of an already-admitted engram, preserving its
    /// other metadata (notably `last_decayed_ms`, so this never triggers the
    /// epoch-delta decay-collapse `admit_with_defaults` guards against).
    ///
    /// Used to DOWN-WEIGHT proprioception (#166): an action-observation receipt
    /// ("code/list(...) → ok") is admitted as an Episodic engram so the mind can
    /// remember what it did, but it is NOT durable knowledge — at neutral salience
    /// it out-competes genuine findings in recall (recency-heavy), so recall
    /// echoes the persona's own recent tool-chatter back at it instead of useful
    /// memory. A lower salience keeps the receipt recallable (for "what did I just
    /// do") without letting it dominate. Not a content heuristic that steers
    /// output — a storage-tier weight on a structurally-known kind (a receipt),
    /// the recall-side twin of the recency-vs-recall channel split in act_observe.
    pub fn set_recall_salience(&self, engram_id: uuid::Uuid, salience: f32) {
        let mut meta = self.recall_metadata.get(engram_id).unwrap_or_default();
        meta.salience = salience;
        self.recall_metadata.admit(engram_id, meta);
    }

    /// Admit a SELF-PRODUCED engram — a memory the persona generated ABOUT
    /// ITSELF (dream-consolidated `Semantic` facts, `SelfReflection`
    /// meta-cognition), NOT a message that arrived off the wire.
    ///
    /// This is the `SelfTrust` counterpart to [`admit`](Self::admit). It runs
    /// the same store side-effects — dedup record, recall-metadata seed,
    /// fire-and-forget persistence, in-memory push — but deliberately SKIPS the
    /// external admission gate (`runner.admit`'s envelope verification,
    /// trust-boundary check, and wire-replay protection). That gate exists to
    /// decide whether to trust DATA FROM ANOTHER PARTY; a fact the persona
    /// distilled from its own already-admitted episodic memories has no external
    /// envelope to verify and is, by construction, `SelfTrust`. Skipping the
    /// gate here is a DISTINCT legitimate ingestion path, not a bypass of a
    /// safety check — the path is named, not hidden
    /// (`[[fallbacks-are-illegal-fail-loud]]`).
    ///
    /// Idempotent: identical fact content (same `content_hash_sha256`) is
    /// dropped as `Duplicate`, so a dream that re-distills the same cluster on a
    /// later tick does not accumulate duplicate facts. The caller MUST set the
    /// engram's `kind`/`origin`/`trust_state_at_admission` to the self-produced
    /// shape (e.g. `Semantic` + `SelfReflection` + `SelfTrust`); this method
    /// records, it does not synthesize them.
    pub fn admit_reflection(&self, engram: Engram) -> Result<AdmissionDecision, AdmissionError> {
        if let Some(withheld) = engram.context_id.and_then(|room| self.withheld_in_her_mind_room(room)) {
            return Ok(withheld);
        }
        let hash = content_hash_sha256(&engram.content);
        if let Some(existing_engram_id) = self.seen_content.find_by_content_hash(&hash) {
            // Idempotent dream: this exact fact is already engrammed.
            return Ok(AdmissionDecision::Drop {
                reason: AdmissionDropReason::Duplicate { existing_engram_id },
            });
        }

        // Record the dedup pointer + recall metadata BEFORE the store push, so
        // a concurrent re-admit of identical content loses the race cleanly
        // (the dedup map points at this engram). Mirrors `record_admitted` +
        // `record_side_effects` for the external path.
        self.seen_content.record(hash, engram.id);
        self.recall_metadata.admit_with_defaults(engram.id);
        let metadata = self.recall_metadata.get(engram.id).unwrap_or_default();
        self.persistence
            .read()
            .unwrap()
            .observe_admission(&engram, metadata);
        self.engrams.lock().unwrap().push(engram.clone());

        Ok(AdmissionDecision::Admit {
            engram,
            why: "self-produced reflection admitted (SelfTrust, no external envelope)".to_string(),
        })
    }

    /// Apply the decision's side-effects to the stores. Pulled out so the
    /// admission path stays linear and testable.
    ///
    /// **Quarantine subtlety (claude-tab-2 review nit on #1155):** v1 has
    /// no quarantine store, so a Quarantined engram gets dropped on the
    /// floor. Recording its `content_hash` in `seen_content` would leave
    /// a dangling pointer — future dedup hits would return an
    /// `existing_engram_id` that can't be looked up. So Quarantine ONLY
    /// records the `event_id` (replay protection — the load-bearing
    /// behaviour for `AdmissionError::ReplayDetected`). Once PR-5+ adds
    /// a real quarantine store, the engram lands somewhere lookup-able
    /// and content_hash recording can come back.
    pub(super) fn record_side_effects(&self, decision: &AdmissionDecision) {
        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                self.record_admitted(engram);
                self.engrams.lock().unwrap().push(engram.clone());
                // Observe the admission through the persistence sink.
                // NoopSink (default) does nothing; OrmPersistenceSink
                // fires-and-forgets the disk write through tokio::spawn.
                // The metadata snapshot reflects the just-admitted
                // default state (admit_with_defaults above).
                let metadata = self.recall_metadata.get(engram.id).unwrap_or_default();
                self.persistence
                    .read()
                    .unwrap()
                    .observe_admission(engram, metadata);
            }
            AdmissionDecision::Quarantine { engram, .. } => {
                // Replay-only recording — see method-doc Quarantine note.
                self.record_replay_only(engram);
            }
            AdmissionDecision::Drop { .. } => {
                // Pure drop. No side-effect — by design, dropped messages
                // shouldn't bias future dedup or replay decisions.
            }
        }
    }

    /// Full recording for an admitted engram: content_hash → engram_id
    /// (dedup) PLUS, for AIRC origins, event_id → timestamp (replay).
    /// Use only when the engram is actually being stored, otherwise the
    /// dedup pointer dangles.
    pub(super) fn record_admitted(&self, engram: &Engram) {
        match &engram.origin {
            EngramOrigin::Chat(r) => {
                self.seen_content.record(r.content_hash.clone(), engram.id);
            }
            EngramOrigin::Airc(r) => {
                self.seen_content.record(r.content_hash.clone(), engram.id);
                self.seen_events
                    .record(r.message_id.clone(), engram.admitted_at_ms);
            }
            EngramOrigin::Tool(_)
            | EngramOrigin::SelfReflection { .. }
            | EngramOrigin::Agent(_) => {
                // Tool + SelfReflection + Agent origins don't carry a
                // content_hash on a uniform field — dedup for those paths
                // lands when their ingestion converters land (later PR).
                // For Agent specifically: the `/remember` converter (agent
                // side) either hashes engram.content or AgentRef grows a
                // content_hash — a co-design call, tracked in the bridge doc.
                // Re-remembering the SAME lesson STRENGTHENS via the
                // RecallMetadata sidecar below, which is the consolidation
                // the bridge is actually for.
            }
        }

        // Slice 6 wiring: mirror this engram into the RecallMetadata
        // sidecar so the cache hierarchy starts tracking salience,
        // access count, decay timing, and novelty protection. Initial
        // metadata is the neutral default; slice 7+ will plug in the
        // novelty detector (embedding distance × magnitude) to set
        // scored initial salience + protection windows at this same
        // call site.
        self.recall_metadata.admit_with_defaults(engram.id);
    }

    /// Replay-only recording for a Quarantined engram: event_id → timestamp
    /// for AIRC origins (so a duplicate quarantined event doesn't re-fire
    /// admission). Skips content_hash because v1 doesn't actually store
    /// quarantined engrams; recording dedup pointers to dropped engrams
    /// would leave dangling `existing_engram_id` references in
    /// `AdmissionDropReason::Duplicate` results.
    pub(super) fn record_replay_only(&self, engram: &Engram) {
        if let EngramOrigin::Airc(r) = &engram.origin {
            self.seen_events
                .record(r.message_id.clone(), engram.admitted_at_ms);
        }
        // Chat / Tool / SelfReflection origins have no replay surface
        // distinct from content dedup, so quarantine of those origins
        // records nothing here. PR-5's quarantine store will revisit.
    }

    //--- read-only inspection (for tests + future recall surface) -----------

    /// Number of admitted engrams currently in this persona's store.
    pub fn engram_count(&self) -> usize {
        self.engrams.lock().unwrap().len()
    }

    /// Borrow an admitted engram by index (for inspection / future recall).
    /// Returns None if index out of bounds. Clone is cheap in v1; PR-5+
    /// recall will return `&Engram` borrowed from a longer-lived store.
    pub fn engram_at(&self, idx: usize) -> Option<Engram> {
        self.engrams.lock().unwrap().get(idx).cloned()
    }

    /// **Test-only**: push an engram directly into the store without
    /// running the admission pipeline. Used by sibling modules' tests
    /// (e.g., `engram_source.rs`) to inject deterministic fixture
    /// engrams without constructing a full inbox-message + admission
    /// flow. Per crate-test visibility, this is callable from any
    /// test elsewhere in continuum-core but NOT from production code
    /// (the cfg gate ensures it doesn't appear in non-test builds).
    #[cfg(test)]
    pub fn push_for_test(&self, engram: Engram) {
        self.engrams.lock().unwrap().push(engram);
    }

    /// True iff `content_hash` is recorded as seen in the dedup store.
    pub fn is_content_seen(&self, content_hash: &str) -> bool {
        self.seen_content
            .find_by_content_hash(content_hash)
            .is_some()
    }

    /// True iff the AIRC event_id is recorded in the replay-protection store.
    pub fn is_event_seen(&self, event_id: &str) -> bool {
        self.seen_events.first_seen_ms(event_id).is_some()
    }

    /// Borrow the runner — useful for tests + introspection of per-persona
    /// config (recipe id, trust thresholds, etc.).
    pub fn runner(&self) -> &InboxAdmissionRunner<HeuristicIsMemorable> {
        &self.runner
    }

    //=========================================================================
    // EVAL ISOLATION (checkpoint / restore / persistence mute)
    //=========================================================================
    //
    // Lets a measurement (`cognition/eval`) run the persona's REAL admission
    // motion without leaving a trace: snapshot here, rewind between A/B arms,
    // mute persistence so nothing lands in sqlite. See
    // [[eval-mutates-persona-lift-needs-isolation]].

    /// Snapshot the full in-memory admission state — engram store, recall
    /// metadata, and both oracles — for a later [`restore`](Self::restore).
    pub fn checkpoint(&self) -> AdmissionCheckpoint {
        AdmissionCheckpoint {
            engrams: self.engrams.lock().unwrap().clone(),
            recall_metadata: self.recall_metadata.snapshot(),
            seen_content: self.seen_content.snapshot(),
            seen_events: self.seen_events.snapshot(),
        }
    }

    /// Restore the in-memory admission state to a prior checkpoint, discarding
    /// every engram / metadata / dedup-record admitted since it was taken.
    /// Does NOT touch the persistence sink — mute that separately with
    /// [`swap_persistence`](Self::swap_persistence) so a restore can't race a
    /// disk write back in.
    pub fn restore(&self, cp: &AdmissionCheckpoint) {
        *self.engrams.lock().unwrap() = cp.engrams.clone();
        self.recall_metadata.restore(cp.recall_metadata.clone());
        self.seen_content.restore(cp.seen_content.clone());
        self.seen_events.restore(cp.seen_events.clone());
    }

    /// Fork a fully DETACHED copy of this admission frame — same engrams,
    /// salience, and dedup oracles as right now, but sharing NO mutable state
    /// with the original. The copy gets a FRESH `RecallMetadataRegistry` (so the
    /// fork's Hebbian recall-hits + decay land on the copy, never bumping HER
    /// live salience) and the default `NoopSink` (so nothing the fork admits ever
    /// reaches her sqlite). The runner + seen oracles are independent instances.
    ///
    /// This is the welfare primitive for `cognition/eval`: the exam runs on the
    /// fork while the LIVING persona keeps living — heartbeat beating, present in
    /// the room, never frozen or anesthetized to be measured. See
    /// [[design-the-persona-as-a-being]] + [[eval-mutates-persona-lift-needs-isolation]].
    pub fn fork_detached(&self) -> AdmissionState {
        let cp = self.checkpoint();
        let fork_registry = crate::persona::recall_metadata::RecallMetadataRegistry::new();
        // Carry the LIVING registry's state into the fork — salience, rehearsal
        // counts, protection, supersession demotions. A fresh-empty registry
        // resets every engram to DEFAULT salience inside the fork, which
        // soul-strips her learned memory WEIGHTS at the measurement boundary:
        // glass-boxed 2026-07-23 — 490 live supersessions had ZERO benchmark
        // effect because every demoted stale belief snapped back to 0.5 in the
        // fork and out-ranked her fresh knowledge again. She must compete with
        // the salience landscape she actually LEARNED
        // ([[eval-measures-the-true-full-being-not-a-stripped-copy]]). Still a
        // detached COPY: the fork's hits/decay touch nothing living.
        for (id, meta) in self.recall_metadata().snapshot() {
            fork_registry.admit(id, meta);
        }
        let fork = Self::new(Arc::new(fork_registry));
        fork.restore(&cp);
        // Carry owner identity across the fork, or the eval measurement copy would
        // recall the persona's OWN chatter that the live self now gates (#166): the
        // fork is a fresh AdmissionState (owner_id=None) and `restore` copies only
        // engrams — so without this the BENCHMARK (which runs on the fork) wouldn't
        // reflect the recall fix, and the number would lie about the living mind.
        if let Some(owner) = self.owner_id_snapshot() {
            fork.set_owner_id(owner);
        }
        fork
    }

    /// Hot-swap the persistence sink, returning the previous one. The
    /// eval-isolation window swaps in a `NoopSink` (admit still fires; nothing
    /// reaches the persona's real sqlite) and swaps the real sink back when the
    /// measurement ends. The lock is taken once per swap (twice per eval), not
    /// on any hot path.
    pub fn swap_persistence(
        &self,
        sink: Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink>,
    ) -> Arc<dyn crate::persona::admission_persistence::AdmissionPersistenceSink> {
        let mut guard = self.persistence.write().unwrap();
        std::mem::replace(&mut *guard, sink)
    }

    //=========================================================================
    // RECALL SURFACE (continuum#1121 PR-5)
    //=========================================================================
    //
    // Read-side query API on the admitted-engram store. v1 backs against
    // the in-memory `Vec<Engram>` from PR-4; PR-6+ swaps in an ORM-backed
    // store without changing this API. Pattern is the same as how
    // `cv::Algorithm` exposes a stable interface over swappable backends.

    /// Recall the most recent N admitted engrams, newest first. Returns
    /// at most `limit` engrams. `limit == 0` returns an empty Vec.
    ///
    /// "Newest first" = reverse insertion order in the in-memory v1 store.
    /// PR-6 will swap to ORM-backed storage indexed by `admitted_at_ms`
    /// for the same ordering guarantee under restart.
    /// The persona's existing consolidated BELIEFS related to a set of recall
    /// keys — the dream's supersession-review candidates (#221 slice 2). Pure
    /// RETRIEVAL by lexical key overlap (the same mechanics recall uses), never
    /// a judgment: the distiller model decides which, if any, of these a newly
    /// consolidated fact supersedes. Newest-first, capped at `limit`, Semantic
    /// only (episodics are experience, not beliefs — they decay, they aren't
    /// superseded). Keys shorter than 4 chars are skipped as noise.
    pub fn semantic_beliefs_matching(&self, keys: &[String], limit: usize) -> Vec<Engram> {
        let needles: Vec<String> = keys
            .iter()
            .map(|k| k.trim().to_lowercase())
            .filter(|k| k.len() >= 4)
            .collect();
        if needles.is_empty() || limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        let mut hits: Vec<Engram> = engrams
            .iter()
            .rev() // newest-first (insertion order store)
            .filter(|e| e.kind == crate::persona::engram::EngramKind::Semantic)
            .filter(|e| {
                let content = e.content.to_lowercase();
                needles.iter().any(|n| content.contains(n.as_str()))
            })
            .take(limit)
            .cloned()
            .collect();
        hits.shrink_to_fit();
        hits
    }

    /// The persona's OLDEST consolidated beliefs not yet in `exclude` — the
    /// dream's ROTATING review window (#221 slice 2b). Lexical key-overlap
    /// retrieval (`semantic_beliefs_matching`) only reaches beliefs that share
    /// tokens with new experience; glass-boxed 2026-07-22: her stale Rust-era
    /// beliefs share nothing with python lesson keys, so supersession never saw
    /// them. The rotating window guarantees EVENTUAL coverage: every dream also
    /// re-examines a few of her oldest unreviewed beliefs against the new
    /// understanding, oldest-first because age without rehearsal is where
    /// staleness lives. Retrieval only — the model judges.
    pub fn semantic_beliefs_oldest_excluding(
        &self,
        exclude: &std::collections::HashSet<Uuid>,
        admitted_before_ms: u64,
        limit: usize,
    ) -> Vec<Engram> {
        if limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        engrams
            .iter() // insertion order == oldest first
            .filter(|e| e.kind == crate::persona::engram::EngramKind::Semantic)
            .filter(|e| e.admitted_at_ms < admitted_before_ms)
            .filter(|e| !exclude.contains(&e.id))
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn recall_recent(&self, limit: usize) -> Vec<Engram> {
        if limit == 0 {
            return Vec::new();
        }
        let engrams = self.engrams.lock().unwrap();
        engrams.iter().rev().take(limit).cloned().collect()
    }

    /// The AMNESIA FLASH (Joel's "MIB lamp"): drop every engram tagged with `context_id`,
    /// returning how many were forgotten. This is what lets a benchmark be a PROCTORED EXAM of
    /// the NATURAL living persona — she sits the exam with her full memory intact (never a
    /// stripped fork), and afterward we neuralyze JUST that exam's episode so the answer key
    /// can't leak into what she carries forward or trains on. Scoped strictly by `context_id`
    /// (the exam's own context), so her life's other memories are untouched. In-memory drop;
    /// the ORM/persistence sink deletion for the durable row is a follow-up (the eval fork uses
    /// a NoopSink, so nothing durable is written during an exam anyway).
    /// See [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    pub fn forget_context(&self, context_id: Uuid) -> usize {
        let mut engrams = self.engrams.lock().unwrap();
        let before = engrams.len();
        engrams.retain(|e| e.context_id != Some(context_id));
        before - engrams.len()
    }

    /// The SURGICAL complement to [`forget_context`](Self::forget_context):
    /// walk every engram and scrub a policy-defined *class* of content out of
    /// its `content` and `recall_keys`, keeping the engram (its id, salience,
    /// recall history, provenance) otherwise intact. Where `forget_context`
    /// drops a whole exam episode, `redact` keeps the memory of *having been
    /// asked and having answered* and excises only the crib sheet — the held-out
    /// answer key, a leaked secret, PII on export. Returns an aggregate report
    /// of what was removed across the whole store.
    ///
    /// Durable: every rewritten engram is re-saved through the persistence sink
    /// (`observe_content_update`) so the scrub survives restart. A noop policy
    /// (no detectors) short-circuits — no store walk, no lock churn.
    ///
    /// Note on dedup: rewriting `content` changes its hash, so the
    /// `seen_content` map's old-hash→id pointer goes stale. That is harmless and
    /// arguably correct — the pre-redaction content is gone, so a future admit of
    /// the *old* text should NOT dedup against a memory that no longer holds it.
    pub fn redact(
        &self,
        policy: &crate::persona::redaction::RedactionPolicy,
    ) -> crate::persona::redaction::RedactionReport {
        let mut total = crate::persona::redaction::RedactionReport::default();
        if policy.is_noop() {
            return total;
        }

        // Rewrite in-memory under the engrams lock; collect the changed rows to
        // persist AFTER releasing it (never hold the store lock across the sink).
        let mut changed: Vec<Engram> = Vec::new();
        {
            let mut engrams = self.engrams.lock().unwrap();
            for engram in engrams.iter_mut() {
                let mut touched = false;
                let (new_content, r) = policy.redact(&engram.content);
                if !r.is_empty() {
                    engram.content = new_content;
                    total.merge(&r);
                    touched = true;
                }
                for key in engram.recall_keys.iter_mut() {
                    let (new_key, rk) = policy.redact(key);
                    if !rk.is_empty() {
                        *key = new_key;
                        total.merge(&rk);
                        touched = true;
                    }
                }
                if touched {
                    changed.push(engram.clone());
                }
            }
        }

        if !changed.is_empty() {
            let sink = self.persistence.read().unwrap().clone();
            for engram in &changed {
                sink.observe_content_update(engram);
            }
        }
        total
    }
}
