#![cfg(test)]

// The confirmed-save ledger: its page format, legacy migration and resident resume.
mod ledger;
mod adopted_checkpoint;

    // what this catches (card 9e3350bd; the 5090 2026-10-11, build 6148): the heartbeat's smoke
    // probe ran on a one-slot lane's only slot, replacing the resident's KV, and the next save
    // wrote the probe's 72 tokens as her page. With no scratch slot and a resident seated, the
    // probe must not run; with a scratch slot, or nobody seated, it may.
    #[test]
    fn the_smoke_probe_never_runs_on_a_seated_residents_only_slot() {
        let one = KvSlotPool::new("test://one-slot", 1);
        assert_eq!(one.scratch_slot(), None, "test: a one-slot pool has no scratch");
        assert!(!one.smoke_would_overwrite_a_resident(), "nobody seated: probing costs no one");
        one.holders.lock().insert(0, key(1, 2));
        assert!(one.smoke_would_overwrite_a_resident(), "a seated resident on the only slot");
        let many = KvSlotPool::new("test://four-slot", 4);
        if many.scratch_slot().is_some() {
            many.holders.lock().insert(0, key(1, 2));
            assert!(!many.smoke_would_overwrite_a_resident(), "a scratch slot takes the probe");
        }
    }
    use super::*;

    // what this catches (IntelMac, 2026-10-11 02:46Z on #4930: the first sizing after a
    // restart used the KV estimate, 1,792 MiB, beside saved states of up to 865 MB): a
    // restarted core measures its states from the pages already on disk. Each sample is a
    // page plus its checkpoint sidecar; other files are not samples.
    #[test]
    fn a_restarted_core_measures_its_states_from_the_pages_on_disk() {
        let dir = tempfile::tempdir().expect("test: page dir");
        std::fs::write(dir.path().join("a-p1-r1.bin"), vec![0u8; 700]).expect("test: page");
        std::fs::write(dir.path().join("a-p1-r1.bin.ckpt"), vec![0u8; 160]).expect("test: sidecar");
        std::fs::write(dir.path().join("a-p1-r1.bin.lora"), vec![0u8; 5]).expect("test: adapter sidecar");
        std::fs::write(dir.path().join("a-p2-r2.bin"), vec![0u8; 300]).expect("test: page");
        std::fs::write(dir.path().join("saved-ledger.json"), vec![0u8; 9_999]).expect("test: not a page");
        let mut states = newest_page_states(dir.path(), STATE_SAMPLES).expect("test: readable dir");
        states.sort_unstable();
        assert_eq!(states, vec![300, 865], "each page with its sidecars, the ledger excluded");
        assert_eq!(newest_page_states(dir.path(), 1).expect("test: readable dir").len(), 1, "at most n");
        assert!(newest_page_states(&dir.path().join("absent"), 4).expect("test: absent dir").is_empty(), "no dir, no samples");
        let not_a_dir = dir.path().join("saved-ledger.json");
        assert!(newest_page_states(&not_a_dir, 4).is_err(), "an unreadable store is an error, never an empty one");
    }

    // what this catches (card 37eb1925, Kimi's node 2026-10-15: a fresh core sized its first
    // lane from the KV estimate, 3,232 MiB, because the pages on disk were only read at the
    // spawn that sizing precedes, and the re-home relaunched it at 9,477 MiB): the exact dir a
    // sizing reads is measured before it reads, among more same-model geometries than the sample
    // cache holds; a known dir is not read again; a geometry made after an earlier seed is read
    // for itself; a dir evicted from the bounded cache is read again, alone (root's fixtures on
    // #5060). A dir with no pages is remembered empty; a live sample is never replaced by the disk's.
    #[tokio::test]
    async fn the_first_sizing_after_a_restart_reads_the_pages_on_disk() {
        let sampler = std::sync::Arc::new(StateSampler::default());
        let root = tempfile::tempdir().expect("test: page store");
        let geometry = |key: usize| root.path().join(format!("test-37eb1925--c{key}"));
        let reads_of = |dir: &std::path::Path| sampler.reads.lock().iter().filter(|d| d.as_path() == dir).count();
        // An asker waits at most REPORTER_TIMEOUT and yields to another dir's read in flight. Re-ask until the dir is known: a re-ask joins its read or waits
        // for the slot, never starting a second read of the same dir.
        let seeded = |dir: std::path::PathBuf| {
            let sampler = sampler.clone();
            async move {
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                while !sampler.known(&dir) {
                    sampler.seed(&dir).await;
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("test: the read lands");
            }
        };
        for key in 0..(STATE_DIRS + 3) {
            std::fs::create_dir(geometry(key)).expect("test: same-model geometry");
            std::fs::write(geometry(key).join("a-p1-r1.bin"), vec![0u8; 1_000 + key]).expect("test: page");
        }
        let desired = geometry(0);
        std::fs::write(desired.join("a-p1-r1.bin.ckpt"), vec![0u8; 900]).expect("test: sidecar");
        assert_eq!(sampler.measured(&desired), None, "a fresh core has measured nothing");
        seeded(desired.clone()).await;
        assert_eq!(sampler.measured(&desired), Some(1_900), "the exact dir, no spawn first");
        seeded(desired.clone()).await;
        assert_eq!(reads_of(&desired), 1, "a known dir is not read again");

        let later = geometry(STATE_DIRS + 7);
        std::fs::create_dir(&later).expect("test: a geometry made after the seed");
        std::fs::write(later.join("a-p2-r2.bin"), vec![0u8; 333]).expect("test: page");
        seeded(later.clone()).await;
        assert_eq!(sampler.measured(&later), Some(333), "a new geometry is read for itself");

        for key in 1..(STATE_DIRS + 3) {
            seeded(geometry(key)).await;
        }
        assert_eq!(sampler.measured(&desired), None, "evicted from the bounded cache");
        seeded(desired.clone()).await;
        assert_eq!(sampler.measured(&desired), Some(1_900), "an evicted dir is read again");
        assert_eq!(reads_of(&desired), 2, "and read alone, not by a census of its model");

        let mut samples = StateSamples::new();
        let empty = std::path::Path::new("/pages/fresh");
        seed_dir_in(&mut samples, empty, Vec::new());
        assert!(state_dir_known_in(&samples, empty) && measured_state_bytes_in(&samples, empty).is_none());
        let live = std::path::Path::new("/pages/live");
        note_state_bytes_in(&mut samples, live, 7);
        seed_dir_in(&mut samples, live, vec![1, 2, 3]);
        assert_eq!(measured_state_bytes_in(&samples, live), Some(7), "the live sample stands");
        for i in 0..(STATE_DIRS * 2) {
            seed_dir_in(&mut samples, std::path::Path::new(&format!("/pages/s{i}")), vec![1]);
        }
        assert!(samples.len() <= STATE_DIRS, "seeding is bounded by dirs too");
    }

    // what this catches (root on #5060, ce7371d37): a stuck read per geometry when the volume stops
    // answering, and a slot never released once a live save made its dir known after the asker
    // timed out. One read is in flight; another dir starts no worker while it runs; the read lands
    // and releases the slot with nobody waiting on it; and the live sample survives it.
    #[tokio::test]
    async fn one_stuck_read_holds_one_slot_and_releases_it_unwatched() {
        let sampler = std::sync::Arc::new(StateSampler::default());
        let root = tempfile::tempdir().expect("test: page store");
        let stuck = root.path().join("test-5060-stuck--c1");
        let other = root.path().join("test-5060-other--c2");
        let reads_of = |dir: &std::path::Path| sampler.reads.lock().iter().filter(|d| d.as_path() == dir).count();
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let gate = std::sync::Arc::new(parking_lot::Mutex::new(gate));
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while reads_of(&stuck) == 0 {
                let gate = gate.clone();
                sampler.seed_with(&stuck, move |_| {
                    let _ = gate.lock().recv(); // released by the test: a volume that stopped answering
                    Ok(vec![111])
                })
                .await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("test: the stuck read is admitted");
        assert_eq!(sampler.measured(&stuck), None, "still reading: unmeasured, not guessed");
        sampler.seed_with(&other, |_| Ok(vec![222])).await;
        assert_eq!(reads_of(&other), 0, "another dir starts no worker while one read is in flight");
        sampler.note(&stuck, 999);
        // A separate owner can churn its entire bounded cache while this read is
        // blocked. Its eviction must never erase this owner's live sample.
        let churning = StateSampler::default();
        churning.note(&stuck, 333);
        for key in 0..(STATE_DIRS + 3) {
            churning.note(&root.path().join(format!("other-owner-{key}")), 222);
        }
        assert_eq!(churning.measured(&stuck), None, "other owner really evicted its sample");
        assert_eq!(sampler.measured(&stuck), Some(999), "independent owner retains its live sample");
        release.send(()).expect("test: release the stuck read");
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while sampler.reading(&stuck) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("test: the read releases its slot with nobody waiting on it");
        assert_eq!(sampler.measured(&stuck), Some(999), "the live sample outranks the disk's");
        assert_eq!(reads_of(&stuck), 1, "one read, however many asks");
    }

    // what this catches (Fable on #4930): a node-wide sample, where a second model's lane on
    // the same node would be sized by the hybrid 27B's 4-6 GB states. Samples are per page
    // dir (one model at one geometry), each dir answers only for itself, and both bounds hold.
    #[test]
    fn measured_state_sizes_answer_only_for_their_own_page_dir() {
        let mut samples = StateSamples::new();
        let big = std::path::Path::new("/pages/qwen-27b--c49152");
        let small = std::path::Path::new("/pages/qwen-1.5b--c32768");
        note_state_bytes_in(&mut samples, big, 5_500_000_000);
        note_state_bytes_in(&mut samples, small, 120_000_000);
        assert_eq!(measured_state_bytes_in(&samples, big), Some(5_500_000_000));
        assert_eq!(measured_state_bytes_in(&samples, small), Some(120_000_000), "never the 27B's size");
        assert_eq!(measured_state_bytes_in(&samples, std::path::Path::new("/pages/other")), None, "an unwritten dir: nothing measured");
        for i in 0..(STATE_SAMPLES * 2) {
            note_state_bytes_in(&mut samples, small, 100 + i as u64);
        }
        assert_eq!(measured_state_bytes_in(&samples, small), Some(100 + (STATE_SAMPLES * 2 - 1) as u64), "only the recent samples count");
        for i in 0..(STATE_DIRS * 2) {
            note_state_bytes_in(&mut samples, std::path::Path::new(&format!("/pages/m{i}")), 1);
        }
        assert!(samples.len() <= STATE_DIRS, "bounded by dirs");
    }

    /// A distinct page per (p, r). The pool is agnostic to the key class, so these tests
    /// key distinct pages as forks of one persona; one page per mind is tested on its own.
    fn key(p: u128, r: u128) -> PageKey {
        PageKey::fork(Uuid::from_u128(p), Uuid::from_u128(r)).expect("non-nil test ids")
    }

    /// A resident mind's page: what the ledger persists.
    fn mind(p: u128) -> PageKey {
        PageKey::mind(Uuid::from_u128(p)).expect("non-nil test id")
    }

    // what this catches (card 6b28a445): a relaunch that cannot save (here, quarantined
    // paging) lost residents' KV with no record, so their next cold turn read as a lost page;
    // and, per Codex's review, a record made when the kill was SENT reports a loss that may
    // not have happened. Through the real replacement path: nothing is recorded while the
    // engine is only quarantined, even on re-entry; its observed exit records only the
    // minds held without a confirmed save, once; a re-entry after the exit records nothing
    // more; the record survives the pool's replacement and is taken exactly once.
    #[tokio::test]
    async fn a_relaunch_records_unsaved_residents_only_at_the_observed_exit_and_once() {
        let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
        let endpoint = dir.endpoint("test://relaunch");
        let mut transition = endpoint.transition().await;
        let generation = transition.start_generation().expect("first generation");
        let pool = Arc::new(KvSlotPool::new("test://relaunch", 4));
        pool.holders.lock().insert(0, mind(1));
        pool.holders.lock().insert(1, mind(2));
        pool.holders.lock().insert(2, key(3, 4));
        pool.note_saved(mind(2));
        endpoint.state.lock().pool = Some(Some(pool));
        endpoint.quarantine_paging();
        let client = reqwest::Client::new();

        for _attempt in 0..2 {
            transition
                .checkpoint_before_replacement(&client, "test://relaunch", &|| true)
                .await
                .expect("quarantined paging has nothing to save");
            assert_eq!(endpoint.discarded.lock().take(&mind(1)), None, "the kill is not the death");
        }
        generation.observed_exit();
        transition
            .checkpoint_before_replacement(&client, "test://relaunch", &|| true)
            .await
            .expect("an exited engine has nothing to save");
        drop(transition);
        // The next generation's pool replaces this one; the record is the endpoint's.
        endpoint.state.lock().pool = Some(Some(Arc::new(KvSlotPool::new("test://relaunch", 4))));
        endpoint.state.lock().ready.send_replace(true); // the replacement verified ready

        let admission = endpoint.admit().await.expect("an open endpoint admits");
        assert_eq!(admission.take_relaunch_discard(&mind(1)), Some(DiscardReason::PagingUncertain));
        assert_eq!(admission.take_relaunch_discard(&mind(1)), None, "recorded once, taken once");
        assert_eq!(admission.take_relaunch_discard(&mind(2)), None, "a saved page restores; nothing was lost");
        assert_eq!(admission.take_relaunch_discard(&key(3, 4)), None, "a fork dies with its core anyway");
    }

    // what this catches (card 6b28a445, Codex's review): minds discarded by one relaunch that
    // never return stayed recorded forever, across every later generation. Retention is one
    // pool's worth of slots: the latest relaunch is kept whole, older records are evicted
    // oldest first, and an empty capture evicts nothing.
    #[tokio::test]
    async fn discard_records_keep_one_pool_of_slots_across_generations() {
        let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
        let endpoint = dir.endpoint("test://retention");
        let relaunch = |minds: &[u128]| {
            let pool = Arc::new(KvSlotPool::new("test://retention", 3));
            for (slot, m) in minds.iter().enumerate() {
                pool.holders.lock().insert(slot as u32, mind(*m));
            }
            endpoint.state.lock().pool = Some(Some(pool));
            endpoint.pending_discard(DiscardReason::PagingUncertain).commit();
        };
        // One mind per relaunch, so "oldest" is defined: the first is evicted by the fourth.
        for m in [1, 2, 3, 4] {
            relaunch(&[m]);
        }
        {
            let mut log = endpoint.discarded.lock();
            assert_eq!(log.take(&mind(1)), None, "oldest evicted first");
            for kept in [2, 3, 4] {
                assert_eq!(log.take(&mind(kept)), Some(DiscardReason::PagingUncertain), "mind {kept} kept");
            }
        }
        relaunch(&[5, 6]);
        relaunch(&[7, 8, 9]); // a whole pool of slots: the latest relaunch is kept whole
        relaunch(&[]); // nothing lost evicts nothing
        let mut log = endpoint.discarded.lock();
        for evicted in [5, 6] {
            assert_eq!(log.take(&mind(evicted)), None, "mind {evicted} evicted by a newer relaunch");
        }
        for kept in [7, 8, 9] {
            assert_eq!(log.take(&mind(kept)), Some(DiscardReason::PagingUncertain), "mind {kept} kept");
        }
    }

    // regression for card 6102ac09 (BigMama, 2026-09-26 12:28Z): a turn that reached a
    // loading engine was refused instantly — 16 of Kimi's 26 requests in 90 min failed
    // inside a 90 s launch window, half of them on the cross-grid fallback.
    // what this catches: a turn HOLDS through an engine transition and is admitted the
    // moment the writer reopens; only past its patience is it refused; the operator
    // path (`admit`) keeps its instant refusal.
    #[tokio::test]
    async fn a_turn_holds_through_engine_readiness_and_is_admitted_when_the_writer_reopens() {
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        let endpoint = dir.endpoint("test://warming");
        let contract = KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec!["fixture.lora".into()],
            page_dir: Some("fixture-pages".into()),
            context: 32768,
            slots: 2,
            cache_type: Some("q8_0".into()),
            flash_attn: Some(true),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        // The launch: the writer closes the endpoint and holds it until the engine is verified.
        let launch = endpoint.transition().await;
        assert!(endpoint.admit().await.is_err(), "the operator path refuses instantly");
        let held = endpoint.admit_when_ready(std::time::Duration::from_secs(5));
        tokio::pin!(held);
        assert!(
            futures::poll!(&mut held).is_pending(),
            "a turn holds through the launch, it is not refused"
        );
        let generation = launch.start_generation().expect("test: engine generation");
        launch
            .ready("test://warming", &generation, contract)
            .expect("test: verified engine");
        drop(launch);
        let admitted = held.await.expect("admitted the moment the writer reopened");
        assert!(admitted.check_ready().is_ok());
        drop(admitted);
        // Past its patience the refusal is the old one, with the wait in it.
        let _closed = endpoint.transition().await;
        let refused = endpoint
            .admit_when_ready(std::time::Duration::from_millis(50))
            .await;
        assert!(
            refused.is_err_and(|e| e.contains("suspended pending engine readiness (held 0 s)")),
            "a turn is refused only past its patience"
        );
    }

    // regression for Cormac's blocker on #4411 (the M5 grow chain): transitions back to
    // back. The first cut waited for ONE reopen and then called `admit()` once; a second
    // transition that closed the endpoint behind the first one's reopen made that call
    // refuse a turn with patience left.
    // what this catches: a held turn stays held across a re-close and is admitted by the
    // transition that finally leaves the endpoint open, on the one deadline it started
    // with. The interleaving is deterministic: the gate is FIFO, so the turn's read
    // lease is granted by the first writer's drop before the second writer can take it,
    // and the second writer has already closed the endpoint by then.
    #[tokio::test]
    async fn a_held_turn_survives_back_to_back_transitions_with_patience_left() {
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        let endpoint = dir.endpoint("test://grow-chain");
        let contract = KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec![],
            page_dir: Some("fixture-pages".into()),
            context: 32768,
            slots: 2,
            cache_type: Some("q8_0".into()),
            flash_attn: Some(true),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        let first = endpoint.transition().await;
        let held = endpoint.admit_when_ready(std::time::Duration::from_secs(5));
        tokio::pin!(held);
        assert!(futures::poll!(&mut held).is_pending(), "held on the closed endpoint");
        let old = first.start_generation().expect("test: first generation");
        first
            .ready("test://grow-chain", &old, contract.clone())
            .expect("test: first engine verified");
        // Reopened, but the first writer is still held: the turn moves from the watch
        // to the gate's read queue.
        assert!(futures::poll!(&mut held).is_pending(), "queued on the gate behind the writer");
        drop(first);
        // The second transition closes the endpoint synchronously and queues for the
        // writer BEHIND the turn's read lease.
        let second = endpoint.transition();
        tokio::pin!(second);
        assert!(futures::poll!(&mut second).is_pending(), "second writer queued behind the read lease");
        assert!(
            futures::poll!(&mut held).is_pending(),
            "closed again with patience left: still held (the first cut refused here)"
        );
        let second = second.await;
        old.observed_exit();
        let new = second.start_generation().expect("test: second generation");
        second
            .ready("test://grow-chain", &new, contract)
            .expect("test: second engine verified");
        drop(second);
        let admitted = held.await.expect("admitted by the transition that finally left the endpoint open");
        assert!(admitted.check_ready().is_ok());
    }

    // what this catches: the 2026-08-26 KV-reuse-0% bug, both halves. One persona
    // running N concurrent activities (a 4-instance benchmark dispatch, each its
    // own room) must lease N DISTINCT slots — under the old persona-keyed lease
    // all four returned slot 0 and thrashed it (cached:0 every turn). And a nil
    // room must be unrepresentable as a key — it previously became the live
    // string key "persona@000…0".
    #[tokio::test]
    async fn one_persona_many_rooms_leases_distinct_slots() {
        let pool = KvSlotPool::new("test", 4);
        // 4 slots = 3 citizen + 1 scratch (B2). Three concurrent activities of
        // ONE persona hold three DISTINCT citizen slots — under the old
        // persona-keyed lease all of them returned slot 0 and thrashed it.
        let mut slots = Vec::new();
        for r in 1..=3u128 {
            slots.push(pool.lease(key(7, r)).await.expect("free slot"));
        }
        slots.sort_unstable();
        assert_eq!(slots, vec![0, 1, 2], "three activities hold three distinct citizen slots");
        // Warm re-entry: the same activity gets ITS slot back.
        assert_eq!(pool.lease(key(7, 1)).await, Some(0), "room 1 kept its warm slot");
        // A 4th activity EVICTS a citizen slot (LRU for now, priced in B5) —
        // it must never spill onto scratch.
        let fourth = pool.lease(key(7, 4)).await.expect("evicts, not refuses");
        assert!(fourth < 3, "the 4th activity takes a citizen slot, never scratch");
        // Nil is unrepresentable.
        assert!(PageKey::fork(Uuid::from_u128(7), Uuid::nil()).is_none());
        assert!(PageKey::fork(Uuid::nil(), Uuid::from_u128(1)).is_none());
        assert!(PageKey::mind(Uuid::nil()).is_none());
    }

    // what this catches: eviction leaking slot indices. When a 5th activity
    // arrives on a full 4-slot pool, the engine evicts the LRU resident and the
    // freed INDEX must return through the lease's Drop — a leak here would
    // shrink the pool one eviction at a time until nobody can lease.
    #[tokio::test]
    async fn eviction_recycles_the_slot_index() {
        let pool = KvSlotPool::new("test", 2);
        let a = pool.lease(key(1, 1)).await.expect("a");
        // last_access_at is ms-granular; real turns are seconds apart, the test
        // must space its touches or LRU order is a coin flip within one ms.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _b = pool.lease(key(1, 2)).await.expect("b");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // Refresh a so b is the LRU.
        let _ = pool.lease(key(1, 1)).await;
        let c = pool.lease(key(1, 3)).await.expect("c evicts LRU and reuses its index");
        assert_ne!(c, a, "c must not steal the warm slot that was just refreshed");
        // And the evicted activity can come back (cold) on whatever frees next.
        let _ = pool.lease(key(1, 2)).await.expect("evicted activity re-leases");
    }

    // what this catches: the paging ledger around slot recycling (the restore-
    // economy context switch). A recycled slot must (1) name the previous
    // holder for SAVE before the new turn overwrites their KV, (2) offer
    // RESTORE only to an activity with a written page whose KV is not already
    // in the slot, (3) never save/restore on a warm same-holder re-lease, and
    // (4) stop offering a page after note_page_lost. Without (1) rotation on
    // an oversubscribed server is a full re-prefill per turn — measured live
    // 2026-09-01 as hit_rate 0.0 across every act, 35-45s prefill each.
    #[tokio::test]
    async fn slot_recycling_pages_the_evictee_out_and_the_returner_in() {
        let pool = KvSlotPool::new("test", 2); // 2 slots, no scratch
        let a = key(1, 1);
        let b = key(1, 2);
        let c = key(1, 3);
        let pa = pool.lease_paged(a).await.expect("a");
        assert_eq!(pa.save_first, None, "fresh slot: nobody to save");
        assert!(!pa.restore, "no page written yet");
        // Warm re-lease: same holder, nothing to page either way.
        let pa2 = pool.lease_paged(a).await.expect("a warm");
        assert_eq!(pa2.slot, pa.slot);
        assert_eq!(pa2.save_first, None, "same holder — no save");
        assert!(!pa2.restore, "slot already holds a's fresher KV — no restore");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _pb = pool.lease_paged(b).await.expect("b");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // c arrives on a full pool: someone (LRU = a) is evicted; the paging
        // ledger must name the previous holder of the recycled slot for save.
        let pc = pool.lease_paged(c).await.expect("c");
        assert_eq!(pc.save_first, Some(a), "the evictee's KV must be saved before overwrite");
        pool.note_saved(a); // adapter reports the save HTTP call succeeded
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        // a returns: her slot was recycled, but she has a page → restore.
        let pa3 = pool.lease_paged(a).await.expect("a returns");
        assert!(pa3.restore, "a has a page and the slot holds someone else's KV");
        // A dead page stops being offered.
        pool.note_page_lost(&a);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let _ = pool.lease_paged(b).await.expect("b again");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let pa4 = pool.lease_paged(a).await.expect("a after lost page");
        assert!(!pa4.restore, "a lost page must not be offered again");
        assert_eq!(
            page_filename(&a),
            format!("a-{}-fork-{}.bin", Uuid::from_u128(1).simple(), Uuid::from_u128(1).simple())
        );
    }

    // what this catches: the scratch reservation and the placement law. On a
    // 4-slot server the highest index is reserved — citizens lease 0..2 and the
    // 4th activity EVICTS rather than spilling onto scratch; on a 2-slot server
    // there is no scratch to spare. And the class map: only the turn purposes
    // may reach a citizen slot; everything unknown defaults to Background so a
    // new producer can never clobber a citizen by omission.
    #[tokio::test]
    async fn scratch_is_reserved_and_only_turns_are_turns() {
        let pool = KvSlotPool::new("test", 4);
        assert_eq!(pool.scratch_slot(), Some(3), "highest index is scratch");
        let mut seen = Vec::new();
        for r in 1..=4u128 {
            seen.push(pool.lease(key(9, r)).await.expect("lease"));
        }
        assert!(!seen.contains(&3), "citizen leases never land on scratch: {seen:?}");
        assert_eq!(KvSlotPool::new("test", 2).scratch_slot(), None, "no scratch on tiny servers");

        assert_eq!(class_for(Some("cognition/deliberation")), SlotClass::Turn);
        assert_eq!(class_for(Some("persona-respond")), SlotClass::Turn);
        assert_eq!(class_for(Some("cognition/should-respond")), SlotClass::Sidecar);
        assert_eq!(class_for(Some("serving-smoke-probe")), SlotClass::Probe);
        assert_eq!(class_for(Some("dream-consolidation")), SlotClass::Background);
        assert_eq!(class_for(Some("some-future-producer")), SlotClass::Background,
            "unknown purposes default AWAY from citizen slots");
        assert_eq!(class_for(None), SlotClass::Background);
    }

    // what this catches: a resident's acts in two rooms keyed as two pages (the per-room key
    // this replaced: every move between rooms paid a restore or a cold prefill for a prefix
    // her epoch view already shares), and a fork keyed onto her page (an eval exam would
    // overwrite her warm view and wait behind her act). Plan line 3b, card 424e0961.
    #[tokio::test]
    async fn one_mind_acting_in_two_rooms_warms_one_page_and_a_fork_never_shares_it() {
        use crate::inference::request_body::{ACT_PURPOSE, DELIBERATION_PURPOSE, FORK_ACT_PURPOSE, FORK_DELIBERATION_PURPOSE};
        let (her, general, academy) = (Uuid::from_u128(31), Uuid::from_u128(32), Uuid::from_u128(33));
        let act = class_for(Some(ACT_PURPOSE));
        let pass = class_for(Some(DELIBERATION_PURPOSE));
        let in_general = act.page_key(Some(her), Some(general)).expect("her act in general has a page");
        assert_eq!(pass.page_key(Some(her), Some(academy)), Some(in_general), "two rooms, one page");
        assert_eq!(act.page_key(Some(her), None), Some(in_general), "a mind's page needs no room");
        assert_eq!(page_filename(&in_general), format!("a-{}.bin", her.simple()));

        let fork = class_for(Some(FORK_ACT_PURPOSE));
        assert_eq!(fork, class_for(Some(FORK_DELIBERATION_PURPOSE)));
        assert!(fork.holds_a_page() && act.holds_a_page() && !class_for(Some("cognition/should-respond")).holds_a_page());
        let exam = fork.page_key(Some(her), Some(general)).expect("a fork in a room has a page");
        assert_ne!(exam, in_general, "a fork never warms her page");
        assert_ne!(fork.page_key(Some(her), Some(academy)), Some(exam), "each fork room is its own page");
        assert_eq!(fork.page_key(Some(her), None), None, "a fork without a room has no page");
        assert_ne!(page_filename(&exam), page_filename(&in_general));

        // One slot holds her across both rooms: the second room's act is a warm re-lease.
        let pool = KvSlotPool::new("test://one-mind", 2);
        let first = pool.lease_paged(in_general).await.expect("general leases");
        let second = pool.lease_paged(act.page_key(Some(her), Some(academy)).expect("academy page")).await.expect("academy leases");
        assert_eq!((second.slot, second.already_resident, second.save_first), (first.slot, true, None));
    }

    // what this catches: KV-CACHE-ECONOMY §2's arithmetic, as a pinned test. A
    // small fresh head must never displace a large warm tail: the shared head is
    // "big enough to win slot selection and small enough that winning throws
    // away 4.7× more than it saves". The priced policy evicts the CHEAPEST
    // tail, recency notwithstanding.
    #[tokio::test]
    async fn a_small_head_never_evicts_a_large_warm_tail() {
        let pool = KvSlotPool::new("test", 2); // no scratch on 2 slots
        let big = key(1, 1);
        let small = key(1, 2);
        pool.lease(big).await.expect("big");
        pool.note_tail(&big, 36_000);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        pool.lease(small).await.expect("small");
        pool.note_tail(&small, 7_600);
        // small is the FRESHER activity — under LRU the big tail would be the
        // victim. Under the priced policy the CHEAP tail goes.
        let incoming = pool.lease(key(1, 3)).await.expect("third leases");
        assert!(
            pool.lease(big).await == Some(0) || incoming != 0,
            "sanity: some slot was assigned"
        );
        // The big activity must still be warm (its slot survives): re-leasing it
        // is a warm hit on the SAME slot, not a re-assignment.
        let big_again = pool.lease(big).await.expect("big still resident");
        assert_eq!(big_again, 0, "the 36k tail survived; the 7.6k head was the victim");
    }

    // what this catches: the directory latch — an unsupported server must never
    // hand out a pool, and the first probe outcome wins for the process life.
    #[tokio::test]
    async fn directory_latch_and_ensure() {
        crate::system_resources::disk_reporters::prepare_writer_fixture().await;
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        assert!(dir.get("s1").is_none(), "unprobed root is unknown");
        dir.latch_unsupported("s1");
        assert!(matches!(dir.get("s1"), Some(None)), "latched unsupported");
        let p = dir.ensure_pool("s2", 4);
        assert_eq!(p.n_slots(), 4);
        assert!(matches!(dir.get("s2"), Some(Some(_))));
        assert!(dir.negotiate_page_admission("s2", &p, None, None).await);
        assert_eq!(dir.page_admission_path("s2").unwrap(), None, "legacy remains unactivated");
        let capability = serde_json::json!(1);
        let remote = dir.ensure_pool("remote", 1);
        assert!(dir.negotiate_page_admission("remote", &remote, None, Some(&capability)).await);
        assert_eq!(dir.page_admission_path("remote").unwrap(), None, "remote needs its owning node");
        let page_dir = tempfile::tempdir().unwrap();
        let local = dir.ensure_pool("local", 1);
        local.attach_ledger(SavedLedger::in_dir(page_dir.path(), "fixture".into()), false);
        assert!(dir.negotiate_page_admission("local", &local, None, Some(&capability)).await);
        assert_eq!(dir.page_admission_path("local").unwrap(), Some(page_dir.path().canonicalize().unwrap()));
        let replacement = Arc::new(KvSlotPool::new("local", 1));
        dir.endpoint("local").state.lock().pool = Some(Some(replacement));
        assert!(!dir.negotiate_page_admission("local", &local, None, Some(&capability)).await,
            "late props cannot authorize a replacement pool");
        let invalid = dir.ensure_pool("invalid", 1);
        assert!(dir.negotiate_page_admission("invalid", &invalid, None, Some(&serde_json::json!("1"))).await);
        assert!(dir.page_admission_path("invalid").is_err(), "invalid capability is not a legacy fallback");
        let missing = dir.ensure_pool("missing", 1);
        missing.attach_ledger(SavedLedger::in_dir(page_dir.path().join("not-created").as_path(), "fixture".into()), false);
        assert!(dir.negotiate_page_admission("missing", &missing, None, Some(&capability)).await);
        assert!(dir.page_admission_path("missing").is_err(), "local path resolution failure never falls back to ungoverned save");
        assert!(!dir.page_admission_known("missing"), "unavailable storage must permit governed rediscovery");
        std::fs::create_dir(page_dir.path().join("not-created")).unwrap();
        assert!(dir.negotiate_page_admission("missing", &missing, None, Some(&capability)).await);
        assert!(dir.page_admission_known("missing"));
        assert_eq!(dir.page_admission_path("missing").unwrap(),
            Some(page_dir.path().join("not-created").canonicalize().unwrap()),
            "the same generation recovers without ungoverned fallback");
        assert!(!dir.negotiate_page_admission("remote", &remote, Some(Uuid::new_v4()), Some(&capability)).await,
            "props for a different generation cannot publish capability");
    }

    // What this catches: same-URL restart must restore saved KV, never trust old
    // physical holders; queued/late work cannot reopen or clear a newer generation.
    #[tokio::test]
    async fn endpoint_restart_drains_admission_and_preserves_only_compatible_pages() {
        let dir = SlotDirectory {
            endpoints: dashmap::DashMap::new(),
        };
        let endpoint = dir.endpoint("test://generation");
        let contract = KvPageContract {
            model_id: "fixture".into(),
            model: "fixture.gguf".into(),
            adapters: vec!["fixture.lora".into()],
            page_dir: Some("fixture-pages".into()),
            context: 32768,
            slots: 2,
            cache_type: Some("q8_0".into()),
            flash_attn: Some(true),
            revisions: Some(vec![(
                "fixture.gguf".into(),
                73,
                std::time::SystemTime::UNIX_EPOCH,
            )]),
        };
        let first = endpoint.transition().await;
        let old = first.start_generation().expect("first engine");
        first
            .ready("test://generation", &old, contract.clone())
            .expect("ready");
        drop(first);
        assert!(endpoint.transition_if(&|| false).await.is_err());
        assert!(endpoint
            .transition_for_generation_if(&old, &|| false)
            .await
            .is_err());
        assert!(
            endpoint.is_ready(),
            "refused lifecycle leaves live readiness untouched"
        );
        let admitted = endpoint.admit().await.expect("old request");
        let old_pool = admitted.pool.as_ref().expect("pool").clone();
        assert!(dir.negotiate_page_admission("test://generation", &old_pool, Some(old.id), None).await);
        assert!(dir.page_admission_known("test://generation"));
        let activity = key(73, 74);
        old_pool
            .lease_paged(activity)
            .await
            .expect("old physical holder");
        old_pool.note_saved(activity);
        let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let authority = || current.load(Ordering::Acquire);
        {
            let queued = endpoint.transition_if(&authority);
            tokio::pin!(queued);
            assert!(futures::poll!(&mut queued).is_pending());
            current.store(false, Ordering::Release);
            drop(admitted);
            assert!(
                queued.await.is_err(),
                "intent changed while existing readers drained"
            );
        }
        assert!(endpoint.is_ready());
        // Rejected checkpoint recovery resumes the exact live ledger, never
        // constructs a new pool or resurrects invalidated saved-page eligibility.
        let suspended = endpoint.transition_for_generation(&old).await.unwrap();
        let receipt = suspended.capture_suspension(&old).unwrap();
        old_pool.note_page_lost(&activity);
        assert!(suspended.resume_suspension(&receipt, &|| false).is_err());
        assert!(!endpoint.is_ready());
        suspended.resume_suspension(&receipt, &|| true).unwrap();
        drop(suspended);
        let resumed = endpoint.admit().await.unwrap();
        assert!(Arc::ptr_eq(resumed.pool.as_ref().unwrap(), &old_pool));
        assert!(
            old_pool
                .lease_paged(activity)
                .await
                .unwrap()
                .already_resident
        );
        assert!(!old_pool.saved.lock().contains(&activity));
        old_pool.note_saved(activity); // Preserve the restart portion's fixture input.
        drop(resumed);
        let admitted = endpoint
            .admit()
            .await
            .expect("refused operation preserves admission");

        // A cancelled retirement waits for readers without closing the live engine.
        // The queued writer also prevents a later reader from overtaking the drain.
        {
            let retirement = endpoint.transition_for_generation(&old);
            tokio::pin!(retirement);
            assert!(futures::poll!(&mut retirement).is_pending());
            let later = endpoint.admit();
            tokio::pin!(later);
            assert!(futures::poll!(&mut later).is_pending());
        }
        assert!(
            endpoint.is_ready(),
            "cancelled pre-retirement drain changes no state"
        );

        let transition = endpoint.transition();
        tokio::pin!(transition);
        assert!(
            futures::poll!(&mut transition).is_pending(),
            "active admission must drain"
        );
        assert!(
            endpoint.admit().await.is_err(),
            "new admission is closed immediately"
        );
        drop(admitted);
        let transition = transition.await;
        assert!(
            transition.start_generation().is_err(),
            "Rust guard release is not engine exit"
        );
        old.observed_exit();
        let new = transition.start_generation().expect("predecessor exited");
        transition
            .ready("test://generation", &new, contract.clone())
            .expect("replacement ready");
        assert!(!dir.page_admission_known("test://generation"), "replacement renegotiates instead of inheriting capability");
        // Refreshing readiness for this generation must preserve its current ledger.
        let new_pool = dir
            .get("test://generation")
            .flatten()
            .expect("replacement pool");
        let pg = new_pool
            .lease_paged(activity)
            .await
            .expect("returning activity");
        assert!(pg.restore, "compatible saved page survives restart");
        assert_eq!(pg.save_first, None, "dead physical KV must never be saved");
        transition
            .ready("test://generation", &new, contract.clone())
            .expect("readiness refresh");
        old.observed_exit();
        drop(transition);
        assert!(endpoint.transition_for_generation(&old).await.is_err());
        assert!(
            endpoint.is_ready(),
            "stale retirement cannot suspend replacement"
        );
        let admitted = endpoint
            .admit()
            .await
            .expect("late old exit cannot close new generation");
        assert!(Arc::ptr_eq(
            admitted.pool.as_ref().expect("pool"),
            &new_pool
        ));
        assert!(!new_pool.lease_paged(activity).await.expect("warm").restore);
        drop(admitted);

        let transition = endpoint.transition().await;
        new.observed_exit();
        let changed = transition.start_generation().expect("new geometry engine");
        let mut incompatible = contract;
        // Another window and slot count is the same page FORMAT: her page stays restorable,
        // and the engine refuses it, loudly, only if it no longer fits the slot (the M5,
        // 2026-10-23: a 129,024 -> 2 x 130,048 relaunch must not turn every resident cold).
        incompatible.context += 256;
        incompatible.slots = 1;
        transition
            .ready("test://generation", &changed, incompatible.clone())
            .expect("changed ready");
        drop(transition);
        let admitted = endpoint.admit().await.expect("changed admission");
        let pool = admitted.pool.as_ref().expect("changed pool");
        assert_eq!(pool.n_slots(), 1);
        assert!(pool.lease_paged(activity).await.expect("relaunched").restore, "another window keeps her page");
        pool.note_saved(activity);
        drop(admitted);
        let transition = endpoint.transition().await;
        changed.observed_exit();
        let different_model = transition.start_generation().expect("different model");
        incompatible.model_id = "other-model".into();
        transition
            .ready("test://generation", &different_model, incompatible)
            .expect("different model ready");
        drop(transition);
        let admitted = endpoint.admit().await.expect("different model admission");
        assert!(
            !admitted
                .pool
                .as_ref()
                .expect("different pool")
                .lease_paged(activity)
                .await
                .expect("cold model")
                .restore
        );
        drop(admitted);
        drop(endpoint.transition().await); // Failed/cancelled bring-up has no ready receipt.
        assert!(
            endpoint.admit().await.is_err(),
            "transition cancellation must stay closed"
        );
    }
    // what this catches: the page store growing without bound (it is temporary disk),
    // and the trim removing the page just saved or a page's sidecar surviving its page.
    #[test]
    fn the_page_store_trims_oldest_first_and_never_the_page_just_saved() {
        let dir = std::env::temp_dir().join(format!("kv-page-trim-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap(); // JUSTIFIED unwrap: test scaffolding
        let write = |name: &str, bytes: usize, age_s: u64| {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; bytes]).unwrap(); // JUSTIFIED unwrap: test scaffolding
            let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age_s);
            let file = std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .expect("fixture timestamp requires a writable handle on Windows");
            file.set_modified(t).expect("set fixture page age");
            let observed = file
                .metadata()
                .expect("fixture page metadata")
                .modified()
                .expect("fixture mtime");
            let error = observed.duration_since(t).unwrap_or_else(|e| e.duration());
            assert!(
                error <= std::time::Duration::from_secs(2),
                "fixture must establish page age within filesystem granularity; ages differ by 100 seconds"
            );
        };
        write("a-old.bin", 100, 300);
        write("a-old.bin.ckpt", 10, 300);
        write("a-old.bin.lora", 5, 300); // the adapter-vector sidecar (card e5eaf9f4): part of the page
        write("a-mid.bin", 100, 200);
        write("a-new.bin", 100, 100);
        write("unrelated.txt", 500, 0);
        let (removed, freed, kept) = trim_page_store(&dir, "a-old.bin", 250, 0);
        assert_eq!(removed, 1, "one page over budget: the oldest NOT protected → a-mid");
        assert_eq!(freed, 100);
        assert_eq!(kept, 215, "a page counts with both its sidecars");
        assert!(dir.join("a-old.bin").exists() && dir.join("a-old.bin.ckpt").exists(), "the just-saved page is kept");
        assert!(!dir.join("a-mid.bin").exists());
        let (removed, _, _) = trim_page_store(&dir, "a-new.bin", 150, 0);
        let (young, _, _) = trim_page_store(&dir, "a-new.bin", 0, 60_000);
        assert_eq!(young, 0, "a page younger than the floor is an active resident's: never trimmed");
        assert_eq!(removed, 1, "now a-old goes, sidecar with it");
        assert!(!dir.join("a-old.bin.ckpt").exists());
        assert!(!dir.join("a-old.bin.lora").exists(), "every sidecar goes with its page: no orphan drains forever");
        assert!(dir.join("unrelated.txt").exists(), "only pages are trimmed");
        let _ = std::fs::remove_dir_all(&dir);
    }

// what this catches (card f3ba3b56, item 4): a Turn that names no activity must never
// run unpinned with its cache on a pool that has a scratch slot, where the server would
// place it by similarity over a resident. It lands where any unattributed request does.
#[test]
fn only_a_keyed_turn_owns_a_slot() {
    let at = |slot, cache_prompt| Placement { slot, cache_prompt };
    assert_eq!(placement(true, Some(1), Some(3)), at(Some(1), true), "her own slot");
    assert_eq!(placement(false, None, Some(3)), at(Some(3), true), "keyless: scratch");
    assert_eq!(placement(false, Some(0), None), at(Some(0), false), "borrowed, never persisted");
    assert_eq!(placement(false, None, None), at(None, false), "no slot: no cache");
}
