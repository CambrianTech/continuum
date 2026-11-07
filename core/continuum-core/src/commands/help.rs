//! `commands/help` — the AI-paradigm manual: how to CALL a command, as the call she makes.
//!
//! Symmetry with the CLI: `continuum <command> --help` renders the SAME single schema as
//! bash flags ("the manual matches the paradigm"); this renders it as her tool call: the
//! name her tool list offers and its arguments object (`sdk_codegen::CallForm`). One source
//! (`command_registry_live()` + the command's `params_schema`), two paradigms. So when a
//! persona is unsure HOW to call a tool, it asks `commands/help` and gets back a
//! fill-in-the-blanks example of the call that decodes. [[command-infra-self-routing-schema-adapters]]
//!
//! Gated to the caller's own trust: you only get help for commands you could
//! actually run (no teaching the format of an Owner-only command to a persona).


use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::routing::command_acl::is_command_authorized;
use crate::modules::grid::trust_policy::caller_trust;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use crate::sdk_codegen::ai_help::{did_you_mean, render_ai_help};
use crate::sdk_codegen::ext::command_registry_live;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/help/CommandsHelpParams.ts"
)]
pub struct CommandsHelpParams {
    /// The command to explain, e.g. `code/read`. OMIT it to get an INDEX of every
    /// command you can call (name + one-line description) — your starting point when
    /// you don't yet know which tool to use.
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    /// What you want to do, when you do not know the command's name: the best matches, ranked.
    #[serde(default)]
    #[ts(optional)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/help/CommandsHelpResult.ts"
)]
pub struct CommandsHelpResult {
    pub name: String,
    pub description: String,
    pub access_level: String,
    /// The how-to-call manual rendered in the caller's paradigm: the exact tool-call
    /// envelope to emit (fill in the values) plus per-argument docs.
    pub manual: String,
}

/// A persona's "how do I call this?" — returns the exact tool-call format for a
/// command it is authorized to run.
#[derive(Default)]
pub struct CommandsHelp;

#[async_trait]
impl ActionCommand for CommandsHelp {
    const NAME: &'static str = "commands/help";
    const ALIASES: &'static [&'static str] = &["help"];
    const NATIVE: bool = true; // discovery pair — the on-demand "how do I call this?" tool
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::Always;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Show how to CALL a command: its exact tool-call format + arguments. Pass `name` \
         (e.g. code/read), or `query` (what you want to do) to find one.";
    type Params = CommandsHelpParams;
    type Output = CommandsHelpResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: CommandsHelpParams,
    ) -> Result<CommandsHelpResult, CommandError> {
        let trust = caller_trust(ctx.caller.as_ref());
        // Everything THIS caller could actually run — the universe for both the index
        // and did-you-mean (never leak commands above the caller's access).
        let authorized: Vec<_> = command_registry_live()
            .into_iter()
            .filter(|d| is_command_authorized(d.name, trust))
            .collect();

        // A query → the commands that best match what she wants to do, ranked, one line each.
        if let Some(query) = p.query.as_deref().filter(|q| !q.trim().is_empty()).filter(|_| p.name.is_none()) {
            return Ok(ranked_matches(query, &authorized));
        }

        // No name → the INDEX: every command you can call, one line each. This is the
        // orientation an agent needs FIRST; erroring here (the old "missing field name")
        // dead-ends the very first discovery move.
        let Some(name) = p.name.as_deref().filter(|s| !s.trim().is_empty()) else {
            let mut lines: Vec<String> = authorized
                .iter()
                .map(|d| format!("- {} — {}", d.name, d.description))
                .collect();
            lines.sort();
            let manual = format!(
                "{} commands available to you. Call `commands/help` with a `name` for the \
                 exact tool-call format of any one.\n\n{}",
                lines.len(),
                lines.join("\n"),
            );
            return Ok(CommandsHelpResult {
                name: String::new(),
                description: "index of all callable commands".to_string(),
                access_level: "ai-safe".to_string(),
                manual,
            });
        };

        // She may name it as her tool list offers it (`list_files`) or by its canonical name.
        let canonical = crate::ai::tool_dialect::resolve_wire_name(name);
        let Some(d) = authorized.iter().find(|d| d.name == canonical) else {
            // Unknown/unauthorized name → don't dead-end; suggest the nearest callable
            // commands so a plausible wrong guess still moves the agent forward.
            let names: Vec<&str> = authorized.iter().map(|d| d.name).collect();
            let suggestions = did_you_mean(name, &names);
            let hint = if suggestions.is_empty() {
                "Call `commands/help` with no name for the full index.".to_string()
            } else {
                format!("Did you mean: {}?", suggestions.join(", "))
            };
            return Err(CommandError::NotFound(format!(
                "no command '{name}' available to you (unknown, or above your access). {hint}"
            )));
        };

        Ok(CommandsHelpResult {
            name: d.name.to_string(),
            description: d.description.to_string(),
            access_level: d.access_level.as_str().to_string(),
            manual: render_ai_help(d),
        })
    }
}

