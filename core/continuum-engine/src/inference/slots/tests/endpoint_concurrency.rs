//! The endpoint-owned decode permits across URLs and engine incarnations (card 7fba9b7b).

use super::super::*;
use std::sync::Arc;

fn contract(slots: u32) -> KvPageContract {
    KvPageContract {
        model_id: "fixture".into(),
        model: "fixture.gguf".into(),
        adapters: Vec::new(),
        page_dir: Some("fixture-pages".into()),
        context: 32768,
        slots,
        cache_type: Some("q8_0".into()),
        flash_attn: Some(true),
        revisions: None,
    }
}

async fn retire_to(endpoint: &EndpointSlots, available: usize) -> usize {
    for _ in 0..100 {
        if endpoint.concurrency().permits().available_permits() == available {
            break;
        }
        tokio::task::yield_now().await;
    }
    endpoint.concurrency().permits().available_permits()
}

// what this catches: permits sized from the GLOBAL snapshot instead of the endpoint's own engine
// (another URL was sized from the gateway), and a replacement incarnation that hands adapters a new
// semaphore (a permit taken before the relaunch would bind nothing) or keeps the old one's size (a
// smaller engine over-admitted). One owner per URL, the same permits across incarnations, sized by
// each incarnation's contract.
#[tokio::test]
async fn each_url_owns_its_permits_and_a_replacement_resizes_the_same_ones() {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    let gateway = dir.endpoint("test://gateway");
    let other = dir.endpoint("test://other");
    let launch = gateway.transition().await;
    let old = launch.start_generation().expect("test: first generation");
    launch.ready("test://gateway", &old, contract(3)).expect("test: first engine verified");
    drop(launch);
    assert_eq!(gateway.concurrency().permits().available_permits(), 3, "sized by its own contract");
    assert_eq!(other.concurrency().permits().available_permits(), 0, "another URL is not sized by this engine");

    let before = gateway.concurrency().permits();
    let held = Arc::clone(&before).try_acquire_owned().expect("test: a decode permit");
    let replace = gateway.transition().await;
    old.observed_exit();
    let new = replace.start_generation().expect("test: replacement generation");
    replace.ready("test://gateway", &new, contract(1)).expect("test: replacement verified");
    drop(replace);
    assert!(Arc::ptr_eq(&before, &gateway.concurrency().permits()), "one semaphore across incarnations");
    assert_eq!(retire_to(&gateway, 0).await, 0, "the free surplus retires; the held permit is never revoked");
    drop(held);
    assert_eq!(retire_to(&gateway, 1).await, 1, "the smaller incarnation admits exactly its slots");

    let grow = gateway.transition().await;
    new.observed_exit();
    let larger = grow.start_generation().expect("test: larger generation");
    grow.ready("test://gateway", &larger, contract(4)).expect("test: larger verified");
    drop(grow);
    assert_eq!(gateway.concurrency().permits().available_permits(), 4, "a larger incarnation grows at once");
    assert_eq!(other.concurrency().permits().available_permits(), 0);
}
