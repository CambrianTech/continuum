#![cfg(test)]

//! The stop's resident checkpoint never waits on acts the turn settle handed back (card b5b9bef3).

use super::super::*;
use super::served_endpoint;

// what this catches (card b5b9bef3; the M5 2026-11-14 00:51:56Z-00:53:34Z, 23:40Z, 19:19Z): the
// stop's checkpoint taking the endpoint's writer while admitted acts still held it, with nothing
// to end them: their turns were already handed back, so the save waited on their decodes and the
// core never reached its exit. Condemned first, the held acts let go and the checkpoint runs; with
// the writer taken before the condemnation this runs into its cap.
#[tokio::test]
async fn the_stop_checkpoint_condemns_held_acts_before_it_takes_the_writer() {
    let dir = SlotDirectory {
        endpoints: dashmap::DashMap::new(),
    };
    let (endpoint, _generation) = served_endpoint(&dir, "test://stopping").await;
    let condemnations = Arc::new(crate::inference::llama_server::Condemnations::new());
    let mut acts = Vec::new();
    for _ in 0..4 {
        let admission = endpoint.admit().await.expect("test: admitted on the ready endpoint");
        let condemned = condemnations.subscribe();
        acts.push(tokio::spawn(async move {
            crate::inference::llama_server::until_condemned_on(condemned).await;
            drop(admission);
        }));
    }
    let cap = std::time::Duration::from_secs(5);
    let checkpoint = super::super::page_io::checkpoint_endpoint(
        &reqwest::Client::new(),
        "test://stopping",
        &endpoint,
        cap,
        || {
            condemnations.condemn();
        },
    );
    let outcome = tokio::time::timeout(cap * 2, checkpoint)
        .await
        .expect("the checkpoint returns");
    // The fixture endpoint is not the process directory's, so the save itself refuses; what is
    // pinned is that the checkpoint got past the writer instead of waiting out its cap.
    let refused = outcome
        .expect("test: an owned engine is checkpointed")
        .expect_err("test: the fixture endpoint refuses the save");
    assert!(!refused.contains("ran out"), "the writer was reached before the cap: {refused}");
    for act in acts {
        act.await.expect("test: act task");
    }
}
