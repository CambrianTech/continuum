use super::*;

/// A fixture verb: reopen a saved output by its act number, declared like a real native command.
mod reopen {
    #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
    pub struct P {
        pub act: u64,
    }
    #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
    pub struct R {
        pub success: bool,
    }
    crate::action_command! {
        /// Test fixture: reopen a saved output.
        pub struct Reopen;
        name: "fixture/refusal-reopen",
        access: AiSafe,
        native: true,
        aliases: &["reopen_fixture"],
        disclosure: Always,
        params: P,
        output: R,
        run(_this, _ctx, _p) => { let done = R { success: true }; Ok(done) }
    }
    crate::projects_success_on_return! { Reopen => R }
    pub const REOPEN: crate::sdk_codegen::VerbRef<Reopen> = crate::sdk_codegen::VerbRef::always();
}

#[derive(Debug, thiserror::Error)]
#[error("act #{act} is not on your record")]
struct UnknownAct {
    act: u64,
    #[source]
    because: std::io::Error,
}

impl RefusalCause for UnknownAct {
    fn category(&self) -> Category {
        Category::NotFound
    }
}

// what this catches: a refusal she reads without a next call, or one naming the verb by a name
// her tool list does not offer, or arguments that are not the verb's own params. The call is
// rendered as offered (its first alias), its arguments decode as the verb's params, the cause
// chain is kept, and the category is the cause's own.
#[test]
fn a_refusal_renders_its_cause_chain_and_a_next_call_she_can_make() {
    let Ok(next) = NextCall::new(reopen::REOPEN, &reopen::P { act: 3 }, "your latest act") else {
        panic!("a struct of named fields is a next call's arguments")
    };
    let refusal = Refusal::new(
        UnknownAct { act: 9, because: std::io::Error::other("the record ends at #3") },
        NextMoves::call(next),
    );
    assert_eq!(refusal.category(), Category::NotFound);
    assert_eq!(
        refusal.to_string(),
        "act #9 is not on your record: because the record ends at #3. Next: reopen_fixture({\"act\":3}): your latest act;"
    );
    let Some(call) = refusal.next().calls().next() else { panic!("one next call") };
    assert_eq!(call.verb().canonical(), "fixture/refusal-reopen");
    let decoded: Result<reopen::P, _> = serde_json::from_value(serde_json::Value::Object(call.args().clone()));
    assert!(matches!(decoded, Ok(reopen::P { act: 3 })), "the next call's arguments decode as the verb's params");
    let error: crate::sdk_codegen::CommandError = refusal.into();
    assert!(error.to_string().starts_with("[not_found] act #9"), "{error}");
}

// what this catches: a refusal with nothing she can do still says why, typed, never an empty
// or missing next move.
#[test]
fn a_refusal_with_no_move_says_why() {
    let refusal = Refusal::new(
        UnknownAct { act: 2, because: std::io::Error::other("its save failed") },
        NextMoves::None(NoMove::NothingMoreExists),
    );
    assert_eq!(refusal.next().calls().count(), 0);
    assert!(refusal.to_string().ends_with(". no call recovers it; what you were shown is all there is"), "{refusal}");
}

// what this catches: a refusal losing its next move or its cause chain where it crosses a seam
// (the executor to her client, one node to another). The wire form keeps the chain outermost
// first and each call by its canonical command, and decodes back to the same value.
#[test]
fn a_refusal_crosses_a_seam_with_its_chain_and_next_call() {
    let Ok(next) = NextCall::new(reopen::REOPEN, &reopen::P { act: 3 }, "your latest act") else {
        panic!("a struct of named fields is a next call's arguments")
    };
    let wire = Refusal::new(
        UnknownAct { act: 9, because: std::io::Error::other("the record ends at #3") },
        NextMoves::call(next),
    )
    .to_wire();
    assert_eq!(wire.cause, ["act #9 is not on your record", "the record ends at #3"]);
    let continuum_airc_protocol::NextMoves::Calls(calls) = &wire.next else { panic!("one next call") };
    assert_eq!(calls.all()[0].command, "fixture/refusal-reopen", "the canonical name crosses; the offered name is her renderer's");
    let Ok(text) = serde_json::to_value(&wire) else { panic!("a refusal serializes") };
    assert!(matches!(serde_json::from_value::<continuum_airc_protocol::CommandRefusal>(text), Ok(back) if back == wire));
}
