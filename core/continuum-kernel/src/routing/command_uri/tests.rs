#![cfg(test)]
    use super::*;

    // Helper for round-trip assertions.
    fn round_trip(input: &str, expected: CommandUri) {
        let parsed =
            CommandUri::parse(input).unwrap_or_else(|e| panic!("parse({input:?}) failed: {e}"));
        assert_eq!(parsed, expected, "parsed mismatch for {input:?}");
        let printed = parsed.to_string();
        let reparsed = CommandUri::parse(&printed)
            .unwrap_or_else(|e| panic!("re-parse of {printed:?} failed: {e}"));
        assert_eq!(reparsed, parsed, "round-trip mismatch for {input:?}");
    }

    // Bare path
    #[test]
    fn bare_path_parses_to_local() {
        round_trip(
            "inference/llm/generate",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn bare_path_with_query() {
        round_trip(
            "inference/llm/generate?model=qwen2.5-0.5b",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: Some("model=qwen2.5-0.5b".into()),
                fragment: None,
            },
        );
    }

    #[test]
    fn bare_path_with_fragment() {
        round_trip(
            "turn/current#chunk:42",
            CommandUri::Local {
                path: "turn/current".into(),
                query: None,
                fragment: Some("chunk:42".into()),
            },
        );
    }

    // Explicit local
    #[test]
    fn explicit_local_parses_to_local() {
        round_trip(
            "airc:///inference/llm/generate",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Peer by name
    #[test]
    fn peer_by_name() {
        round_trip(
            "airc://maya/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_by_uuid() {
        let uuid = Uuid::parse_str("18c04c5b-e059-4129-816f-75e8e58fd74c").unwrap();
        round_trip(
            "airc://18c04c5b-e059-4129-816f-75e8e58fd74c/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Uuid(uuid),
                node: None,
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_node() {
        round_trip(
            "airc://maya@5090-rig/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_env() {
        round_trip(
            "airc://maya:web/widget/show",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: Some(EnvironmentId::Named("web".into())),
                path: "widget/show".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_node_and_env() {
        round_trip(
            "airc://maya@5090-rig:vr/scene/spawn",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                env: Some(EnvironmentId::Named("vr".into())),
                path: "scene/spawn".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Broadcast (wildcard env on a peer)
    #[test]
    fn peer_wildcard_env_is_broadcast() {
        round_trip(
            "airc://maya:*/notification/post",
            CommandUri::Broadcast {
                peer: PeerRef::Name("maya".into()),
                node: None,
                path: "notification/post".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn broadcast_with_node() {
        round_trip(
            "airc://maya@5090-rig:*/notification/post",
            CommandUri::Broadcast {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                path: "notification/post".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Room
    #[test]
    fn room_by_uuid() {
        let uuid = Uuid::parse_str("cb2e21a1-999a-5a03-a184-df06e4ee7097").unwrap();
        round_trip(
            "airc://room:cb2e21a1-999a-5a03-a184-df06e4ee7097/render/start",
            CommandUri::Room {
                room_id: uuid,
                env: None,
                path: "render/start".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn room_with_env_filter() {
        let uuid = Uuid::parse_str("cb2e21a1-999a-5a03-a184-df06e4ee7097").unwrap();
        round_trip(
            "airc://room:cb2e21a1-999a-5a03-a184-df06e4ee7097:web/render/start",
            CommandUri::Room {
                room_id: uuid,
                env: Some(EnvironmentId::Named("web".into())),
                path: "render/start".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Persona-internal address space (substrate paths under a peer)
    #[test]
    fn persona_internal_address_space() {
        round_trip(
            "airc://maya/cognition/genome/lora:typescript-expertise",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "cognition/genome/lora:typescript-expertise".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn debug_namespace_routing() {
        round_trip(
            "airc://maya/debug/probes/latency/stream",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "debug/probes/latency/stream".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Query + fragment on every variant
    #[test]
    fn peer_with_query_and_fragment() {
        round_trip(
            "airc://maya/debug/profile/flamegraph?window=5m&format=svg#frame:42",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "debug/profile/flamegraph".into(),
                query: Some("window=5m&format=svg".into()),
                fragment: Some("frame:42".into()),
            },
        );
    }

    // Error cases
    #[test]
    fn unknown_scheme_errors() {
        assert!(matches!(
            CommandUri::parse("https://example.com/foo"),
            Err(UriParseError::UnknownScheme(_))
        ));
    }

    #[test]
    fn empty_input_errors() {
        assert!(matches!(
            CommandUri::parse(""),
            Err(UriParseError::EmptyPath)
        ));
    }

    #[test]
    fn empty_path_after_authority_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya/"),
            Err(UriParseError::EmptyPath)
        ));
        assert!(matches!(
            CommandUri::parse("airc://maya"),
            Err(UriParseError::EmptyPath)
        ));
    }

    #[test]
    fn invalid_room_uuid_errors() {
        assert!(matches!(
            CommandUri::parse("airc://room:not-a-uuid/render/start"),
            Err(UriParseError::InvalidRoomUuid(_, _))
        ));
    }

    #[test]
    fn empty_room_uuid_errors() {
        assert!(matches!(
            CommandUri::parse("airc://room:/render/start"),
            Err(UriParseError::EmptyRoomUuid(_))
        ));
    }

    #[test]
    fn empty_node_id_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya@/inference/llm/generate"),
            Err(UriParseError::EmptyNodeId(_))
        ));
    }

    #[test]
    fn empty_env_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya:/inference/llm/generate"),
            Err(UriParseError::EmptyEnv(_))
        ));
    }

    #[test]
    fn from_str_delegates_to_parse() {
        let uri: CommandUri = "airc://maya/inference/llm/generate".parse().unwrap();
        assert!(matches!(uri, CommandUri::Peer { .. }));
    }

    /// The migration shim: `From<&str>` accepts the conventional bare
    /// path AND fully-qualified URIs, falling through to Local on
    /// parse failure (downstream handler-not-found is the typed
    /// error). This is what lets every existing `execute("path", ...)`
    /// call site keep compiling unchanged after `execute()` takes
    /// `impl Into<CommandUri>`.
    #[test]
    fn from_str_shim_accepts_bare_path() {
        let uri: CommandUri = "inference/llm/generate".into();
        assert!(matches!(uri, CommandUri::Local { .. }));
        assert_eq!(uri.path(), "inference/llm/generate");
    }

    #[test]
    fn from_str_shim_accepts_full_uri() {
        let uri: CommandUri = "airc://maya/inference/llm/generate".into();
        assert!(matches!(uri, CommandUri::Peer { .. }));
        assert_eq!(uri.path(), "inference/llm/generate");
    }

    #[test]
    fn from_str_shim_falls_back_to_local_on_parse_error() {
        // A malformed URI (unknown scheme) doesn't surface a parse error
        // at the boundary — it converts to Local so the dispatcher's
        // handler-not-found path produces the typed error instead.
        let uri: CommandUri = "https://example.com/foo".into();
        assert!(matches!(uri, CommandUri::Local { .. }));
    }

    #[test]
    fn from_string_shim_works_for_owned_strings() {
        let owned = String::from("ai/generate");
        let uri: CommandUri = owned.into();
        assert!(matches!(uri, CommandUri::Local { .. }));
        assert_eq!(uri.path(), "ai/generate");
    }

    #[test]
    fn local_constructor_is_infallible_and_round_trips() {
        let uri = CommandUri::local("inference/llm/generate");
        assert!(uri.is_local());
        assert_eq!(uri.path(), "inference/llm/generate");
        assert_eq!(uri.to_string(), "inference/llm/generate");
    }

    #[test]
    fn is_local_polarity() {
        assert!(CommandUri::local("foo").is_local());
        assert!(!CommandUri::Peer {
            peer: PeerRef::Name("maya".into()),
            node: None,
            env: None,
            path: "foo".into(),
            query: None,
            fragment: None,
        }
        .is_local());
        assert!(!CommandUri::Broadcast {
            peer: PeerRef::Name("maya".into()),
            node: None,
            path: "foo".into(),
            query: None,
            fragment: None,
        }
        .is_local());
        assert!(!CommandUri::Room {
            room_id: Uuid::nil(),
            env: None,
            path: "foo".into(),
            query: None,
            fragment: None,
        }
        .is_local());
    }

    // Name-vs-UUID detection
    #[test]
    fn name_with_hyphens_does_not_false_positive_as_uuid() {
        // Five segments but wrong lengths → name, not UUID.
        let r = CommandUri::parse("airc://maya-the-helper-of-operator/foo")
            .expect("hyphenated name should parse as Name");
        match r {
            CommandUri::Peer {
                peer: PeerRef::Name(n),
                ..
            } => assert_eq!(n, "maya-the-helper-of-operator"),
            other => panic!("expected Peer{{Name}}, got {other:?}"),
        }
    }

    #[test]
    fn malformed_uuid_at_segment_lengths_does_error() {
        // Looks like a UUID (right segment lengths) but contains
        // non-hex chars → InvalidPeerUuid.
        let bad = "zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz";
        assert!(matches!(
            CommandUri::parse(&format!("airc://{bad}/foo")),
            Err(UriParseError::InvalidPeerUuid(_, _))
        ));
    }

    // looks_like_uuid contract
    #[test]
    fn looks_like_uuid_recognizes_canonical_form() {
        assert!(looks_like_uuid("18c04c5b-e059-4129-816f-75e8e58fd74c"));
    }

    #[test]
    fn looks_like_uuid_rejects_names() {
        assert!(!looks_like_uuid("maya"));
        assert!(!looks_like_uuid("maya-the-helper"));
        assert!(!looks_like_uuid("18c04c5b"));
    }
