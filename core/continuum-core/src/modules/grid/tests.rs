//! Grid integration tests.
//!
//! These tests spin up real TCP listeners and verify:
//! - Frame roundtrip (send request → receive → send response → receive)
//! - TailscaleTransport connect/accept/send/recv
//! - ReticulumTransport identity persistence
//! - NodeRegistry persistence and merge logic
//! - GridRouter routing decisions with live registry
//! - AuditLog write and read

#[cfg(test)]
mod tailscale_transport_integration {
    use crate::modules::grid::frame::{FrameType, GridFrame, GridPayload};
    use crate::modules::grid::node::TransportAddress;
    use crate::modules::grid::transport::GridTransport;
    use crate::modules::grid::transports::tailscale::TailscaleTransport;
    use std::sync::Arc;

    /// Find a free port by binding to port 0.
    async fn free_port() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn test_tcp_connect_and_send_frame() {
        let port = free_port().await;
        let server = Arc::new(TailscaleTransport::new(port));
        server.start_with_ip("127.0.0.1").await.unwrap();

        // Spawn server accept loop
        let server_clone = server.clone();
        let server_task = tokio::spawn(async move {
            let conn = server_clone.accept().await.unwrap();
            // Receive the request
            let frame = conn.recv_frame().await.unwrap();
            assert_eq!(frame.frame_type, FrameType::Request);
            // Send response
            let response = GridFrame::success_response(&frame, serde_json::json!({"pong": true}));
            conn.send_frame(&response).await.unwrap();
            frame
        });

        // Client connects and sends
        let client = TailscaleTransport::new(0); // Port 0 = no listener needed
        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port,
            machine_name: None,
        };

        let conn = client.connect(&addr).await.unwrap();
        assert!(conn.is_connected());

        let request = GridFrame::command_request(
            "test-001".into(),
            "client".into(),
            "server".into(),
            "health-check".into(),
            serde_json::json!({}),
        );
        conn.send_frame(&request).await.unwrap();

        // Receive response
        let response = conn.recv_frame().await.unwrap();
        assert_eq!(response.frame_type, FrameType::Response);
        assert_eq!(response.correlation_id, "test-001");
        if let GridPayload::CommandResult {
            success, result, ..
        } = &response.payload
        {
            assert!(success);
            assert_eq!(result.as_ref().unwrap()["pong"], true);
        } else {
            panic!("Expected CommandResult payload");
        }

        conn.close().await.unwrap();

