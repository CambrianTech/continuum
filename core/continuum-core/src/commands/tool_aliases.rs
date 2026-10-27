//! The tool dialect's contract against the LIVE command registry: the aliases commands
//! declare (`ALIASES`, `action_command!` `aliases:`) aggregate into the index the engine's
//! `ai::tool_dialect` reads. These tests live here because the registrations do; the
//! engine's own tests cannot see commands it does not link.

#[cfg(test)]
mod tests {
    use crate::ai::tool_dialect::*;
    use crate::ai::types::{NativeToolSpec, ToolCall, ToolInputSchema};

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

    // what this catches: an alias that ALSO carries args — both halves of the adapter
    // have to compose, or `list_tasks(state=open)` still dies.
    #[test]
    fn an_alias_carrying_arguments_resolves_through_both_halves() {
        let mut c = call("list_tasks(state=open)", serde_json::json!({}));
        normalize_call(&mut c);
        assert_eq!(c.name, "work/list");
        assert_eq!(c.input.get("state").and_then(|v| v.as_str()), Some("open"));
    }

    // what this catches: the per-command declarations aggregate into a working
    // round-trip over the LIVE registry under BOTH offer styles — a hot verb offers
    // under its trained reflex OR our charset-legal canonical, and EITHER offered
    // form maps back to the same canonical command. This is the decentralized
    // replacement for the old global DIALECT table; if a command's ALIASES stops
    // being read, or the round-trip stops being style-symmetric, these break.
    #[test]
    fn per_command_aliases_round_trip_under_both_offer_styles() {
        for (canonical, reflex) in [
            ("code/shell", "bash"),
            ("code/read", "read_file"),
            ("code/write", "write_file"),
            ("code/run", "run_code"),
            ("code/list", "list_files"),
            ("work/claim", "claim_task"),
            ("work/list", "list_tasks"),
            ("work/get", "get_task"),
        ] {
            // TrainedReflex offers the alias; it maps back to canonical.
            assert_eq!(
                to_wire_spec_with(spec(canonical), OfferStyle::TrainedReflex).name,
                reflex,
                "TrainedReflex offers {canonical} as {reflex}"
            );
            assert_eq!(
                from_wire_name(reflex),
                canonical,
                "map {reflex} back to {canonical}"
            );
            // Canonical offers OUR name charset-legal; it maps back to canonical too.
            let canon_wire = canonical.replace('/', "_");
            assert_eq!(
                to_wire_spec_with(spec(canonical), OfferStyle::Canonical).name,
                canon_wire,
                "Canonical offers {canonical} as {canon_wire}"
            );
            assert_eq!(
                from_wire_name(&canon_wire),
                canonical,
                "map {canon_wire} back to {canonical}"
            );
        }
        // A canonical name a model emits directly still resolves (never narrows).
        assert_eq!(from_wire_name("code/read"), "code/read");
        // An unknown name passes through — the executor fails it loud with a
        // did-you-mean; the adapter never invents a route.
        assert_eq!(from_wire_name("frobnicate"), "frobnicate");
    }

    // what this catches: the DEFAULT offer policy MEETS THEIR TRAINING — a hot verb is
    // offered under the model's trained reflex (`read_file`), NOT our canonical
    // `code_read`. Canonical is a hypothesis not yet validated by a tool-using A/B
    // (#204): the only offer-surface gate we can run (humaneval-rs) is pure codegen and
    // offers no tools, so it can't measure the change. If someone flips the default to
    // Canonical without that validation, this fails loudly — the guard against a blind
    // flip of the tuned surface. [[tool-naming-meet-their-training-alias-or-redirect]]
    #[test]
    fn default_offer_policy_meets_their_training() {
        assert_eq!(offer_style_for(None), OfferStyle::TrainedReflex);
        assert_eq!(to_wire_spec(spec("code/read")).name, "read_file");
        assert_eq!(to_wire_spec(spec("work/claim")).name, "claim_task");
        // Canonical is still REACHABLE as an explicit policy (the seam is intact) —
        // it's just not the blind default.
        assert_eq!(
            to_wire_spec_with(spec("code/read"), OfferStyle::Canonical).name,
            "code_read"
        );
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

    // what this catches: the index is NON-VACUOUS — the migration actually landed
    // aliases on commands. A regression that dropped the ALIASES consts (or the
    // descriptor plumbing) would empty this and silently stop all reflex mapping.
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
        // Git reflexes declared via the action_command! macro's `aliases:` clause
        // (#202 Slice 6) — a model reaching for the industry-standard `git_status`
        // resolves to our `code/git/status`. If the macro's aliases plumbing breaks,
        // these are the canary.
        assert_eq!(from_wire_name("git_status"), "code/git/status");
        assert_eq!(from_wire_name("git_commit"), "code/git/commit");
        assert_eq!(from_wire_name("git_diff"), "code/git/diff");
    }
}
