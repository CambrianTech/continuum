    use super::*;
    use async_trait::async_trait;
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    struct StubDescriber;
    #[async_trait]
    impl FrameDescriber for StubDescriber {
        async fn describe(&self, source: &[u8], mime: &str) -> Result<String, String> {
            Ok(format!("{mime} image, {} bytes", source.len()))
        }
    }

    const AMBIENT: DestSize = DestSize {
        width: 32,
        height: 24,
    };

    // what this catches: COALESCE — a newer frame for the same participant REPLACES the old
    // (room-as-now, no backlog). Two observes of the same participant leave ONE percept,
    // carrying the LATEST content hash.
    #[tokio::test]
    async fn observe_coalesces_to_the_latest_frame_per_participant() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);

        let old = MediaFrame::from_bytes(png(40, 40));
        let new = MediaFrame::from_bytes(png(60, 40)); // different bytes → different hash
        buffer.observe(
            "alice".into(),
            old.clone(),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );
        buffer.observe(
            "alice".into(),
            new.clone(),
            compute.clone(),
            describer.clone(),
            "image/png",
            1,
        );
        buffer.observe(
            "bob".into(),
            MediaFrame::from_bytes(png(20, 20)),
            compute.clone(),
            describer.clone(),
            "image/png",
            2,
        );

        assert_eq!(
            buffer.len(),
            2,
            "alice coalesced, bob separate → 2 participants"
        );
        let percepts = buffer.current_percepts(&compute);
        let alice = percepts.iter().find(|p| p.participant == "alice").unwrap();
        assert_eq!(
            alice.content_hash,
            new.content_hash(),
            "alice holds the LATEST frame"
        );
    }

    // what this catches: NON-BLOCKING read semantics — before a cell is warmed the percept
    // carries None (perception takes what's ready, doesn't wait); once the same content-hash
    // cell is resolved on the shared compute, current_percepts surfaces it as Some. Proves the
    // buffer reads via the _if_ready twins, not a blocking recompute.
    #[tokio::test]
    async fn current_percepts_surface_cells_only_once_resolved() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let frame = MediaFrame::from_bytes(png(50, 40));

        // Push onto the ring WITHOUT warming (bypass observe's spawn): read the percept from
        // the head before any cell resolves → all None.
        buffer
            .rings
            .lock()
            .unwrap()
            .entry("alice".into())
            .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY))
            .push(frame.clone());
        let before = &buffer.current_percepts(&compute)[0];
        assert!(
            before.thumbnail.is_none() && before.description.is_none(),
            "cold → nothing rendered"
        );
        assert!(!before.has_any());

        // Resolve the cells on the SHARED compute (deterministic; in prod the observe spawn
        // does this async). Now the non-blocking read surfaces them.
        frame.scaled(&compute, None, AMBIENT).await;
        frame
            .description(&compute, &StubDescriber, "image/png")
            .await;
        let after = &buffer.current_percepts(&compute)[0];
        assert!(after.thumbnail.is_some(), "thumbnail now ready");
        assert!(after.description.is_some(), "description now ready");
        assert_eq!(
            image::load_from_memory(after.thumbnail.as_ref().unwrap().as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            AMBIENT.width,
            "the ready thumbnail is the ambient size"
        );
    }

    // what this catches: the ring is a bounded sliding window — newest-first, an identical
    // re-send of the head coalesces (no dup), and past capacity the oldest is evicted. This is
    // the "pre-warmed sliding-window cache" the grouped + windowed reads sit on.
    #[test]
    fn frame_ring_windows_newest_first_coalesces_head_and_bounds_capacity() {
        let mut ring = FrameRing::with_capacity(3);
        assert!(ring.head().is_none(), "empty ring has no head");

        let a = MediaFrame::from_bytes(png(11, 11));
        let b = MediaFrame::from_bytes(png(12, 12));
        let c = MediaFrame::from_bytes(png(13, 13));
        let d = MediaFrame::from_bytes(png(14, 14));

        ring.push(a.clone());
        ring.push(a.clone()); // identical head → coalesced no-op
        assert_eq!(
            ring.window(9).count(),
            1,
            "re-send of the same head coalesces"
        );

        ring.push(b.clone());
        ring.push(c.clone());
        ring.push(d.clone()); // capacity 3 → 'a' evicted

        let hashes: Vec<_> = ring
            .window(9)
            .map(|f| f.content_hash().to_string())
            .collect();
        assert_eq!(
            hashes,
            vec![
                d.content_hash().to_string(),
                c.content_hash().to_string(),
                b.content_hash().to_string()
            ],
            "newest-first, capacity-bounded, oldest ('a') evicted"
        );
        assert_eq!(
            ring.head().unwrap().content_hash(),
            d.content_hash(),
            "head = most current"
        );
        assert_eq!(ring.window(2).count(), 2, "window respects k");
    }

    // what this catches: the two serving shapes off the ONE shared store — GROUPED (all ring
    // heads = the human-widget gallery, everyone at a glance) and WINDOWED (one source's recent
    // history, newest-first). An unknown participant windows to empty.
    #[tokio::test]
    async fn grouped_read_is_all_heads_windowed_read_is_one_sources_history() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());

        // alice gets a short history (3 distinct frames), bob one — pushed WITHOUT warming so
        // the read exercises structure (heads/window), not cell resolution.
        {
            let mut rings = buffer.rings.lock().unwrap();
            let alice = rings
                .entry("alice".into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY));
            for (w, h) in [(11, 11), (12, 12), (13, 13)] {
                alice.push(MediaFrame::from_bytes(png(w, h)));
            }
            rings
                .entry("bob".into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY))
                .push(MediaFrame::from_bytes(png(20, 20)));
        }

        // GROUPED: one head per source — the gallery of everyone at a glance.
        let grouped = buffer.current_percepts(&compute);
        assert_eq!(grouped.len(), 2, "gallery = one head per participant");

        // WINDOWED: the last k of ONE source, newest-first.
        let win = buffer.window_percepts("alice", 2, &compute);
        assert_eq!(win.len(), 2, "windowed read = last k of one source");
        assert!(
            win.iter().all(|p| p.participant == "alice"),
            "windowed read is source-scoped"
        );

        // Unknown participant → empty (never a fabricated look).
        assert!(buffer.window_percepts("nobody", 5, &compute).is_empty());
    }

    // what this catches: the à la carte PULL — a persona requests images of a SOURCE or the
    // GROUP, at THUMBNAIL / higher-RES / FULL, satisfied ASAP. Higher-res is a distinct cache
    // key on the same content-hash; a repeat pull is compute-once/shared (same Arc); FULL is
    // the raw bytes; an unknown source pulls empty.
    #[tokio::test]
    async fn look_serves_source_or_group_at_requested_fidelity_asap_and_shared() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);

        // Two sources present (via the ingest path — the head is what a look reads).
        buffer.observe(
            "alice".into(),
            MediaFrame::from_bytes(png(120, 90)),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );
        buffer.observe(
            "bob".into(),
            MediaFrame::from_bytes(png(64, 64)),
            compute.clone(),
            describer.clone(),
            "image/png",
            0,
        );

        // SOURCE + THUMBNAIL: one image, at the ambient size, satisfied ASAP (awaited).
        let a = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Thumbnail,
                &compute,
            )
            .await;
        assert_eq!(a.len(), 1, "source scope → just that source");
        assert_eq!(a[0].participant, "alice");
        let bytes = a[0].image.as_ref().as_ref().expect("thumbnail resolved");
        assert_eq!(
            image::load_from_memory(bytes).unwrap().width(),
            AMBIENT.width,
            "ambient thumbnail size"
        );

        // PREFER-WARM / compute-once: a second identical pull returns the SAME shared Arc.
        let a2 = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Thumbnail,
                &compute,
            )
            .await;
        assert!(
            Arc::ptr_eq(&a[0].image, &a2[0].image),
            "repeat pull is compute-once/shared, not recomputed"
        );

        // Higher-RES: a distinct size → a distinct derivative (bigger than the thumbnail).
        let hi = DestSize {
            width: 96,
            height: 72,
        };
        let r = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Res(hi),
                &compute,
            )
            .await;
        assert_eq!(
            image::load_from_memory(r[0].image.as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            hi.width,
            "higher-res honored"
        );
        assert!(
            !Arc::ptr_eq(&a[0].image, &r[0].image),
            "different fidelity → different cell"
        );

        // FULL: the raw source bytes (original 120×90).
        let f = buffer
            .look(
                LookScope::Source("alice".into()),
                LookFidelity::Full,
                &compute,
            )
            .await;
        assert_eq!(
            image::load_from_memory(f[0].image.as_ref().as_ref().unwrap())
                .unwrap()
                .width(),
            120,
            "full = raw frame"
        );

        // GROUP (Everyone) + THUMBNAIL: the contact-sheet — one image per source.
        let group = buffer
            .look(LookScope::Everyone, LookFidelity::Thumbnail, &compute)
            .await;
        assert_eq!(
            group.len(),
            2,
            "group scope → every source's current frame (the gallery)"
        );

        // Unknown source → empty (never a fabricated look).
        assert!(buffer
            .look(
                LookScope::Source("nobody".into()),
                LookFidelity::Thumbnail,
                &compute
            )
            .await
            .is_empty());
    }

    // what this catches: the change MONITOR — `scene_recently_changed` diffs the two most
    // recent WARMED luma signatures. Same scene (same pattern, different frame) → NOT changed
    // (decays to the floor); a visually different frame → changed (escalates). Unknown source
    // or unwarmed signatures → true (assume changed, ride the ceiling until we can tell).
    #[tokio::test]
    async fn scene_recently_changed_reads_warmed_signatures() {
        fn solid_png(w: u32, h: u32) -> Vec<u8> {
            let img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
            let mut out = Cursor::new(Vec::new());
            DynamicImage::ImageRgba8(img)
                .write_to(&mut out, ImageFormat::Png)
                .unwrap();
            out.into_inner()
        }

        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let src = "alice";

        // Unknown source → assume changed.
        assert!(buffer.scene_recently_changed(src, &compute));

        // Two visually-IDENTICAL frames (same red|blue pattern, different dims → different
        // content hash, so both are kept in the ring). Warm their signatures.
        let f1 = MediaFrame::from_bytes(png(80, 60));
        let f2 = MediaFrame::from_bytes(png(120, 90));
        f1.signature(&compute).await;
        f2.signature(&compute).await;
        {
            let mut rings = buffer.rings.lock().unwrap();
            let r = rings
                .entry(src.into())
                .or_insert_with(|| FrameRing::with_capacity(AMBIENT_RING_CAPACITY));
            r.push(f1.clone());
            r.push(f2.clone());
        }
        assert!(
            !buffer.scene_recently_changed(src, &compute),
            "same scene across frames → not changed → decays to the slow floor"
        );

        // A visually DIFFERENT frame on top → changed.
        let solid = MediaFrame::from_bytes(solid_png(50, 50));
        solid.signature(&compute).await;
        buffer
            .rings
            .lock()
            .unwrap()
            .get_mut(src)
            .unwrap()
            .push(solid.clone());
        assert!(
            buffer.scene_recently_changed(src, &compute),
            "a different scene → changed → escalate to the ceiling"
        );
    }

    // what this catches: remove drops a participant who left the call.
    #[tokio::test]
    async fn remove_drops_a_participant() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let compute = Arc::new(SharedCompute::new());
        let describer: Arc<dyn FrameDescriber> = Arc::new(StubDescriber);
        buffer.observe(
            "alice".into(),
            MediaFrame::from_bytes(png(10, 10)),
            compute,
            describer,
            "image/png",
            0,
        );
        assert_eq!(buffer.len(), 1);
        buffer.remove("alice");
        assert!(buffer.is_empty());
    }

    // what this catches: the 30fps SAFETY guarantee, as a pure decision (no timing/spawn).
    // The first frame warms; further frames WITHIN the interval are gated even though they
    // are distinct content (the frame rate outruns the perception sample rate); once the
    // interval elapses the next warm is due again. This is what keeps a 30fps stream from
    // triggering 30 LLM describes/sec.
    #[test]
    fn warm_cadence_gates_within_the_interval_and_reopens_after() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "alice";

        // First ever frame → warms immediately; holds the in-flight flag.
        let flag = buffer.should_warm(p, 0, true).expect("first frame warms");
        // Simulate the warm completing so the in-flight guard is not what's under test.
        flag.store(false, Ordering::Release);

        // A CHANGED frame 10ms later (30fps ≈ 33ms apart) → GATED by the MIN ceiling.
        assert!(
            buffer.should_warm(p, 10, true).is_none(),
            "a changed frame within the min ceiling is gated — perception samples, not mirrors, 30fps"
        );
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS - 1, true)
                .is_none(),
            "still within the min ceiling → still gated"
        );

        // Once the min ceiling elapses, a changed frame warms again.
        let flag2 = buffer
            .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS, true)
            .expect("min ceiling elapsed + changed → warm is due");
        flag2.store(false, Ordering::Release);
    }

    // what this catches: the BAND — "slow cadence, not NO cadence". Absent a change signal a
    // participant is NOT starved: it decays to the slow BASELINE floor (still warms every few
    // seconds), while a relevant change escalates it to the fast MIN ceiling.
    #[test]
    fn warm_cadence_is_a_band_change_escalates_static_decays_to_floor() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "carol";

        // First look happens regardless.
        buffer
            .should_warm(p, 0, false)
            .expect("first look")
            .store(false, Ordering::Release);

        // STATIC (changed=false): gated until the SLOW baseline floor — but NOT forever.
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_MIN_INTERVAL_MS, false)
                .is_none(),
            "static past the min ceiling is still gated — no change, don't spend a describe"
        );
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS - 1, false)
                .is_none(),
            "static within the baseline floor → still gated"
        );
        buffer
            .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS, false)
            .expect("static past the baseline floor → STILL warms (slow cadence, not none)")
            .store(false, Ordering::Release);

        // CHANGE escalates: a relevant change warms as soon as the MIN ceiling allows,
        // long before the baseline floor.
        let t = AMBIENT_WARM_BASELINE_INTERVAL_MS + AMBIENT_WARM_MIN_INTERVAL_MS;
        buffer
            .should_warm(p, t, true)
            .expect("change past the min ceiling → escalates, no need to wait for the floor")
            .store(false, Ordering::Release);
    }

    // what this catches: never STACK a warm — while one is in flight, later observes are
    // gated even if the interval has elapsed, so a describe slower than the interval under
    // a fast stream cannot pile up concurrent inferences.
    #[test]
    fn warm_gate_never_stacks_an_in_flight_warm() {
        let buffer = PerceptionBuffer::new(AMBIENT);
        let p = "bob";

        // Launch a warm and DON'T release it (simulates a describe still running).
        let _held = buffer.should_warm(p, 0, true).expect("first warms");

        // Even far past the interval, a second warm is refused while one is in flight.
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS * 100, true)
                .is_none(),
            "in-flight guard blocks stacking regardless of elapsed time"
        );

        // Once it completes, the next due tick warms again.
        _held.store(false, Ordering::Release);
        assert!(
            buffer
                .should_warm(p, AMBIENT_WARM_BASELINE_INTERVAL_MS * 100, true)
                .is_some(),
            "warm reopens after the in-flight one finishes"
        );
    }
