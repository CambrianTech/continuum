#![cfg(test)]
    use super::*;
    use continuum_base::audio_constants::{AUDIO_FRAME_SIZE, AUDIO_SAMPLE_RATE};
    use crate::live::audio::mixer::test_utils::*;
    use crate::live::audio::pcm::base64_encode_i16;

    /// A valid airc RoomId (uuid) to key test calls. Post-#193 the call room IS the airc
    /// room, so `join_call` accepts only an airc RoomId — a bare name is refused. Tests dial
    /// with the RoomId, exactly as a real client would after resolving the name through airc.
    const TEST_ROOM: &str = "11111111-1111-1111-1111-111111111111";

    // what this catches (#193): the call room IS the airc room BY CONSTRUCTION. The gate
    // refuses a bare name (there is no parallel call_id namespace to fall back to — the
    // nodejs rogue-id cancer), and every spelling of an airc RoomId collapses to ONE
    // canonical key, so two peers dialing the same room meet in the same call.
    #[test]
    fn require_airc_room_refuses_names_and_canonicalizes_uuid_spellings() {
        assert!(
            CallManager::require_airc_room("general").is_err(),
            "a name is not an airc room id"
        );
        assert!(
            CallManager::require_airc_room("persona-call").is_err(),
            "no rogue call_id namespace"
        );
        assert!(
            CallManager::require_airc_room("").is_err(),
            "empty is refused"
        );
        let dashed =
            CallManager::require_airc_room("22222222-2222-2222-2222-222222222222").unwrap();
        let simple = CallManager::require_airc_room("22222222222222222222222222222222").unwrap();
        assert_eq!(
            dashed, simple,
            "dashed + 32-char spellings of one RoomId ⇒ one canonical key"
        );
    }

    #[test]
    fn test_base64_roundtrip() {
        let samples = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        let encoded = base64_encode_i16(&samples);
        let decoded = base64_decode_i16(&encoded).unwrap();
        assert_eq!(samples, decoded);
    }

    #[tokio::test]
    async fn test_call_manager_join_leave() {
        let manager = CallManager::new();

        // Join a call (false = not AI)
        let join = manager
            .join_call(TEST_ROOM, "user-1", "Alice", false)
            .await
            .unwrap();

        // Check stats
        let stats = manager.get_stats(&join.handle).await;
        assert!(stats.is_some());
        let (count, _) = stats.unwrap();
        assert_eq!(count, 1);

        // Leave call
        manager.leave_call(&join.handle).await;

        // Stats should be gone
        let stats = manager.get_stats(&join.handle).await;
        assert!(stats.is_none());
    }

    #[tokio::test]
    async fn test_call_manager_multi_participant() {
        let manager = CallManager::new();

        // Two participants join (humans)
        let join_a = manager
            .join_call(TEST_ROOM, "user-a", "Alice", false)
            .await
            .unwrap();
        let join_b = manager
            .join_call(TEST_ROOM, "user-b", "Bob", false)
            .await
            .unwrap();

        // Check count
        let stats = manager.get_stats(&join_a.handle).await;
        assert_eq!(stats.unwrap().0, 2);

        // Push audio from Alice (buffered, mixed by audio loop)
        let audio = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        manager.push_audio(&join_a.handle, audio).await;

        // Give audio loop time to tick
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check samples processed (audio loop should have ticked)
        let stats = manager.get_stats(&join_a.handle).await;
        assert!(stats.unwrap().1 > 0);

        // Leave
        manager.leave_call(&join_a.handle).await;
        manager.leave_call(&join_b.handle).await;
    }

    #[tokio::test]
    async fn test_mute() {
        let manager = CallManager::new();

        let join = manager
            .join_call(TEST_ROOM, "user-1", "Alice", false)
            .await
            .unwrap();

        // Mute
        manager.set_mute(&join.handle, true).await;

        // Unmute
        manager.set_mute(&join.handle, false).await;

        manager.leave_call(&join.handle).await;
    }

    #[tokio::test]
    async fn test_video_push_broadcast() {
        let manager = CallManager::new();

        // Two participants join
        let join_a = manager
            .join_call(TEST_ROOM, "user-a", "Alice", false)
            .await
            .unwrap();
        let mut join_b = manager
            .join_call(TEST_ROOM, "user-b", "Bob", false)
            .await
            .unwrap();

        // Alice sends a video frame
        let fake_frame = vec![0x00; 20]; // 16 byte header + 4 byte payload
        manager.push_video(&join_a.handle, fake_frame.clone()).await;

        // Bob should receive Alice's video (mix-minus: not your own)
        // Give broadcast time to propagate
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Try receiving — might be empty if broadcast hasn't delivered yet
        match join_b.video_rx.try_recv() {
            Ok((sender, user_id, data)) => {
                assert_eq!(sender, join_a.handle);
                assert_eq!(user_id, "user-a");
                assert_eq!(data, fake_frame);
            }
            Err(_) => {
                // Broadcast delivery is async, this is acceptable in tests
            }
        }

        manager.leave_call(&join_a.handle).await;
        manager.leave_call(&join_b.handle).await;
    }

    // what this catches (#192): the media substrate is right if TWO AI PERSONAS can hold a
    // call — SEE + HEAR + SPEAK each other — with NO UI, purely through the server-side call
    // machinery. Proves per-sender SFU delivery + mix-minus in BOTH directions, and that a
    // persona never keeps its OWN echoed frame. Unlike `test_video_push_broadcast` (which
    // tolerated non-delivery — "Err is acceptable"), this AWAITS delivery and FAILS if the
    // substrate drops it: the headless proof that no browser is needed for two beings to be
    // present to each other in a call.
    #[tokio::test(flavor = "multi_thread")]
    async fn two_ai_personas_see_hear_and_speak_headless() {
        use tokio::time::{timeout, Duration};

        let manager = CallManager::new();
        // Two PERSONAS (is_ai = true) join the same call — no browser, no UI.
        let asha = manager
            .join_call(TEST_ROOM, "@persona:asha", "Asha", true)
            .await
            .unwrap();
        let mut anwen = manager
            .join_call(TEST_ROOM, "@persona:anwen", "Anwen", true)
            .await
            .unwrap();
        // Snapshot Asha's video stream from the start so we catch every frame (incl. her own
        // echo, which mix-minus must let us skip).
        let mut asha_video = asha.video_rx.resubscribe();

        // SEE+SPEAK, direction 1: Asha shows video → Anwen must SEE Asha's exact frame.
        let asha_frame = {
            let mut f = vec![0u8; 20]; // 16-byte header + payload (opaque to the broadcast)
            f[16] = 0xA1;
            f
        };
        manager.push_video(&asha.handle, asha_frame.clone()).await;
        let seen = timeout(Duration::from_secs(2), async {
            loop {
                let (sender, uid, data) = anwen.video_rx.recv().await.expect("video open");
                if sender == asha.handle {
                    return (uid, data);
                }
            }
        })
        .await
        .expect("Anwen must SEE Asha — the substrate dropped the video");
        assert_eq!(seen.0, "@persona:asha");
        assert_eq!(seen.1, asha_frame, "Anwen sees Asha's exact frame");

        // SEE+SPEAK, direction 2: Anwen shows video → Asha must SEE Anwen's exact frame,
        // skipping Asha's OWN echoed frame first (mix-minus at the consumer).
        let anwen_frame = {
            let mut f = vec![0u8; 20];
            f[16] = 0xB2;
            f
        };
        manager.push_video(&anwen.handle, anwen_frame.clone()).await;
        let seen = timeout(Duration::from_secs(2), async {
            loop {
                let (sender, uid, data) = asha_video.recv().await.expect("video open");
                if sender == asha.handle {
                    continue; // mix-minus: never keep your own echo
                }
                return (uid, data);
            }
        })
        .await
        .expect("Asha must SEE Anwen — the substrate dropped the video");
        assert_eq!(seen.0, "@persona:anwen");
        assert_eq!(seen.1, anwen_frame, "Asha sees Anwen's exact frame");

        // HEAR: Asha speaks → Anwen must HEAR a frame that ORIGINATES from Asha (per-sender
        // SFU + mix-minus loop). Non-silent sine so VAD passes.
        let audio = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        manager.push_audio(&asha.handle, audio).await;
        let heard = timeout(Duration::from_secs(3), async {
            loop {
                match anwen.audio_rx.recv().await {
                    Ok((sender, uid, _)) if sender == asha.handle => return uid,
                    Ok(_) => continue, // hold-music / own echo — skip
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => panic!("audio channel closed before Anwen heard Asha"),
                }
            }
        })
        .await
        .expect("Anwen must HEAR Asha — the audio substrate dropped it");
        assert_eq!(heard, "@persona:asha");

        manager.leave_call(&asha.handle).await;
        manager.leave_call(&anwen.handle).await;
    }
