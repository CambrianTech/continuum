#![cfg(test)]

//! The endpoint-owned decode permits across URLs and engine incarnations (card 7fba9b7b).

use super::super::*;

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

// what this catches: permits sized from the GLOBAL snapshot instead of the endpoint's own engine
// (another URL was sized from the gateway), and a replacement incarnation that keeps the old one's
// size (a smaller engine over-admitted) or forgets a permit taken before it (one admission counted
// by neither). One owner per URL, sized by each incarnation's contract at its readiness, and a
// permit from before the relaunch counts against the new size.
#[tokio::test]
async fn each_url_owns_its_permits_and_a_replacement_resizes_them_at_readiness() {
    let dir = SlotDirectory { endpoints: dashmap::DashMap::new() };
    let gateway = dir.endpoint("test://gateway");
    let other = dir.endpoint("test://other");
    let launch = gateway.transition().await;
    let old = launch.start_generation().expect("test: first generation");
    launch.ready("test://gateway", &old, contract(3)).expect("test: first engine verified");
    drop(launch);
    assert_eq!(gateway.concurrency().available_permits(), 3, "sized by its own contract");
    assert_eq!(other.concurrency().available_permits(), 0, "another URL is not sized by this engine");

    let held = gateway.concurrency().bound().try_acquire().expect("test: a decode permit");
    let replace = gateway.transition().await;
    old.observed_exit();
    let new = replace.start_generation().expect("test: replacement generation");
    replace.ready("test://gateway", &new, contract(1)).expect("test: replacement verified");
    drop(replace);
    assert_eq!(gateway.concurrency().available_permits(), 0, "the held permit from before the relaunch is the one slot");
    drop(held);
    assert_eq!(gateway.concurrency().available_permits(), 1, "it returns into the smaller incarnation's size");

    let grow = gateway.transition().await;
    new.observed_exit();
    let larger = grow.start_generation().expect("test: larger generation");
    grow.ready("test://gateway", &larger, contract(4)).expect("test: larger verified");
    drop(grow);
    assert_eq!(gateway.concurrency().available_permits(), 4, "a larger incarnation grows at readiness");
    let discovered = dir.ensure_pool("test://gateway", 8);
    assert_eq!(discovered.n_slots(), 4, "a late discovery returns the installed pool");
    assert_eq!(gateway.concurrency().available_permits(), 4, "and never resizes the permits past it");
    assert_eq!(other.concurrency().available_permits(), 0);
}
