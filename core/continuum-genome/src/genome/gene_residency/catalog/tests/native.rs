//! The gene catalog's owner against a REAL engine (CONTINUUM-ENGINE 4.10 P4a; the Windows cutover
//! gate Codex named): the fork's llama-server, adopted (this process never owned it), with residents'
//! KV in its slots. A gene the engine itself wrote is loaded and retired through `GeneCatalog`, and
//! every resident's cached prefix must survive each change; an engine replaced on the same port
//! between the identity read and the mutation must refuse with nothing unloaded.
//!
//! Ignored by default: it needs the engine binary and a model it loads. Run it with
//! `CONTINUUM_NATIVE_ENGINE_BIN=<fork llama-server> CONTINUUM_NATIVE_ENGINE_MODEL=<gguf>
//! cargo test -p continuum-genome --lib native -- --ignored --nocapture`. Any model whose attention
//! tensors are named `attn_q`/`attn_v` works; a tiny one is enough.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};

use super::*;
use crate::genome::learner::seat::{seat, Authority};
use crate::inference::engine_residency::EngineIncarnation;
use crate::inference::llama_server::{FreshShadow, LedgerAnswer};

const GENE: &str = "native-gene.gguf";

/// A real engine process serving `port`, killed when dropped.
struct Engine {
    child: tokio::process::Child,
    root: String,
}

impl Engine {
    /// Stop the engine and wait for its exit, so its port is free for a replacement.
    async fn stop(mut self) {
        self.child.kill().await.expect("test: the engine stops"); // test: a live child we own
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _killed = self.child.start_kill(); // test teardown: an engine already gone needs no kill
    }
}

fn required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("test: set {var} to run the native gene-catalog proof")) // test: names the missing input
}

/// A free local port, released for the engine to bind.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test: a local port"); // test: loopback bind
    listener.local_addr().expect("test: its address").port() // test: a bound socket has an address
}

