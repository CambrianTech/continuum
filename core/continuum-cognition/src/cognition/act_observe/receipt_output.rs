//! What an act's receipt is saved as: the one place that decides whether an act leaves an output
//! id behind, and what that id names.
//!
//! Three rules, each a variant, so none is a check a caller can skip:
//! - an act where nothing ran (every call refused, a cut at the output limit) leaves NO output: its
//!   refusal reached her in the act's result, and no id names it;
//! - an act whose every call that ran reopened ONE saved output is a view of that output: its
//!   custody is that output's id, never a new save of her selection;
//! - any other act saves its own text, in which each read of a saved output is a line naming that
//!   output (never its content again) and a refusal is left out.
//!
//! So an id never names her read of another id, and a read never nests: before this, a cut act's
//! refusal was saved under an id, her read of it (beside the next cut) was saved under another, and
//! her read of THAT under a third, each holding the last (IntelMac 0051cdcf, 2026-11-14: forty
//! minutes reading `tool/output` results three levels deep, each read minting the next level).

use super::observation::Observation;
use crate::ai::types::ResultCustody;
use crate::cognition::tool_executor::spill::LivePin;
use crate::cognition::tool_executor::tool_output::Reopen;

/// What one call of an act left behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallOutput<'a> {
    /// Nothing ran: its refusal is in the act's result, and nowhere else.
    Refused,
    /// It reopened this saved output: a view of it, never an output of its own.
    Reopened(&'a str),
    /// It ran and returned an output of its own.
    Ran,
}

impl<'a> CallOutput<'a> {
    fn of(act: &'a Observation) -> Self {
        let custody = &act.output.result.custody;
        if *custody == ResultCustody::Refused {
            return Self::Refused;
        }
        let canonical = crate::ai::tool_dialect::resolve_wire_name(&act.call.name);
        let reopens = crate::sdk_codegen::ext::effect_of(&canonical) == continuum_base::tool_effect::ToolEffect::Reopens;
        match custody.handle() {
            Some(source) if reopens => Self::Reopened(source),
            _ => Self::Ran,
        }
    }
}

/// What an act's receipt is saved as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReceiptOutput<'a> {
    /// No call ran: no output, so no id.
    NothingRan,
    /// Every call that ran reopened this one saved output: the receipt is a view of it.
    ViewOf(&'a str),
    /// At least one call returned an output of its own (or two outputs were reopened): saved as
    /// this text.
    Own(String),
}

impl<'a> ReceiptOutput<'a> {
    /// The receipt `acts` leave, given `observation`, the text she was shown for them. An act of
    /// calls that each ran and reopened nothing saves exactly that text, so its pointer resolves
    /// to what she saw.
    pub(crate) fn of(acts: &'a [Observation], observation: &str) -> Self {
        let outputs: Vec<CallOutput<'a>> = acts.iter().map(CallOutput::of).collect();
        let mut ran = outputs.iter().filter(|output| **output != CallOutput::Refused);
        let Some(first) = ran.next() else { return Self::NothingRan };
        if let CallOutput::Reopened(source) = *first {
            if ran.all(|output| *output == CallOutput::Reopened(source)) {
                return Self::ViewOf(source);
            }
        }
        if outputs.iter().all(|output| *output == CallOutput::Ran) {
            return Self::Own(observation.trim().to_owned());
        }
        let resolved: String = acts.iter().zip(&outputs).map(|(act, output)| match output {
            CallOutput::Refused => String::new(),
            CallOutput::Reopened(source) => format!(
                "{}\nResult:\n[your read of saved output {source}, not saved again; its content is {source}'s: {}]\n\n",
                act.call_line(),
                Reopen::Output(source),
            ),
            CallOutput::Ran => act.render_recency(),
        }).collect();
        Self::Own(resolved.trim().to_owned())
    }

