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
// rendered as offered (its path, never its alias), its arguments decode as the verb's params, the cause
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
        "act #9 is not on your record: because the record ends at #3. Next: fixture_refusal-reopen({\"act\":3}): your latest act;"
    );
    let Some(call) = refusal.next().calls().next() else { panic!("one next call") };
    assert_eq!(call.verb().canonical(), "fixture/refusal-reopen");
    let decoded: Result<reopen::P, _> = serde_json::from_value(serde_json::Value::Object(call.args().clone())); // boundary: her tool-call wire, decoded as the verb's params
    assert!(matches!(decoded, Ok(reopen::P { act: 3 })), "the next call's arguments decode as the verb's params");
    let error: crate::sdk_codegen::CommandError = refusal.into();
    assert!(matches!(&error, crate::sdk_codegen::CommandError::Refused(refused) if refused.category() == Category::NotFound), "{error}");
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
    let Ok(text) = serde_json::to_value(&wire) else { panic!("a refusal serializes") }; // boundary: the grid wire a refusal crosses, round-tripped
    assert!(matches!(serde_json::from_value::<continuum_airc_protocol::CommandRefusal>(text), Ok(back) if back == wire));
}

/// A command whose handler refuses with a next call: never registered, dispatched directly.
struct RefusingSpec;
impl crate::sdk_codegen::CommandSpec for RefusingSpec {
    const NAME: &'static str = "fixture/refusal-refusing";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Bare;
    type Params = reopen::P;
    type Result = reopen::R;
}

struct Refusing;

#[async_trait::async_trait]
impl crate::sdk_codegen::CommandHandler for Refusing {
    type Spec = RefusingSpec;
    async fn execute(
        &self,
        _ctx: &crate::sdk_codegen::Ctx,
        p: reopen::P,
    ) -> Result<crate::sdk_codegen::Outcome<reopen::R>, crate::sdk_codegen::CommandError> {
        let next = NextCall::new(reopen::REOPEN, &reopen::P { act: 3 }, "your latest act")
            .map_err(|unwritable| crate::sdk_codegen::CommandError::Internal(unwritable.to_string()))?;
        Err(Refusal::new(UnknownAct { act: p.act, because: std::io::Error::other("the record ends at #3") }, NextMoves::call(next)).into())
    }
}

// what this catches: a refusal's next moves flattened into its sentence where it leaves dispatch,
// so the mind she acts through could only read the commands back out of prose (Aiko on the M5,
// 2026-11-13: 8 refused `work/submit` calls naming verbs her tool list offers under other names).
// The failure keeps the sentence she reads and the refusal typed, each next call by its command.
#[tokio::test]
async fn a_refusal_leaves_dispatch_with_its_next_moves_typed() {
    let failure = match crate::sdk_codegen::dispatch_with_caller(&Refusing, serde_json::json!({ "act": 9 }), None).await {
        Err(failure) => failure,
        Ok(_) => panic!("the handler refuses"),
    };
    let sentence = &failure.message;
    assert!(sentence.contains("fixture_refusal-reopen({\"act\":3})"), "the sentence renders the call as offered: {sentence}");
    let Some(refusal) = failure.refusal else { panic!("a refusal with next moves crosses typed: {}", failure.message) };
    assert_eq!(refusal.category, Category::NotFound);
    assert_eq!(refusal.next.commands().collect::<Vec<_>>(), ["fixture/refusal-reopen"]);
    let plain = crate::sdk_codegen::dispatch_with_caller(&Refusing, serde_json::json!({ "act": "nine" }), None).await;
    assert!(matches!(plain, Err(crate::sdk_codegen::CommandFailure { refusal: None, .. })), "a refusal without next moves carries none");
}
