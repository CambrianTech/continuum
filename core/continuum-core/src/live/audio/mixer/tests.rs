#![cfg(test)]
    use super::*;
    use continuum_base::audio_constants::{AUDIO_FRAME_SIZE, AUDIO_SAMPLE_RATE};
    use crate::live::audio::pcm::is_silence;
    use test_utils::*;

    #[test]
    fn test_generate_sine_wave() {
        let samples = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        assert_eq!(samples.len(), AUDIO_FRAME_SIZE);

        // Should not be silence
        assert!(!is_silence(&samples, 100.0));

        // Frequency should be approximately 440Hz
        let detected = detect_frequency_approx(&samples, AUDIO_SAMPLE_RATE);
        assert!((detected - 440.0).abs() < 50.0, "Detected: {detected}");
    }

    #[test]
    fn test_generate_silence() {
        let samples = generate_silence(AUDIO_FRAME_SIZE);
        assert_eq!(samples.len(), AUDIO_FRAME_SIZE);
        assert!(is_silence(&samples, 1.0));
    }

    #[tokio::test]
    async fn test_mixer_add_remove() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        stream_a.initialize_vad().expect("VAD init failed");

        mixer.add_participant(stream_a);
        assert_eq!(mixer.participant_count(), 1);

        mixer.remove_participant(&handle_a);
        assert_eq!(mixer.participant_count(), 0);
    }

    #[tokio::test]
    async fn test_mix_all() {
        let mut mixer = AudioMixer::default_voice();

        // Add two participants with different tones
        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        // Alice plays 440Hz, Bob plays 880Hz
        stream_a.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_b.push_audio(generate_sine_wave(
            880.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Mix should contain both frequencies (not silence)
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), AUDIO_FRAME_SIZE);
        assert!(!is_silence(&mixed, 100.0));
    }

    #[tokio::test]
    async fn test_mix_minus() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();
        let handle_c = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());
        let mut stream_c = ParticipantStream::new(handle_c, "user-c".into(), "Charlie".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");
        stream_c.initialize_vad().expect("VAD init failed");

        // Each plays a different frequency
        stream_a.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_b.push_audio(generate_sine_wave(
            880.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_c.push_audio(generate_sine_wave(
            1320.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);
        mixer.add_participant(stream_c);

        // Alice should hear Bob + Charlie (not herself)
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert!(!is_silence(&mix_for_a, 100.0));

        // Bob should hear Alice + Charlie (not himself)
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert!(!is_silence(&mix_for_b, 100.0));

        // Mix-minus for each should be different from mix_all
        let mix_all = mixer.mix_all();
        assert_ne!(mix_for_a, mix_all);
        assert_ne!(mix_for_b, mix_all);
    }

    #[tokio::test]
    async fn test_mix_minus_two_participants() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        let audio_a = generate_sine_wave(440.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);
        let audio_b = generate_sine_wave(880.0, AUDIO_SAMPLE_RATE, AUDIO_FRAME_SIZE);

        stream_a.push_audio(audio_a.clone());
        stream_b.push_audio(audio_b.clone());

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Alice hears only Bob
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert_eq!(mix_for_a, audio_b, "Alice should hear exactly Bob's audio");

        // Bob hears only Alice
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert_eq!(mix_for_b, audio_a, "Bob should hear exactly Alice's audio");
    }

    #[tokio::test]
    async fn test_muted_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        stream_a.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_b.push_audio(generate_sine_wave(
            880.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_a.muted = true; // Alice is muted

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        // Mix-minus for Bob should be silence (Alice is muted)
        let mix_for_b = mixer.mix_minus(&handle_b);
        assert!(
            is_silence(&mix_for_b, 10.0),
            "Bob should hear silence (Alice muted)"
        );

        // Mix-minus for Alice should be Bob's audio
        let mix_for_a = mixer.mix_minus(&handle_a);
        assert!(!is_silence(&mix_for_a, 100.0), "Alice should hear Bob");
    }

    #[tokio::test]
    async fn test_ai_participant() {
        let mut mixer = AudioMixer::default_voice();

        let handle_human = Handle::new();
        let handle_ai = Handle::new();

        let mut stream_human =
            ParticipantStream::new(handle_human, "user-human".into(), "test-user".into());
        let stream_ai =
            ParticipantStream::new_ai(handle_ai, "ai-helper".into(), "Helper AI".into());

        stream_human.initialize_vad().expect("VAD init failed");
        // AI doesn't need VAD initialization

        assert!(!stream_human.is_ai);
        assert!(stream_ai.is_ai);

        // Human speaks (3 frames - mix_all() + mix_minus() x2 each consume from human)
        stream_human.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));

        // AI injects TTS audio (3 frames - each mixing call consumes one frame from ring buffer)
        // mix_all(), mix_minus(&human), mix_minus(&ai) each pull one frame
        let mut stream_ai_mut = stream_ai;
        stream_ai_mut.push_audio(generate_sine_wave(
            220.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE * 3,
        ));

        mixer.add_participant(stream_human);
        mixer.add_participant(stream_ai_mut);

        // Both should be in the mix
        let mix_all = mixer.mix_all();
        assert!(!is_silence(&mix_all, 100.0));

        // Human hears AI
        let mix_for_human = mixer.mix_minus(&handle_human);
        assert!(!is_silence(&mix_for_human, 100.0));

        // AI "hears" human (for STT purposes)
        let mix_for_ai = mixer.mix_minus(&handle_ai);
        assert!(!is_silence(&mix_for_ai, 100.0));
    }

    #[tokio::test]
    async fn test_mix_minus_all() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");

        stream_a.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_b.push_audio(generate_sine_wave(
            880.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);

        let all_mixes = mixer.mix_minus_all();
        assert_eq!(all_mixes.len(), 2);
        assert!(all_mixes.contains_key(&handle_a));
        assert!(all_mixes.contains_key(&handle_b));
    }

    #[tokio::test]
    async fn test_pull_all_audio() {
        let mut mixer = AudioMixer::default_voice();

        let handle_a = Handle::new();
        let handle_b = Handle::new();
        let handle_c = Handle::new();

        let mut stream_a = ParticipantStream::new(handle_a, "user-a".into(), "Alice".into());
        let mut stream_b = ParticipantStream::new(handle_b, "user-b".into(), "Bob".into());
        let stream_c = ParticipantStream::new(handle_c, "user-c".into(), "Charlie".into());

        stream_a.initialize_vad().expect("VAD init failed");
        stream_b.initialize_vad().expect("VAD init failed");
        // Charlie doesn't push audio — should not appear in result

        stream_a.push_audio(generate_sine_wave(
            440.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));
        stream_b.push_audio(generate_sine_wave(
            880.0,
            AUDIO_SAMPLE_RATE,
            AUDIO_FRAME_SIZE,
        ));

        mixer.add_participant(stream_a);
        mixer.add_participant(stream_b);
        mixer.add_participant(stream_c);

        let frames = mixer.pull_all_audio();

        // Should have exactly 2 frames (Alice and Bob have audio, Charlie is silent)
        assert_eq!(
            frames.len(),
            2,
            "Expected 2 active senders, got {}",
            frames.len()
        );

        // Check user_ids are correct
        let user_ids: Vec<&str> = frames.iter().map(|(_, uid, _)| uid.as_str()).collect();
        assert!(user_ids.contains(&"user-a"), "Should contain user-a");
        assert!(user_ids.contains(&"user-b"), "Should contain user-b");

        // Check audio is not empty
        for (_, user_id, audio) in &frames {
            assert!(
                !audio.is_empty(),
                "Audio for {} should not be empty",
                user_id
            );
            assert!(
                !is_silence(audio, 100.0),
                "Audio for {} should not be silence",
                user_id
            );
        }
    }

    #[tokio::test]
    async fn test_clipping_prevention() {
        let mut mixer = AudioMixer::default_voice();

        // Add many loud participants
        for i in 0..10 {
            let handle = Handle::new();
            let mut stream =
                ParticipantStream::new(handle, format!("user-{i}"), format!("User {i}"));
            stream.initialize_vad().expect("VAD init failed");
            // Max amplitude sine wave
            stream.push_audio(generate_sine_wave(
                440.0 + (i as f32 * 100.0),
                AUDIO_SAMPLE_RATE,
                AUDIO_FRAME_SIZE,
            ));
            mixer.add_participant(stream);
        }

        // Mix should not overflow - if we get here without panic, clamping worked
        let mixed = mixer.mix_all();
        assert_eq!(mixed.len(), AUDIO_FRAME_SIZE);
        // Values are already i16 so they're in valid range by type constraints
        // The real test is that clamp_to_i16 prevents overflow during mixing
    }
