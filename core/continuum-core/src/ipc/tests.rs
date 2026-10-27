#![cfg(test)]
    // what this catches (card 4fb5895c): the socket path never built a CommandUri, so
    // `airc://<peer>/ping` was refused as "Unknown command" while #3690's remote transport
    // sat installed on the executor. A peer- or room-addressed string MUST leave the
    // name-based route (never dispatch locally under a remote address — the one outcome
    // the #3690 probe pre-registered as forbidden); a bare name and the explicit-local
    // form MUST stay on it, byte-for-byte, so nothing that works today changes route.
    #[test]
    fn a_peer_or_room_address_leaves_the_name_route_and_a_bare_name_stays() {
        use crate::routing::CommandUri;
        let peer = "airc://2f0aed7f-3359-4dd5-9f9b-aaa0b07c6266/ping";
        assert!(
            matches!(
                super::addressed_elsewhere(peer),
                Some(CommandUri::Peer { .. })
            ),
            "a peer-addressed verb goes to the executor's remote transport"
        );
        assert!(
            matches!(
                super::addressed_elsewhere("airc://room:3be59578-7f1d-5d78-8b17-1eb0834b4643/ping"),
                Some(CommandUri::Room { .. })
            ),
            "a room-addressed verb goes to the executor"
        );
        // Bare names and the explicit-local spelling are the local route, unchanged.
        for local in ["ping", "genome/job-status", "airc:///ping", "commands/list"] {
            assert!(
                super::addressed_elsewhere(local).is_none(),
                "{local} stays name-routed"
            );
        }
        // Garbage is not an address: it stays on the name route and gets the registry's
        // own "Unknown command … did you mean" sentence rather than a URI parse error.
        assert!(super::addressed_elsewhere("airc://").is_none());
    }

    use super::*;

    #[test]
    fn prepare_unix_socket_path_creates_parent_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir
            .path()
            .join("missing")
            .join("sockets")
            .join("continuum-core.sock");

        prepare_unix_socket_path(socket_path.to_str().unwrap()).unwrap();

        assert!(socket_path.parent().unwrap().is_dir());
    }

    #[test]
    fn prepare_unix_socket_path_removes_stale_socket_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir.path().join("continuum-core.sock");
        std::fs::write(&socket_path, b"stale").unwrap();

        prepare_unix_socket_path(socket_path.to_str().unwrap()).unwrap();

        assert!(!socket_path.exists());
    }

    // ========================================================================
    // Binary Framing Unit Tests
    // ========================================================================

    #[test]
    fn test_json_frame_roundtrip() {
        // Create a response, write to buffer, verify framing
        let response = Response::success(serde_json::json!({"healthy": true}));
        let json = serde_json::to_string(&response).unwrap();
        let payload = json.as_bytes();

        // Build frame: [4-byte BE length][payload]
        let length = payload.len() as u32;
        let mut frame = Vec::new();
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(payload);

        // Parse frame
        assert!(frame.len() >= 4);
        let parsed_length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(parsed_length, payload.len());

        let parsed_json: serde_json::Value =
            serde_json::from_slice(&frame[4..4 + parsed_length]).unwrap();
        assert_eq!(parsed_json["success"], true);
        assert_eq!(parsed_json["result"]["healthy"], true);
    }

    #[test]
    fn test_binary_frame_roundtrip() {
        // Simulate binary response: JSON header + \0 + raw PCM
        let response = Response::success(serde_json::json!({
            "sample_rate": 16000,
            "duration_ms": 500,
            "binary_pcm": true
        }));
        let json = serde_json::to_string(&response).unwrap();
        let json_bytes = json.as_bytes();

        // Simulate PCM audio data (4 samples of i16)
        let audio_samples: Vec<i16> = vec![1000, -2000, 3000, -4000];
        let pcm_bytes: Vec<u8> = audio_samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        // Build binary frame: [4-byte BE total_length][JSON][\0][PCM]
        let total_length = (json_bytes.len() + 1 + pcm_bytes.len()) as u32;
        let mut frame = Vec::new();
        frame.extend_from_slice(&total_length.to_be_bytes());
        frame.extend_from_slice(json_bytes);
        frame.push(0u8); // separator
        frame.extend_from_slice(&pcm_bytes);

        // Parse frame
        let parsed_total = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        let payload = &frame[4..4 + parsed_total];

        // Find \0 separator
        let sep_idx = payload
            .iter()
            .position(|&b| b == 0)
            .expect("Should have separator");
        let parsed_json_bytes = &payload[..sep_idx];
        let parsed_binary = &payload[sep_idx + 1..];

        // Verify JSON header
        let parsed: serde_json::Value = serde_json::from_slice(parsed_json_bytes).unwrap();
        assert_eq!(parsed["result"]["sample_rate"], 16000);
        assert_eq!(parsed["result"]["binary_pcm"], true);

        // Verify binary PCM data
        assert_eq!(parsed_binary.len(), pcm_bytes.len());
        let parsed_samples: Vec<i16> = parsed_binary
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(parsed_samples, audio_samples);
    }

    #[test]
    fn test_binary_frame_separator_unambiguous() {
        // Verify that serde_json never produces a raw 0x00 byte
        // (it encodes null chars as \u0000, which is 6 ASCII bytes)
        let json_with_null = serde_json::json!({"text": "before\0after"});
        let serialized = serde_json::to_string(&json_with_null).unwrap();
        let bytes = serialized.as_bytes();

        // Should NOT contain raw 0x00 byte
        assert!(
            !bytes.contains(&0u8),
            "serde_json should never emit raw 0x00 byte, got: {:?}",
            serialized
        );
        // Should contain the escaped form
        assert!(
            serialized.contains("\\u0000"),
            "Null should be escaped as \\u0000"
        );
    }

    // ========================================================================
    // Response Serialization Tests
    // ========================================================================
    // NOTE: Request deserialization tests removed - legacy Request enum deleted.
    // Commands now route through ServiceModule implementations (modules/*.rs).
    // Each module has its own tests for command handling.

    #[test]
    fn test_response_success_serialization() {
        let response = Response::success(serde_json::json!({"key": "value"}));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["result"]["key"], "value");
        assert!(parsed.get("error").is_none() || parsed["error"].is_null());
    }

    #[test]
    fn test_response_error_serialization() {
        let response = Response::error("something broke".to_string());
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["error"], "something broke");
    }

    #[test]
    fn test_response_with_request_id() {
        let response = Response::success(serde_json::json!({})).with_request_id(Some(42));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["requestId"], 42);
    }

    // ========================================================================
    // Integration Test: Full IPC Round-Trip via Unix Socket
    // Requires: continuum-core-server running (cargo test --ignored)
    // ========================================================================

    #[test]
    #[ignore] // Requires running continuum-core server
    #[cfg(unix)] // dials the unix IPC socket; even #[ignore] must compile (#304)
    fn test_ipc_health_check_live() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;

        let socket_path = "/tmp/continuum-core.sock";
        let mut stream =
            UnixStream::connect(socket_path).expect("Failed to connect to continuum-core socket");

        // Send health-check request
        let request = r#"{"command":"health-check","requestId":1}"#;
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();

        // Read length-prefixed response
        let mut len_buf = [0u8; 4];
        std::io::Read::read_exact(&mut stream, &mut len_buf).unwrap();
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut payload = vec![0u8; length];
        std::io::Read::read_exact(&mut stream, &mut payload).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(response["success"], true);
        assert_eq!(response["result"]["healthy"], true);
        assert_eq!(response["requestId"], 1);

        println!("IPC health-check response: {}", response);
    }

    #[test]
    #[ignore] // Requires running continuum-core server with Kokoro model
    #[cfg(unix)] // dials the unix IPC socket; even #[ignore] must compile (#304)
    fn test_ipc_voice_synthesize_binary_live() {
        use std::io::Write;

        let socket_path = "/tmp/continuum-core.sock";
        let mut stream = std::os::unix::net::UnixStream::connect(socket_path)
            .expect("Failed to connect to continuum-core socket");

        // Send voice/synthesize request
        let request =
            r#"{"command":"voice/synthesize","text":"Hello world","voice":"af","requestId":2}"#;
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();

        // Read length-prefixed response (may be binary)
        let mut len_buf = [0u8; 4];
        std::io::Read::read_exact(&mut stream, &mut len_buf).unwrap();
        let length = u32::from_be_bytes(len_buf) as usize;
        assert!(length > 0, "Response should not be empty");

        let mut payload = vec![0u8; length];
        std::io::Read::read_exact(&mut stream, &mut payload).unwrap();

        // Find \0 separator for binary frame
        let sep_idx = payload.iter().position(|&b| b == 0);

        if let Some(idx) = sep_idx {
            // Binary response: JSON header + \0 + raw PCM
            let json_bytes = &payload[..idx];
            let pcm_bytes = &payload[idx + 1..];

            let header: serde_json::Value = serde_json::from_slice(json_bytes).unwrap();
            assert_eq!(header["success"], true);
            assert_eq!(header["result"]["binary_pcm"], true);

            let sample_rate = header["result"]["sample_rate"].as_u64().unwrap();
            let num_samples = header["result"]["num_samples"].as_u64().unwrap();
            let duration_ms = header["result"]["duration_ms"].as_u64().unwrap();

            assert_eq!(sample_rate, 16000);
            assert!(num_samples > 100, "Should have >100 samples");
            assert!(duration_ms > 50, "Should be >50ms");
            assert_eq!(
                pcm_bytes.len(),
                num_samples as usize * 2,
                "PCM bytes should be 2 * num_samples"
            );

            // Verify PCM data is valid i16 audio (not all zeros)
            let samples: Vec<i16> = pcm_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            let max_amp = samples.iter().map(|s| s.abs()).max().unwrap_or(0);
            assert!(
                max_amp > 100,
                "Audio should not be silence, max amplitude: {}",
                max_amp
            );

            println!(
                "IPC voice/synthesize: {} samples, {}Hz, {}ms, {} bytes PCM, max amp: {}",
                num_samples,
                sample_rate,
                duration_ms,
                pcm_bytes.len(),
                max_amp
            );
        } else {
            // JSON-only response (likely an error)
            let response: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            panic!("Expected binary response, got JSON: {}", response);
        }
    }