/// Start the engine on `port` with `train_dir`, returning once it says it is listening (its own
/// readiness line, never a timer). A process that exits first fails the test with what it printed.
async fn start(port: u16, train_dir: &std::path::Path) -> Engine {
    let mut child = tokio::process::Command::new(required("CONTINUUM_NATIVE_ENGINE_BIN"))
        .args(["-m", &required("CONTINUUM_NATIVE_ENGINE_MODEL"), "--host", "127.0.0.1", "--port", &port.to_string(), "-np", "3", "-c", "6144"])
        .arg("--train-dir")
        .arg(train_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("test: the engine binary starts"); // test: the named binary exists
    let mut lines = BufReader::new(child.stderr.take().expect("test: stderr was piped")).lines(); // test: piped above
    let mut printed = Vec::new();
    loop {
        match lines.next_line().await.expect("test: the engine's stderr reads") { // test: a pipe read
            Some(line) if line.contains("listening on") => break, // printed after the model loads
            Some(line) => printed.push(line),
            None => panic!("test: the engine exited before listening:\n{}", printed.join("\n")),
        }
    }
    // keep draining stderr so the engine never blocks on a full pipe
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Engine { child, root: format!("http://127.0.0.1:{port}") }
}

/// The engine as the learning routes know it: its process incarnation at `port`.
fn learning(engine: &Engine, port: u16) -> LearningEngine {
    let pid = engine.child.id().expect("test: a running engine has a pid"); // test: spawned above
    LearningEngine::of(EngineIncarnation::of(pid, port).expect("test: its start time is readable")) // test: a live local process
}

/// The engine's identity, read first, as `converge_genome` reads it before the catalog.
async fn identity(engine: &LearningEngine) -> LedgerEngine {
    match engine.learn_ledger().await.expect("test: the ledger answers") { // test: a live engine with --train-dir
        LedgerAnswer::Ledger(snapshot) => snapshot.engine,
        LedgerAnswer::NoLedger => panic!("test: this engine has no learning ledger; build the fork with the learning routes"),
    }
}

/// A resident's act on `slot`: the cached prefix the engine reused for it.
async fn act(client: &reqwest::Client, root: &str, slot: u32, prompt: &str) -> u64 {
    let body = serde_json::json!({ "prompt": prompt, "n_predict": 2, "id_slot": slot, "cache_prompt": true, "temperature": 0.0 });
    let reply: serde_json::Value = client
        .post(format!("{root}/completion"))
        .json(&body)
        .send()
        .await
        .expect("test: the completion route answers") // test: a live engine
        .json()
        .await
        .expect("test: the completion reply is JSON"); // test: the engine's own reply shape
    reply["timings"]["cache_n"].as_u64().expect("test: the reply names its cached prefix") // test: the fork reports timings.cache_n
}

/// The ids the engine lists for adapter `name`.
async fn listed(client: &reqwest::Client, root: &str, name: &str) -> Vec<i64> {
    let catalog: serde_json::Value = client.get(format!("{root}/lora-adapters")).send().await.expect("test: the catalog answers").json().await.expect("test: the catalog is JSON"); // test: a live engine
    catalog
        .as_array()
        .expect("test: the catalog is a list") // test: the engine's own reply shape
        .iter()
        .filter(|entry| entry["path"].as_str().is_some_and(|path| path.ends_with(name)))
        .filter_map(|entry| entry["id"].as_i64())
        .collect()
}

const RESIDENTS: [&str; 2] = [
    "Aiko is reading the scheduler and writing down what each step of the engine does, one line at a time.",
    "Aris is reviewing a pull request about grants, checking each claim against the code it names.",
];

/// Every resident acts again; each must reuse exactly the prefix she reused before.
async fn residents_kept_their_kv(client: &reqwest::Client, root: &str, before: &[u64], when: &str) {
    for ((slot, prompt), reused) in (0u32..).zip(RESIDENTS).zip(before) {
        let cached = act(client, root, slot, prompt).await;
        assert_eq!(cached, *reused, "{when}: resident in slot {slot} lost her cached prefix");
    }
}

// what this catches (the 03:27 adoption failure; Codex's cutover gate): a gene change on an adopted
// engine that unloads by bare id (refused), loads with no residency (refused), or disturbs the
// residents' KV. Through the owner, the gene loads inside a leased residency and retires bound to the
// engine, and both residents reuse the same cached prefix after each change.
#[tokio::test]
#[ignore = "native: needs CONTINUUM_NATIVE_ENGINE_BIN and CONTINUUM_NATIVE_ENGINE_MODEL"]
async fn an_adopted_engines_gene_change_keeps_every_residents_kv() {
    let train_dir = tempfile::tempdir().expect("test: a train dir"); // test: a temp dir
    let port = free_port();
    let engine = start(port, train_dir.path()).await;
    let client = reqwest::Client::new();
    let mut before = Vec::new();
    for (slot, prompt) in (0u32..).zip(RESIDENTS) {
        act(&client, &engine.root, slot, prompt).await;
        before.push(act(&client, &engine.root, slot, prompt).await); // the prefix she reuses
    }
    assert!(before.iter().all(|cached| *cached > 0), "the engine reuses each resident's prefix: {before:?}");

    // the engine writes the gene itself: a fresh shadow, seated, unloaded and released, leaves its file
    let authority = Authority::isolated(u64::MAX / 2);
    let learner = learning(&engine, port);
    let shadow = FreshShadow { name: GENE.into(), rank: 4, alpha: 8.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 1, seed: 1 };
    let written = seat(&authority, learner.clone(), "native:writer", shadow).await.expect("test: the engine writes a shadow"); // test: a live engine with --train-dir
    written.unload().await.expect("test: the writer's shadow unloads"); // test: it was just loaded
    if let Err((_seat, why)) = written.release().await {
        panic!("test: the writer's residency did not release: {why}");
    }
    assert!(train_dir.path().join(GENE).exists(), "the gene file is in the engine's train dir");
    residents_kept_their_kv(&client, &engine.root, &before, "after the engine wrote the gene").await;

    // adopted: the catalog's owner, under a held serving gate, bound to the identity read first
    let operation = authority.serving.try_operation().expect("test: a fresh gate is free"); // test: nothing else holds it
    let catalog = GeneCatalog::new(Authority::isolated(u64::MAX / 2));
    let ledger = identity(&learner).await;
    let gene = train_dir.path().join(GENE).to_string_lossy().into_owned();
    let add = CatalogChange {
        operation: &operation,
        engine: learner.clone(),
        ledger: ledger.clone(),
        retire: vec![],
        add: vec![Add { gene: gene.clone(), name: GENE.into(), staged: train_dir.path().join(GENE) }],
    };
    let loaded = catalog.converge(add).await;
    assert_eq!(loaded.end, CatalogEnd::Done, "the gene loads inside a leased residency: {loaded:?}");
    let ids = listed(&client, &engine.root, GENE).await;
    assert_eq!(ids.len(), 1, "the engine lists the gene once");
    residents_kept_their_kv(&client, &engine.root, &before, "after the gene loaded").await;

    let retire = CatalogChange { operation: &operation, engine: learner.clone(), ledger, retire: vec![Retire { gene: gene.clone(), id: ids[0] }], add: vec![] };
    let retired = catalog.converge(retire).await;
    assert_eq!(retired.end, CatalogEnd::Done, "the gene retires bound to this engine: {retired:?}");
    assert!(listed(&client, &engine.root, GENE).await.is_empty(), "the engine no longer lists it");
    residents_kept_their_kv(&client, &engine.root, &before, "after the gene retired").await;
}

// what this catches (Codex): an engine replaced on the same port between the identity read and the
// mutation. The change bound to the old identity refuses: the load before anything is leased, the
// retire with nothing unloaded from the replacement.
#[tokio::test]
#[ignore = "native: needs CONTINUUM_NATIVE_ENGINE_BIN and CONTINUUM_NATIVE_ENGINE_MODEL"]
async fn a_replacement_on_the_same_port_refuses_the_change_bound_to_the_old_engine() {
    let train_dir = tempfile::tempdir().expect("test: a train dir"); // test: a temp dir
    let port = free_port();
    let first = start(port, train_dir.path()).await;
    let authority = Authority::isolated(u64::MAX / 2);
    let learner = learning(&first, port);
    let shadow = FreshShadow { name: GENE.into(), rank: 4, alpha: 8.0, targets: vec!["attn_q".into(), "attn_v".into()], top_layers: 1, seed: 1 };
    let written = seat(&authority, learner.clone(), "native:writer", shadow).await.expect("test: the engine writes a shadow"); // test: a live engine with --train-dir
    written.unload().await.expect("test: the writer's shadow unloads"); // test: it was just loaded
    if let Err((_seat, why)) = written.release().await {
        panic!("test: the writer's residency did not release: {why}");
    }
    let stale = identity(&learner).await;
    first.stop().await; // the engine is replaced on the same port
    let second = start(port, train_dir.path()).await;
    let client = reqwest::Client::new();

    let operation = authority.serving.try_operation().expect("test: a fresh gate is free"); // test: nothing else holds it
    let resources = std::sync::Arc::clone(&authority.resources);
    let catalog = GeneCatalog::new(authority);
    let gene = train_dir.path().join(GENE).to_string_lossy().into_owned();
    let add = CatalogChange {
        operation: &operation,
        engine: learner.clone(),
        ledger: stale.clone(),
        retire: vec![],
        add: vec![Add { gene: gene.clone(), name: GENE.into(), staged: train_dir.path().join(GENE) }],
    };
    let refused = catalog.converge(add).await;
    assert_eq!(refused.end, CatalogEnd::Refused(CatalogRefusal::EngineReplaced), "{refused:?}");
    assert!(listed(&client, &second.root, GENE).await.is_empty(), "nothing loaded into the replacement");
    assert_eq!(resources.available_for("an observer", crate::resources::ResourceKind::Vram), u64::MAX / 2, "nothing leased");

    // the replacement holds a gene of its own; a retire bound to the old identity must not touch it
    let replacement = learning(&second, port);
    let own = identity(&replacement).await;
    let operation_two = Authority::isolated(u64::MAX / 2);
    let held = operation_two.serving.try_operation().expect("test: a fresh gate is free"); // test: nothing else holds it
    let theirs = GeneCatalog::new(operation_two);
    let load = CatalogChange { operation: &held, engine: replacement, ledger: own, retire: vec![], add: vec![Add { gene: gene.clone(), name: GENE.into(), staged: train_dir.path().join(GENE) }] };
    assert_eq!(theirs.converge(load).await.end, CatalogEnd::Done);
    let ids = listed(&client, &second.root, GENE).await;
    let retire = CatalogChange { operation: &operation, engine: learner, ledger: stale, retire: vec![Retire { gene, id: ids[0] }], add: vec![] };
    let refused = catalog.converge(retire).await;
    assert!(matches!(refused.end, CatalogEnd::Refused(_)), "a retire bound to the old engine refuses: {refused:?}");
    assert_eq!(listed(&client, &second.root, GENE).await, ids, "the replacement still holds its gene");
}
