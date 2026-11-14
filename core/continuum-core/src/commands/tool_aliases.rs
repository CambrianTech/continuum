//! The tool dialect's contract against the LIVE command registry: every command is offered by
//! ONE name derived from its path (`wire_name`: `work/release` → `work_release`), and the aliases
//! commands declare (`ALIASES`, `action_command!` `aliases:`) aggregate into the inbound index the
//! engine's `ai::tool_dialect` reads. These tests live here because the registrations do; the
//! engine's own tests cannot see commands it does not link.

#[cfg(test)]
mod tests {
    use crate::ai::tool_dialect::*;
    use crate::ai::types::{NativeToolSpec, ToolCall, ToolInputSchema};
    use crate::sdk_codegen::ext::command_registry_live;

    fn call(name: &str, input: serde_json::Value) -> ToolCall {
        ToolCall { id: "t1".into(), name: name.into(), input }
    }

    fn spec(n: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: n.to_string(),
            description: String::new(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: THE live #326 defect. Before the adapter handled this,
    // `work/list(state=open)` classified as a MISS, passed through untouched, and was
    // dispatched as a URI — "forbidden: no policy grants access to URI:
    // work/list(state=open)". Two citizens read that as "no open tasks".
    // what this catches: a `tools/` namespace in front of a real command reaching
    // the ACL as a literal URI (peer b6dcfc8e, 2026-10-04: refused on every act).
    // It resolves to the command; an unknown remainder stays a miss.
    #[test]
    fn a_tools_namespace_prefix_resolves_to_the_command() {
        let mut c = call("tools/work/list", serde_json::json!({"state": "open"}));
        assert_eq!(normalize_call(&mut c), None, "a clean name with a prefix needs no repair note");
        assert_eq!(c.name, "work/list");
        assert_eq!(resolve_wire_name("tools/not/a/command"), "tools/not/a/command");
    }

    // what this catches: an offered name or an alias that ALSO carries args — both halves of
    // the adapter have to compose, or `work_list(state=open)` still dies.
    #[test]
    fn a_name_carrying_arguments_resolves_through_both_halves() {
        for (name, canonical, arg) in [("work_list(state=open)", "work/list", "state"), ("read_file(file_path=a.rs)", "code/read", "file_path")] {
            let mut c = call(name, serde_json::json!({}));
            normalize_call(&mut c);
            assert_eq!(c.name, canonical, "{name}");
            assert!(c.input.get(arg).is_some(), "{name} keeps its argument: {}", c.input);
        }
    }

    // what this catches: two naming schemes in her tool list (card d2b6dbce). Live, the M5
    // 2026-11-14 04:55Z: Aiko was offered `claim_task`, `release_task`, `list_tasks` (aliases
    // renaming the offer) beside `work_create`, `work_submit` (paths); she reasoned in `work_*`,
    // found no `work_release`, and called `work_create` again. Every registered command is
    // offered as `wire_name` of its path, and that name resolves back to it, whatever any
    // command declares as an alias.
    #[test]
    fn every_registered_command_is_offered_by_its_path_and_resolves_back() {
        let registry = command_registry_live();
        assert!(registry.iter().any(|d| d.name == "work/release"), "the live registry is linked");
        for d in &registry {
            let offered = to_wire_spec(spec(d.name)).name;
            assert_eq!(offered, d.name.replace('/', "_"), "{} is offered by its path", d.name);
            assert_eq!(offered, wire_name(d.name), "{}", d.name);
            assert_eq!(resolve_wire_name(&offered), d.name, "the name offered for {} resolves to it", d.name);
        }
    }

    // what this catches: a family split across schemes. Every command under one namespace
    // (`work/*`, `code/git/*`) is offered with that namespace's prefix, so a verb she infers
    // from its siblings is spelled the way it is offered; and the verb Aiko looked for exists.
    #[test]
    fn no_family_mixes_naming_schemes_and_work_release_exists() {
        use std::collections::BTreeMap;
        let mut families: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for d in command_registry_live() {
            if let Some((family, _verb)) = d.name.rsplit_once('/') {
                families.entry(family).or_default().push(to_wire_spec(spec(d.name)).name);
            }
        }
        for (family, offered) in &families {
            let prefix = format!("{}_", family.replace('/', "_"));
            let strays: Vec<&String> = offered.iter().filter(|name| !name.starts_with(&prefix)).collect();
            assert!(strays.is_empty(), "family {family} offers {strays:?} outside `{prefix}*`");
        }
        let Some(work) = families.get("work") else { panic!("the work family is registered") };
        for verb in ["work_claim", "work_create", "work_release", "work_review", "work_submit", "work_list", "work_get"] {
            assert!(work.iter().any(|name| name == verb), "the work family offers {verb}: {work:?}");
        }
        assert_eq!(resolve_wire_name("work_release"), "work/release");
    }

    // what this catches: an alias that takes another command's offered name, or a work verb
    // still answering to a retired per-verb name that no family member shares.
    #[test]
    fn aliases_never_shadow_an_offered_name_and_the_work_family_has_none() {
        let registry = command_registry_live();
        let offered: std::collections::HashMap<String, &str> = registry.iter().map(|d| (wire_name(d.name), d.name)).collect();
        for d in &registry {
            for alias in d.aliases {
                if let Some(other) = offered.get(*alias) {
                    assert_eq!(*other, d.name, "{}'s alias `{alias}` is the offered name of {other}", d.name);
                }
            }
            if d.name.starts_with("work/") {
                assert!(d.aliases.is_empty(), "{} declares per-verb aliases {:?}", d.name, d.aliases);
            }
        }
        for retired in ["claim_task", "release_task", "list_tasks", "get_task", "update_task", "heartbeat_task"] {
            assert_eq!(resolve_wire_name(retired), retired, "`{retired}` no longer names a work verb");
        }
    }

    // what this catches: the socket/CLI resolver (used at Runtime::route_command and
    // the `uu` entry) maps IDENTICALLY to the persona path — alias, canonical
    // (idempotent), charset-legal, unknown-passthrough — so `uu` / IPC / MCP accept
    // the same vocabulary a persona does. Non-recording is a structural guarantee
    // (resolve_wire_name == classify().0, and classify never calls record), so it's
    // asserted by construction, not by racy global-tally inspection.
    #[test]
    fn resolve_wire_name_maps_like_the_persona_path() {
        assert_eq!(resolve_wire_name("read_file"), "code/read"); // trained alias
        assert_eq!(resolve_wire_name("code/read"), "code/read"); // canonical, idempotent
        assert_eq!(resolve_wire_name("code_read"), "code/read"); // charset-legal
        assert_eq!(resolve_wire_name("frobnicate"), "frobnicate"); // unknown passes through
    }

    // what this catches: the index is NON-VACUOUS — trained reflexes (`read_file`, `bash`), the
    // real need the alias scheme served, still resolve inbound. A regression that dropped the
    // ALIASES consts (or the descriptor plumbing) would empty this and silently stop all reflex
    // mapping. They are answered, never offered: `code/read` is offered as `code_read`.
    #[test]
    fn the_alias_index_is_populated() {
        assert!(
            alias_to_command().len() >= 10,
            "expected the hot-verb aliases from the migrated commands, got {}",
            alias_to_command().len()
        );
        // Spot-check the highest-frequency live reflexes (mined 2026-07-19).
        assert_eq!(from_wire_name("file_tree"), "code/tree");
        assert_eq!(from_wire_name("read_file"), "code/read");
        assert_eq!(from_wire_name("bash"), "code/shell");
        assert_eq!(to_wire_spec(spec("code/read")).name, "code_read");
        // Git reflexes declared via the action_command! macro's `aliases:` clause
        // (#202 Slice 6) — a model reaching for the industry-standard `git_status`
        // resolves to our `code/git/status`. If the macro's aliases plumbing breaks,
        // these are the canary.
        assert_eq!(from_wire_name("git_status"), "code/git/status");
        assert_eq!(from_wire_name("git_commit"), "code/git/commit");
        assert_eq!(from_wire_name("git_diff"), "code/git/diff");
    }
}