        // Verify server received the right frame
        let received = server_task.await.unwrap();
        assert_eq!(received.correlation_id, "test-001");
        if let GridPayload::Command { command, .. } = &received.payload {
            assert_eq!(command, "health-check");
        } else {
            panic!("Expected Command payload");
        }
    }

    #[tokio::test]
    async fn test_multiple_frames_on_same_connection() {
        let port = free_port().await;
        let server = Arc::new(TailscaleTransport::new(port));
        server.start_with_ip("127.0.0.1").await.unwrap();

        let server_clone = server.clone();
        let server_task = tokio::spawn(async move {
            let conn = server_clone.accept().await.unwrap();
            let mut received = Vec::new();
            for _ in 0..5 {
                let frame = conn.recv_frame().await.unwrap();
                let response = GridFrame::success_response(
                    &frame,
                    serde_json::json!({"n": frame.correlation_id}),
                );
                conn.send_frame(&response).await.unwrap();
                received.push(frame.correlation_id.clone());
            }
            received
        });

        let client = TailscaleTransport::new(0);
        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port,
            machine_name: None,
        };
        let conn = client.connect(&addr).await.unwrap();

        // Send 5 frames on the same connection
        for i in 0..5 {
            let request = GridFrame::command_request(
                format!("multi-{i}"),
                "client".into(),
                "server".into(),
                "gpu/stats".into(),
                serde_json::json!({}),
            );
            conn.send_frame(&request).await.unwrap();
            let response = conn.recv_frame().await.unwrap();
            assert_eq!(response.correlation_id, format!("multi-{i}"));
        }

        let received_ids = server_task.await.unwrap();
        assert_eq!(
            received_ids,
            vec!["multi-0", "multi-1", "multi-2", "multi-3", "multi-4"]
        );
    }

    #[tokio::test]
    async fn test_large_frame_payload() {
        let port = free_port().await;
        let server = Arc::new(TailscaleTransport::new(port));
        server.start_with_ip("127.0.0.1").await.unwrap();

        let server_clone = server.clone();
        let server_task = tokio::spawn(async move {
            let conn = server_clone.accept().await.unwrap();
            let frame = conn.recv_frame().await.unwrap();
            let response = GridFrame::success_response(&frame, serde_json::json!({"ok": true}));
            conn.send_frame(&response).await.unwrap();
            frame
        });

        let client = TailscaleTransport::new(0);
        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port,
            machine_name: None,
        };
        let conn = client.connect(&addr).await.unwrap();

        // Send a frame with a large payload (~1MB of JSON)
        let large_data: Vec<String> = (0..10_000)
            .map(|i| format!("item-{i}-padding-data-here"))
            .collect();
        let request = GridFrame::command_request(
            "large-001".into(),
            "client".into(),
            "server".into(),
            "dataset/import".into(),
            serde_json::json!({"data": large_data}),
        );
        conn.send_frame(&request).await.unwrap();
        let response = conn.recv_frame().await.unwrap();
        assert_eq!(response.correlation_id, "large-001");

        let received = server_task.await.unwrap();
        if let GridPayload::Command { params, .. } = &received.payload {
            let arr = params["data"].as_array().unwrap();
            assert_eq!(arr.len(), 10_000);
        } else {
            panic!("Expected Command payload");
        }
    }

    #[tokio::test]
    async fn test_event_frame_roundtrip() {
        let port = free_port().await;
        let server = Arc::new(TailscaleTransport::new(port));
        server.start_with_ip("127.0.0.1").await.unwrap();

        let server_clone = server.clone();
        let server_task = tokio::spawn(async move {
            let conn = server_clone.accept().await.unwrap();
            conn.recv_frame().await.unwrap()
        });

        let client = TailscaleTransport::new(0);
        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port,
            machine_name: None,
        };
        let conn = client.connect(&addr).await.unwrap();

        // Send an event frame (not a request)
        let event = GridFrame::event(
            "train-001".into(),
            "5090-tower".into(),
            "laptop".into(),
            "genome:train:progress".into(),
            serde_json::json!({"epoch": 2, "loss": 0.043, "accuracy": 0.845}),
        );
        conn.send_frame(&event).await.unwrap();

        let received = server_task.await.unwrap();
        assert_eq!(received.frame_type, FrameType::Event);
        if let GridPayload::Event { event, data } = &received.payload {
            assert_eq!(event, "genome:train:progress");
            assert_eq!(data["epoch"], 2);
            assert_eq!(data["loss"], 0.043);
        } else {
            panic!("Expected Event payload");
        }
    }

    #[tokio::test]
    async fn test_connect_to_wrong_address_type_fails() {
        let client = TailscaleTransport::new(0);
        let reticulum_addr = TransportAddress::Reticulum {
            destination_hash: "abcd1234".into(),
        };

        let result = client.connect(&reticulum_addr).await;
        assert!(result.is_err());
        assert!(result.is_err());
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Expected error"),
        };
        assert!(err.contains("wrong transport type"), "Error was: {err}");
    }

    #[tokio::test]
    async fn test_connect_to_unreachable_host_fails() {
        let client = TailscaleTransport::new(0);
        // Port 1 is almost certainly not listening
        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port: 1,
            machine_name: None,
        };

        let result = client.connect(&addr).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_clients_to_same_server() {
        let port = free_port().await;
        let server = Arc::new(TailscaleTransport::new(port));
        server.start_with_ip("127.0.0.1").await.unwrap();

        // Spawn server that accepts 3 connections
        let server_clone = server.clone();
        let server_task = tokio::spawn(async move {
            let mut results = Vec::new();
            for _ in 0..3 {
                let conn = server_clone.accept().await.unwrap();
                let frame = conn.recv_frame().await.unwrap();
                let response = GridFrame::success_response(&frame, serde_json::json!({"ok": true}));
                conn.send_frame(&response).await.unwrap();
                results.push(frame.correlation_id.clone());
            }
            results
        });

        let addr = TransportAddress::Tailscale {
            ip: "127.0.0.1".into(),
            port,
            machine_name: None,
        };

        // 3 separate clients connect
        for i in 0..3 {
            let client = TailscaleTransport::new(0);
            let conn = client.connect(&addr).await.unwrap();
            let request = GridFrame::command_request(
                format!("client-{i}"),
                "client".into(),
                "server".into(),
                "health-check".into(),
                serde_json::json!({}),
            );
            conn.send_frame(&request).await.unwrap();
            let response = conn.recv_frame().await.unwrap();
            assert_eq!(response.correlation_id, format!("client-{i}"));
            conn.close().await.unwrap();
        }

        let results = server_task.await.unwrap();
        assert_eq!(results.len(), 3);
    }
}

