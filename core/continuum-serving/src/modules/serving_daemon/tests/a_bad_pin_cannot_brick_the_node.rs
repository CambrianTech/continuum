#![cfg(test)]

use super::*;

// what this catches: the #3955 outage, twice on this host. A stored pin naming
// `AtomicChat/Qwen3.8-Flash-Next-GGUF` (`availability: not_downloaded`) was
// honoured absolutely at boot: every downloaded candidate refused as "another
// model is pinned", the pinned one unloadable, so zero lanes and zero citizens —
// with NOTHING red. `serving/pin` fit-gates at SET time, so this shape can only
// arrive via RESTORE, which had no check at all.
//
// The daemon's constructor needs a GPU, a monitor and a resource daemon, so the
// decision is asserted where it is made: honour a restored pin only when the
// catalog says the model is actually servable.
#[test]
fn a_restored_pin_is_honoured_only_when_the_model_is_servable() {
    let src = production_source();
    assert!(
                src.contains("serving.pin.unservable_ignored"),
                "an unservable restored pin must be IGNORED with a loud probe — honouring                  it refuses every real candidate and takes the node to zero lanes"
            );
    assert!(
                src.contains("entry.status.availability == Availability::Ready"),
                "and the test for servable must be the catalog's own availability, not a                  guess about the file"
            );
    // Intent is durable: the file is kept so the pin is honoured again once the
    // weights land. A fix that silently discarded the operator's choice would be
    // its own bug.
    assert!(
                !src.contains("pin_store.clear()"),
                "the pin FILE must survive being ignored — pull the weights and the next                  boot honours it; only serving/unpin drops the intent"
            );
}

// what this catches: the refusal that named every loser and never the winner.
// Three refusals a second, none identifying the pin doing the refusing, so
// diagnosing it needed a grep of a different log for `serving.pin.restored`.
#[test]
fn a_refusal_names_the_pin_that_caused_it() {
    let src = production_source();
    let probe = src
        .split("class = \"serving.plan.candidate_refused\"")
        .nth(1)
        .expect("the refusal probe exists");
    let body = &probe[..probe.find(");").expect("probe ends")];
    assert!(
                body.contains("pinned ="),
                "the refusal must carry the pin that blocked the candidate — a reason that                  states its category but not its cause cannot be acted on: {body}"
            );
}
