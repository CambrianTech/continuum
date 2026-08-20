//! End-to-end proof of the contracted grid: owner ISSUES a signed capability
//! grant → grantee PRESENTS it on a cross-grid command → owner VERIFIES it and
//! runs an otherwise tier-DENIED command. This is the loop that makes the grid
//! "ready for compute" — a peer can be sold a capability (e.g. `ai/generate`,
//! `compute/run`) and exercise exactly it, cryptographically, with nothing else.
//!
//! Two REAL airc peers over the loopback fixture (mutual enrolment + LAN), the
//! production install path (`PersonaCommandInboundPump` + `build_grant_authorizer`),
//! the production gate (`GridTrustAuthPolicy`), and the production send path
//! (`AircTransport` + `InMemoryPresentedGrantStore`). No mock transport, no manual
//! handler wiring.
//!
//! what this catches: a regression anywhere in issue → present → verify → dispatch
//! that would either (a) let a tier-denied command through WITHOUT a grant
//! (authorization hole) or (b) fail to honor a valid owner-signed grant (the grid
//! can't sell compute). Both are asserted.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use airc_core::PeerId;
use airc_test_fixtures::TwoAircLoopback;
use async_trait::async_trait;
use continuum_core::persona::command_inbound_pump::{
    build_grant_authorizer, PersonaCommandInboundPump,
};
use continuum_core::routing::grant_issuance::{issue_grant, IssueGrantParams};
use continuum_core::routing::presented_grant_store::InMemoryPresentedGrantStore;
use continuum_core::routing::{route, AircTransport, CommandUri, GridTrustAuthPolicy, Transport};
use continuum_core::runtime::command_executor::CommandExecutor;
use continuum_core::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
};
use serde_json::json;

/// A trivial tier-DENIED command (`compute/echo`) the owner exposes — not in the
/// `ai/generate` namespace, so a remote caller is forbidden it by default. Echoes
/// its params so the test can prove the body round-tripped through the full chain.
struct EchoModule;

#[async_trait]
impl ServiceModule for EchoModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "compute-echo",
            priority: ModulePriority::Normal,
            command_prefixes: &["compute/echo"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }
    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }
    async fn handle_command(
        &self,
        _command: &str,
        params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Ok(CommandResult::Json(params))
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tokio::test]
async fn owner_signed_grant_lets_grantee_run_a_tier_denied_command() {
    let loop_back = TwoAircLoopback::new().await.expect("loopback fixture");
    let owner = loop_back.peer_a(); // issuer + verifier (the compute seller)
    let grantee = loop_back.peer_b(); // buyer presenting the grant
    let owner_id = loop_back.peer_a_id();
    let grantee_id = loop_back.peer_b_id();

    // --- Owner side: the production gate + the EchoModule, addressable via the pump.
    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(EchoModule) as Arc<dyn ServiceModule>);
    let executor =
        Arc::new(CommandExecutor::new(registry).with_policy(Arc::new(GridTrustAuthPolicy::new())));
    let owner_home = tempfile::tempdir().expect("owner home");
    let grant_authorizer = build_grant_authorizer(owner, owner_home.path())
        .await
        .expect("owner builds its grant authorizer");
    let pump =
        PersonaCommandInboundPump::spawn(owner_id, Arc::clone(owner), executor, grant_authorizer)
            .await
            .expect("install owner command pump");

    let decision = || {
        route(
            &CommandUri::parse(&format!("airc://{owner_id}/compute/echo")).expect("valid peer URI"),
        )
    };

    // --- (1) WITHOUT a grant: the tier gate DENIES compute/echo to the remote peer.
    let no_grant = AircTransport::new(Arc::clone(grantee));
    let denied = no_grant.dispatch(decision(), json!({"msg": "hello"})).await;
    assert!(
        denied.is_err(),
        "a remote peer must be DENIED a tier-gated command with no grant; got {denied:?}"
    );

    // --- Owner ISSUES a grant for the grantee conferring exactly compute/echo,
    // via the production issuance primitive (binds the grantee's authenticated key
    // + the owner's mesh + the owner's signature, all from the owner handle).
    let grant_b64 = issue_grant(
        owner,
        now_ms(),
        IssueGrantParams {
            grantee: PeerId(grantee_id),
            capabilities: vec!["compute/echo".to_string()],
            expires_at_ms: Some(now_ms() + 3_600_000),
            epoch: 1,
        },
    )
    .await
    .expect("owner issues the grant");

    // --- Grantee HOLDS the grant (keyed by the owner it presents to) + a transport
    // that presents it.
    let store = Arc::new(InMemoryPresentedGrantStore::new());
    store.insert(PeerId(owner_id), grant_b64);
    let granted = AircTransport::new(Arc::clone(grantee)).with_grant_store(store);

    // --- (2) WITH the grant presented: the owner verifies it and RUNS compute/echo.
    let result = granted
        .dispatch(decision(), json!({"msg": "hello"}))
        .await
        .expect("an owner-signed grant must let the grantee run the conferred command");
    match result {
        CommandResult::Json(value) => assert_eq!(
            value,
            json!({"msg": "hello"}),
            "the command ran and echoed the params through the full grid chain"
        ),
        other => panic!("expected Json echo, got {other:?}"),
    }

    pump.shutdown().await;
}