#[cfg(test)]
mod reticulum_transport_integration {
    use crate::modules::grid::node::TransportAddress;
    use crate::modules::grid::transport::GridTransport;
    use crate::modules::grid::transports::reticulum::ReticulumTransport;

    #[tokio::test]
    async fn test_identity_persists_across_restarts() {
        let dir = std::env::temp_dir().join("grid-test-ret-persist");
        let _ = std::fs::remove_dir_all(&dir);

        // First start — generates identity
        let transport1 = ReticulumTransport::new(dir.clone());
        transport1.start().await.unwrap();
        let addr1 = transport1.local_address().unwrap();
        let hash1 = match &addr1 {
            TransportAddress::Reticulum { destination_hash } => destination_hash.clone(),
            _ => panic!("Expected Reticulum address"),
        };
        transport1.shutdown().await.unwrap();

        // Second start — should load same identity
        let transport2 = ReticulumTransport::new(dir.clone());
        transport2.start().await.unwrap();
        let addr2 = transport2.local_address().unwrap();
        let hash2 = match &addr2 {
            TransportAddress::Reticulum { destination_hash } => destination_hash.clone(),
            _ => panic!("Expected Reticulum address"),
        };
        transport2.shutdown().await.unwrap();

        assert_eq!(hash1, hash2, "Identity should persist across restarts");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_discover_returns_empty() {
        let dir = std::env::temp_dir().join("grid-test-ret-discover");
        let transport = ReticulumTransport::new(dir.clone());
        transport.start().await.unwrap();

        let nodes = transport.discover().await.unwrap();
        assert!(nodes.is_empty(), "No peers to discover yet");

        transport.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_connect_returns_not_ready_error() {
        let dir = std::env::temp_dir().join("grid-test-ret-connect");
        let transport = ReticulumTransport::new(dir.clone());
        transport.start().await.unwrap();

        let result = transport
            .connect(&TransportAddress::Reticulum {
                destination_hash: "abcdef01".into(),
            })
            .await;

        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Expected error"),
        };
        assert!(err.contains("not yet implemented"), "Error was: {err}");

        transport.shutdown().await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_provides_encryption() {
        let dir = std::env::temp_dir().join("grid-test-ret-enc");
        let transport = ReticulumTransport::new(dir.clone());
        assert!(transport.provides_encryption());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod registry_integration {
    use crate::modules::grid::node::*;
    use crate::modules::grid::registry::NodeRegistry;

    #[test]
    fn test_persist_and_reload() {
        let dir = std::env::temp_dir().join("grid-test-reg-persist");
        let _ = std::fs::remove_dir_all(&dir);

        // Create and populate registry
        {
            let registry = NodeRegistry::new(&dir);
            registry.register_node(GridNode {
                node_id: "100.1.2.3".into(),
                node_name: Some("bigmama".into()),
                addresses: vec![TransportAddress::Tailscale {
                    ip: "100.1.2.3".into(),
                    port: 7117,
                    machine_name: Some("bigmama".into()),
                }],
                capabilities: vec![
                    NodeCapability::Compute {
                        gpu: Some("RTX 5090".into()),
                        vram_mb: Some(32768),
                    },
                    NodeCapability::Training {
                        max_rank: 64,
                        max_epochs: 100,
                    },
                ],
                trust_level: TrustLevel::Owner,
                last_seen: 1000000,
                latency_ms: Some(47),
                peer_id: None,
            });
            registry.save_to_disk().unwrap();
        }

        // Reload from disk
        {
            let registry = NodeRegistry::new(&dir);
            let node = registry.get("100.1.2.3").unwrap();
            assert_eq!(node.node_name, Some("bigmama".into()));
            assert_eq!(node.trust_level, TrustLevel::Owner);
            assert_eq!(node.capabilities.len(), 2);
            assert_eq!(node.latency_ms, Some(47));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_capability_search() {
        let dir = std::env::temp_dir().join("grid-test-reg-cap");
        let registry = NodeRegistry::new(&dir);

        // GPU node
        registry.register_node(GridNode {
            node_id: "gpu-node".into(),
            node_name: None,
            addresses: vec![],
            capabilities: vec![NodeCapability::Compute {
                gpu: Some("RTX 5090".into()),
                vram_mb: Some(32768),
            }],
            trust_level: TrustLevel::Trusted,
            last_seen: 0,
            latency_ms: None,
            peer_id: None,
        });

        // Storage node
        registry.register_node(GridNode {
            node_id: "storage-node".into(),
            node_name: None,
            addresses: vec![],
            capabilities: vec![NodeCapability::Storage {
                available_mb: 500_000,
            }],
            trust_level: TrustLevel::Trusted,
            last_seen: 0,
            latency_ms: None,
            peer_id: None,
        });

        let compute_nodes = registry.nodes_with_capability("compute");
        assert_eq!(compute_nodes.len(), 1);
        assert_eq!(compute_nodes[0].node_id, "gpu-node");

        let storage_nodes = registry.nodes_with_capability("storage");
        assert_eq!(storage_nodes.len(), 1);
        assert_eq!(storage_nodes[0].node_id, "storage-node");

        let inference_nodes = registry.nodes_with_capability("inference");
        assert!(inference_nodes.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_trust_level_update() {
        let dir = std::env::temp_dir().join("grid-test-reg-trust");
        let registry = NodeRegistry::new(&dir);

        registry.register_node(GridNode {
            node_id: "node-1".into(),
            node_name: None,
            addresses: vec![],
            capabilities: vec![],
            trust_level: TrustLevel::Blocked,
            last_seen: 0,
            latency_ms: None,
            peer_id: None,
        });

        assert_eq!(
            registry.get("node-1").unwrap().trust_level,
            TrustLevel::Blocked
        );

        registry.set_trust("node-1", TrustLevel::Owner).unwrap();
        assert_eq!(
            registry.get("node-1").unwrap().trust_level,
            TrustLevel::Owner
        );

        // Unknown node returns error
        assert!(registry
            .set_trust("nonexistent", TrustLevel::Trusted)
            .is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_remove_node() {
        let dir = std::env::temp_dir().join("grid-test-reg-remove");
        let registry = NodeRegistry::new(&dir);

        registry.register_node(GridNode {
            node_id: "removeme".into(),
            node_name: None,
            addresses: vec![],
            capabilities: vec![],
            trust_level: TrustLevel::Blocked,
            last_seen: 0,
            latency_ms: None,
            peer_id: None,
        });

        assert!(registry.get("removeme").is_some());
        let removed = registry.remove("removeme");
        assert!(removed.is_some());
        assert!(registry.get("removeme").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod audit_integration {
    use crate::modules::grid::audit::*;

    #[tokio::test]
    async fn test_audit_write_and_read() {
        let dir = std::env::temp_dir().join("grid-test-audit");
        let _ = std::fs::remove_dir_all(&dir);

        let log = AuditLog::new(&dir);

        // Write entries
        for i in 0..10 {
            log.log(&AuditEntry {
                timestamp: 1000 + i,
                direction: if i % 2 == 0 {
                    AuditDirection::Inbound
                } else {
                    AuditDirection::Outbound
                },
                remote_node: format!("node-{i}"),
                command: "gpu/stats".into(),
                correlation_id: format!("corr-{i}"),
                outcome: AuditOutcome::Success,
                duration_ms: 10 + i,
            })
            .await
            .unwrap();
        }

        // Read last 5
        let recent = log.recent(5).await.unwrap();
        assert_eq!(recent.len(), 5);
        // recent() returns in reverse order (most recent first)
        assert_eq!(recent[0].remote_node, "node-9");
        assert_eq!(recent[4].remote_node, "node-5");

        // Read all
        let all = log.recent(100).await.unwrap();
        assert_eq!(all.len(), 10);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_audit_empty_file() {
        let dir = std::env::temp_dir().join("grid-test-audit-empty");
        let _ = std::fs::remove_dir_all(&dir);

        let log = AuditLog::new(&dir);
        let recent = log.recent(10).await.unwrap();
        assert!(recent.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod router_integration {
    use crate::modules::grid::node::*;
    use crate::modules::grid::registry::NodeRegistry;
    use crate::modules::grid::router::{GridRouter, RouteDecision};

    fn setup_router_with_5090() -> (GridRouter, NodeRegistry) {
        let dir = std::env::temp_dir().join("grid-test-router-int");
        let registry = NodeRegistry::new(&dir);

        // Register a 5090 tower
        registry.register_node(GridNode {
            node_id: "100.124.122.107".into(),
            node_name: Some("bigmama".into()),
            addresses: vec![TransportAddress::Tailscale {
                ip: "100.124.122.107".into(),
                port: 7117,
                machine_name: Some("bigmama".into()),
            }],
            capabilities: vec![NodeCapability::Compute {
                gpu: Some("RTX 5090".into()),
                vram_mb: Some(32768),
            }],
            trust_level: TrustLevel::Owner,
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            latency_ms: Some(47),
            peer_id: None,
        });

        // Router: Mac with no GPU
        let router = GridRouter::new(false, 0);
        (router, registry)
    }

    #[test]
    fn test_genome_train_routes_to_5090() {
        let (router, registry) = setup_router_with_5090();

        let decision = router.route(
            "genome/train",
            &serde_json::json!({"personaId": "abc", "baseModel": "llama-3.2-3b"}),
            &registry,
        );

        match decision {
            RouteDecision::Remote { node, reason } => {
                assert_eq!(node.node_id, "100.124.122.107");
                assert_eq!(reason, "no local GPU");
            }
            RouteDecision::Local => panic!("Should route to 5090, not local"),
        }
    }

    #[test]
    fn test_ai_inference_routes_to_5090() {
        let (router, registry) = setup_router_with_5090();

        let decision = router.route(
            "ai/generate",
            &serde_json::json!({"prompt": "hello"}),
            &registry,
        );

        match decision {
            RouteDecision::Remote { node, .. } => {
                assert_eq!(node.node_id, "100.124.122.107");
            }
            RouteDecision::Local => panic!("Should route to 5090"),
        }
    }

    #[test]
    fn test_non_gpu_command_stays_local() {
        let (router, registry) = setup_router_with_5090();

        let decision = router.route(
            "data/list",
            &serde_json::json!({"collection": "users"}),
            &registry,
        );

        assert!(matches!(decision, RouteDecision::Local));
    }

    #[test]
    fn test_named_node_hint() {
        let (router, registry) = setup_router_with_5090();

        let decision = router.route(
            "data/list",
            &serde_json::json!({"routingHint": "node:bigmama"}),
            &registry,
        );

        match decision {
            RouteDecision::Remote { node, reason } => {
                assert_eq!(node.node_name, Some("bigmama".into()));
                assert_eq!(reason, "named node hint");
            }
            RouteDecision::Local => panic!("Should route to bigmama"),
        }
    }

    #[test]
    fn test_with_local_gpu_stays_local() {
        let dir = std::env::temp_dir().join("grid-test-router-local-gpu");
        let registry = NodeRegistry::new(&dir);
        let router = GridRouter::new(true, 8192); // Has GPU

        let decision = router.route("genome/train", &serde_json::json!({}), &registry);

        assert!(matches!(decision, RouteDecision::Local));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod acl_integration {
    use crate::modules::grid::acl::is_command_authorized;
    use crate::modules::grid::node::TrustLevel;

    #[test]
    fn test_owner_trust_allows_all_commands() {
        // Every command in the system should work for owner-trusted nodes
        let commands = [
            "gpu/stats",
            "gpu/pressure",
            "genome/train",
            "genome/dataset-prepare",
            "ai/generate",
            "ai/report",
            "cognition/create-engine",
            "sentinel/execute",
            "plasticity/compact",
            "code/read",
            "code/write",
            "data/list",
            "data/create",
            "embedding/generate",
            "search/query",
            "health-check",
            "models/list",
            "system/resources",
            "screenshot",
            "collaboration/chat/send",
            "voice/synthesize",
        ];

        for cmd in &commands {
            assert!(
                is_command_authorized(cmd, TrustLevel::Owner),
                "Owner should be authorized for '{cmd}'"
            );
        }
    }

    #[test]
    fn test_blocked_trust_allows_nothing() {
        let commands = ["gpu/stats", "health-check", "data/list"];
        for cmd in &commands {
            assert!(
                !is_command_authorized(cmd, TrustLevel::Blocked),
                "Blocked should not be authorized for '{cmd}'"
            );
        }
    }
}