crate::register_stateless_command!(CommandsHelp);

/// The authorized commands whose name and description best match `query`, through the same
/// relevance seam the learned tool head reads (`tool_working_set::relevance`).
fn ranked_matches(query: &str, authorized: &[crate::sdk_codegen::CommandDescriptor]) -> CommandsHelpResult {
    use tool_working_set::relevance::{Lexical, Relevance};
    use tool_working_set::vocab::{ToolSchema, Vocabulary};
    let schemas: Vec<ToolSchema> = authorized
        .iter()
        .map(|d| ToolSchema { name: d.name.to_string(), description: d.description.to_string(), tokens: 0 })
        .collect();
    let lines = match Vocabulary::new(schemas) {
        Ok(vocab) => {
            let relevance = Lexical::new(&vocab).relevance(query);
            let mut ranked: Vec<(f32, tool_working_set::ToolId)> =
                vocab.ids().map(|tool| (relevance[tool.index()], tool)).filter(|(score, _)| *score > 0.0).collect();
            ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            // The leading group: every match above the largest drop in relevance, measured on this
            // query's own scores, never a fixed count.
            let gaps = ranked.windows(2).enumerate().map(|(i, pair)| (i + 1, pair[0].0 - pair[1].0));
            let lead = match gaps.max_by(|a, b| a.1.total_cmp(&b.1)) {
                Some((cut, _)) => cut,
                None => ranked.len(),
            };
            ranked
                .into_iter()
                .take(lead)
                .map(|(_, tool)| format!("- {} — {}", vocab.name(tool), vocab.schema(tool).description))
                .collect::<Vec<_>>()
        }
        Err(refused) => vec![format!("(the registry could not be ranked: {refused})")],
    };
    let manual = if lines.is_empty() {
        format!("No command matches \"{query}\". Call `commands/help` with no arguments for the full index.")
    } else {
        format!(
            "The commands that best match \"{query}\". Call `commands/help` with the `name` you choose for its exact call; \
             it is then callable on your next act.\n\n{}",
            lines.join("\n")
        )
    };
    CommandsHelpResult {
        name: String::new(),
        description: format!("commands matching \"{query}\""),
        access_level: "ai-safe".to_string(),
        manual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // what this catches: a mind that does not know a verb's name having no way to find it
    // once her head no longer lists every verb: a query in her words must reach the verb
    // over the LIVE registry, ranked first, in the group that leads by relevance.
    #[test]
    fn a_query_in_her_words_ranks_the_verb_she_means_over_the_live_registry() {
        let registry = command_registry_live();
        let result = ranked_matches("open a pull request on github", &registry);
        let names: Vec<&str> = result.manual.lines().filter_map(|l| l.strip_prefix("- ")).map(|l| l.split(" — ").next().unwrap_or(l)).collect(); // unwrap_or: a line without a description is all name
        assert!(names.first().is_some_and(|n| n.contains("pr-create")), "{}", result.manual);
        assert!(!names.is_empty() && names.len() < registry.len(), "the leading group, not the registry: {}", result.manual);
        let none = ranked_matches("zzqx", &registry);
        assert!(none.manual.starts_with("No command matches"), "{}", none.manual);
    }

    // what this catches: an underscored bare guess getting NO suggestion at all, which
    // is what let a citizen re-issue `list_cards` 31 times (2026-09-05). Neither
    // `commands/list` nor `work/list` shares the category prefix `list_cards` or a whole
    // `/`-segment with it, so before the token tier every candidate was discarded and the
    // refusal named nothing to try. The perception-side loop notice deliberately withholds
    // what to do next, so this function is the only channel that can carry a route.
    #[test]
    fn an_underscored_guess_still_reaches_the_commands_that_share_its_words() {
        let names = ["commands/list", "work/list", "work/claim", "code/read"];
        let hits = did_you_mean("list_cards", &names);
        assert!(
            hits.contains(&"commands/list") && hits.contains(&"work/list"),
            "`list_cards` must reach the /list commands; got {hits:?}"
        );
        assert!(
            !hits.contains(&"code/read"),
            "a candidate sharing no word must not be suggested; got {hits:?}"
        );
    }

    // what this catches: the RIGHT command losing the alphabetical coin-flip. #3769
    // added the token tier so an invented verb gets suggestions instead of silence,
    // and then — measured live on the box that produced the original 31-call loop —
    // `list_cards` returned ai/lora/list, ai/models/list, ai/providers/list,
    // benchmark/list, code/list and one since-deleted verb: six names, none of the two
    // she needed, because twenty commands tie at score 0 and take(6) cuts at `code/`.
    // Suggestions are not a route unless the route survives the window.
    #[test]
    fn the_closest_command_survives_the_window_when_many_share_a_word() {
        let names = [
            "ai/lora/list",
            "ai/models/list",
            "ai/providers/list",
            "benchmark/list",
            "code/list",
            "commands/list",
            "work/list",
        ];
        let hits = did_you_mean("list_cards", &names);
        assert!(
            hits.contains(&"commands/list") && hits.contains(&"work/list"),
            "the commands whose own last word IS `list` must not be sorted out of the window; got {hits:?}"
        );
    }

    // what this catches: the token tier out-ranking the exact-category tier. A same-prefix
    // match is the strongest signal and must stay first, or the useful suggestion drowns in
    // every command that merely shares a common word like `list` or `get`.
    #[test]
    fn same_category_still_outranks_a_merely_shared_word() {
        let names = ["work/list", "work/claim", "commands/list"];
        let hits = did_you_mean("work/lst", &names);
        assert_eq!(
            hits.first(),
            Some(&"work/claim"),
            "same-prefix candidates rank ahead of a shared-word match; got {hits:?}"
        );
    }

    // what this catches: a manual whose example call is not the call that decodes, for ANY
    // registered command (card 1d9c0a64: a `{"tool_call": …}` wrapper she pasted into an
    // argument field). Each example must carry no wrapper, name a tool that resolves back to
    // its own command, and decode as that command's own Params through the check its dispatch
    // runs. Every command, not a sample: a new command's manual is held to it on arrival. A
    // command with no published schema shows no example, and that set only shrinks.
    #[test]
    fn every_registered_manual_example_decodes_as_its_own_command() {
        use crate::ai::tool_dialect::resolve_wire_name;
        // Ratchet: hand-written `CommandSpec`s still on the `Null` schema default (2026-11-07: the
        // four `ai/inference/*` handle verbs and `inference/llm/request`, engine crate).
        // Publish one's schema and lower this; it never rises.
        const UNDECLARED_CEILING: usize = 5;
        let registry = command_registry_live();
        assert!(registry.len() > 50, "the live registry is linked: {}", registry.len());
        let undeclared: Vec<&str> = registry.iter().filter(|d| d.params_schema.is_null()).map(|d| d.name).collect();
        assert!(undeclared.len() <= UNDECLARED_CEILING, "a command added without a params schema: {undeclared:?}");
        let refused: Vec<String> = registry
            .iter()
            .filter_map(|d| {
                let manual = render_ai_help(d);
                let problem = if manual.contains("\"tool_call\"") {
                    "carries a tool_call wrapper".to_owned()
                } else if d.params_schema.is_null() {
                    match crate::sdk_codegen::read_call(&manual) {
                        Err(_) => return None,
                        Ok(_) => "shows an example call for a contract it never published".to_owned(),
                    }
                } else {
                    match crate::sdk_codegen::read_call(&manual) {
                        Err(miss) => miss.to_string(),
                        Ok((name, _)) if resolve_wire_name(&name) != d.name => format!("`{name}` resolves to `{}`", resolve_wire_name(&name)),
                        Ok((_, args)) => match d.arguments.check(&Value::Object(args)) {
                            Ok(()) => return None,
                            Err(refusal) => refusal.to_string(),
                        },
                    }
                };
                Some(format!("{} ({}): {problem}\n{manual}", d.name, d.access_level.as_str()))
            })
            .collect();
        assert!(refused.is_empty(), "{} manuals teach a call that does not decode:\n\n{}", refused.len(), refused.join("\n\n"));
    }

    // what this catches: regression for card 1d9c0a64 on the exact captured case. She sent
    // `bash(cmd: {"tool_call": {"name": "code/list", "arguments": {"path": "."}}})` and
    // `code/read(file_path: {"tool_call": …})` (the M5, 2026-11-07), copying the manual. The
    // manuals of those commands now show her offered call, which `commands/help` also accepts
    // by that offered name.
    #[test]
    fn the_captured_commands_teach_their_offered_call() {
        use crate::ai::tool_dialect::{offered_name_of_stored, resolve_wire_name};
        use crate::sdk_codegen::command_descriptor;
        for canonical in ["code/list", "code/read", "code/shell"] {
            let d = command_descriptor(canonical).expect("registered");
            let manual = render_ai_help(d);
            let offered = offered_name_of_stored(canonical);
            assert!(!manual.contains("tool_call") && !manual.contains("\"arguments\""), "{manual}");
            let (name, args) = crate::sdk_codegen::read_call(&manual).expect("an example call");
            assert_eq!(name, offered, "the example names her offered tool: {manual}");
            assert_eq!(resolve_wire_name(&name), canonical);
            assert!(manual.starts_with(&format!("{offered}({{")) || manual.contains(&format!("\n{offered}({{")), "{manual}");
            d.arguments.check(&Value::Object(args)).unwrap_or_else(|refusal| panic!("{canonical}: {refusal}\n{manual}"));
        }
    }

    // what this catches: a command with no required argument renders its bare call.
    #[test]
    fn no_args_command_renders_cleanly() {
        let d = crate::sdk_codegen::command_descriptor("ping").expect("registered");
        let manual = render_ai_help(d);
        assert!(manual.contains("\nping({})\n"), "only optional arguments: the example is the bare call: {manual}");
    }
}