    /// Its custody: none for an act where nothing ran, the source for a view, else its own text
    /// saved through `pin` (or `Unsaved` when that save fails).
    pub(crate) fn custody(self, pin: &LivePin, persona: uuid::Uuid) -> ResultCustody {
        match self {
            Self::NothingRan => ResultCustody::Refused,
            Self::ViewOf(source) => ResultCustody::Whole { handle: source.to_owned() },
            Self::Own(text) => pin.retain(persona, &text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::{ToolCall, ToolResult};
    use crate::cognition::act_observe::observation::{ActStatus, ToolOutput, ToolVerb};

    fn act(name: &str, input: serde_json::Value, content: &str, custody: ResultCustody) -> Observation {
        Observation {
            call: ToolCall { id: format!("call-{name}"), name: name.into(), input },
            output: ToolOutput {
                image: None,
                additional_images: None,
                result: ToolResult { tool_use_id: format!("call-{name}"), content: content.into(), is_error: None, custody },
                verb: ToolVerb::classify(name),
                paths: Vec::new(),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        }
    }

    fn whole(handle: &str) -> ResultCustody {
        ResultCustody::Whole { handle: handle.into() }
    }

    fn read(source: &str) -> Observation {
        act("read_output", serde_json::json!({ "outputId": source }), "     1 test it_works ... FAILED", whole(source))
    }

    fn cut() -> Observation {
        act(crate::cognition::tool_executor::command_executor::CUT_AT_LIMIT_SENTINEL, serde_json::json!({}), "refused: cut", ResultCustody::Refused)
    }

    // what this catches: a refused act given an output id (rule 3 of card fc55ff7b). Her cut act's
    // refusal was saved and named, so she read it; a refusal now reaches her in the act's result
    // only, and an act where nothing ran leaves no id at all.
    #[test]
    fn an_act_where_nothing_ran_leaves_no_output() {
        let acts = [cut()];
        assert_eq!(ReceiptOutput::of(&acts, "refused: cut"), ReceiptOutput::NothingRan);
        assert_eq!(ReceiptOutput::NothingRan.custody(&LivePin::new(), uuid::Uuid::nil()).handle(), None);
    }

    // what this catches: a read of a read nesting (rule 2): a read beside a refusal (her cut) was
    // saved as a new output holding the read's content, and her read of THAT as another, a level
    // deeper each time (IntelMac 0051cdcf, 2026-11-14). Every call that ran reopened X, so the
    // receipt is a view of X; reading it again, alone or beside another cut, is X again. Two
    // identical reads leave the same custody, so her repeat is the same outcome her view's repeat
    // rule answers with what she already has (rule 4).
    #[test]
    fn a_read_beside_a_refusal_is_a_view_of_its_source_never_a_new_output() {
        let source = "59db1a33d1db5959";
        for acts in [vec![read(source)], vec![read(source), cut()], vec![cut(), read(source), read(source)]] {
            assert_eq!(ReceiptOutput::of(&acts, "shown"), ReceiptOutput::ViewOf(source), "{acts:?}");
        }
        let custody = |acts: &[Observation]| ReceiptOutput::of(acts, "shown").custody(&LivePin::new(), uuid::Uuid::nil());
        assert_eq!(custody(&[read(source)]), custody(&[read(source), cut()]));
        assert_eq!(custody(&[read(source)]), whole(source));
    }

    // what this catches: a saved receipt that holds a read's content (rule 2): an act that read X
    // and also ran something else saves its own text, where the read is ONE line naming X (her
    // read of it resolves to X and says so), the refusal is left out, and the other call's output
    // is kept whole. An act that only ran saves exactly what she was shown.
    #[test]
    fn a_saved_receipt_names_the_output_a_read_reopened_and_never_holds_its_content() {
        let source = "127e4756aa00bb11";
        let ran = act("code/read", serde_json::json!({ "file_path": "a.rs" }), "fn a() {}", whole("0123456789abcdef"));
        let acts = [read(source), cut(), ran.clone()];
        let ReceiptOutput::Own(text) = ReceiptOutput::of(&acts, "shown") else { panic!("a read beside a read of a file saves its own text") };
        assert!(text.contains(&format!("your read of saved output {source}, not saved again")), "{text}");
        assert!(text.contains(&Reopen::Output(source).to_string()), "{text}");
        assert!(!text.contains("it_works ... FAILED"), "the read's content is not saved again: {text}");
        assert!(!text.contains("refused: cut"), "a refusal is not saved: {text}");
        assert!(text.contains("fn a() {}"), "{text}");
        let two_sources = [read(source), read("0123456789abcdef")];
        assert!(matches!(ReceiptOutput::of(&two_sources, "shown"), ReceiptOutput::Own(_)), "two outputs are no one view");
        assert_eq!(ReceiptOutput::of(&[ran], "  as shown  "), ReceiptOutput::Own("as shown".into()));
    }
}
