#![cfg(test)]
// Declared only under test_env (itself test-only); the attribute also tells the source scans so.
//! The ACL's policy over THIS crate's registered commands (the kernel's own test binary
//! links none of them), so its tests live where the commands do.

use crate::routing::command_acl::*;
use crate::routing::trust_level::TrustLevel;

    // what this catches: an ALIAS of an ai-safe command must authorize at the
    // same tier as its canonical name — the gate checks the raw invoked path,
    // and before this rule persona/list (alias of ai-safe persona/roster)
    // fell to the Owner wildcard, refusing the operator's own desktop a
    // read-only roster. regression for the 2026-08-30 who-panel outage.
    #[test]
    fn an_alias_authorizes_at_its_canonical_commands_tier() {
        let registry = crate::sdk_codegen::ext::command_registry_live();
        let canonical_ai_safe = registry
            .iter()
            .find(|d| {
                d.access_level == crate::sdk_codegen::AccessLevel::AiSafe && !d.aliases.is_empty()
            })
            .expect("at least one aliased ai-safe command exists");
        for alias in canonical_ai_safe.aliases {
            assert!(
                is_command_authorized(alias, TrustLevel::Provisional),
                "alias `{alias}` of ai-safe `{}` must authorize at Provisional",
                canonical_ai_safe.name
            );
        }
    }


    #[test]
    fn test_owner_can_do_anything() {
        // Owner trust level allows any command
        assert!(is_command_authorized("gpu/stats", TrustLevel::Owner));
        assert!(is_command_authorized("genome/train", TrustLevel::Owner));
        assert!(is_command_authorized("screenshot", TrustLevel::Owner));
        assert!(is_command_authorized("data/list", TrustLevel::Owner));
        assert!(is_command_authorized(
            "collaboration/chat/send",
            TrustLevel::Owner
        ));
    }

    #[test]
    fn test_blocked_cannot_do_anything() {
        assert!(!is_command_authorized("gpu/stats", TrustLevel::Blocked));
        assert!(!is_command_authorized("genome/train", TrustLevel::Blocked));
    }

    #[test]
    fn test_sensitive_operations_require_owner() {
        // data/delete specifically requires Owner
        assert!(!is_command_authorized("data/delete", TrustLevel::Trusted));
        assert!(is_command_authorized("data/delete", TrustLevel::Owner));

        // grid/pair requires Owner
        assert!(!is_command_authorized("grid/pair", TrustLevel::Trusted));
        assert!(is_command_authorized("grid/pair", TrustLevel::Owner));

        // grid/grant/issue (minting capability grants) is OWNER-ONLY — load-bearing
        // for the contracted grid: only the local operator may sell its personas'
        // compute. A remote peer must NEVER reach issuance (it would let a grantee
        // mint its own grants). Pins the property the GrantIssuanceModule relies on.
        assert!(!is_command_authorized(
            "grid/grant/issue",
            TrustLevel::Trusted
        ));
        assert!(!is_command_authorized(
            "grid/grant/issue",
            TrustLevel::Provisional
        ));
        assert!(is_command_authorized("grid/grant/issue", TrustLevel::Owner));
    }

    // what this catches: cross-grid inference (ai/generate) is admitted at
    // Provisional+ (so an enrolled non-Owner consumer like IntelMac can request
    // generation) WITHOUT opening everything else — a durable policy decision,
    // not a per-machine manual trust elevation. Owner still works; Blocked is
    // still denied; a sibling sensitive command stays Owner-only.
    #[test]
    fn ai_generate_is_provisional_for_cross_grid_consumers() {
        assert!(is_command_authorized(
            "ai/generate",
            TrustLevel::Provisional
        ));
        assert!(is_command_authorized("ai/generate", TrustLevel::Trusted));
        assert!(is_command_authorized("ai/generate", TrustLevel::Owner));
        assert!(!is_command_authorized("ai/generate", TrustLevel::Blocked));
        // The Provisional rule must NOT leak access to Owner-gated commands. Use a
        // genuinely unclassified op (genome/train → defaults to Owner): a non-AiSafe
        // sibling stays denied. (gpu/stats is itself declared AiSafe, so it IS
        // Provisional-authorized — it is not a valid "should be denied" example.)
        assert!(!is_command_authorized(
            "data/delete",
            TrustLevel::Provisional
        ));
        assert!(!is_command_authorized(
            "genome/train",
            TrustLevel::Provisional
        ));
    }

    // what this catches: THE reconciliation that lets a persona's hands work — a
    // command that declares `AccessLevel::AiSafe` in its own spec is runnable at
    // Provisional (the persona's trust), WITHOUT opening Owner-gated ops. This is
    // why `ping` (the live failure: "no policy granting access") now succeeds for a
    // persona, while `data/delete`/`grid/trust` stay Owner-only.
    #[test]
    fn ai_safe_commands_are_provisional_for_personas() {
        // Declared-AiSafe commands → a Provisional persona may run them.
        assert!(is_command_authorized("ping", TrustLevel::Provisional));
        assert!(is_command_authorized("data/list", TrustLevel::Provisional));
        // Owner-gated sensitive ops stay Owner-only even for a Provisional persona.
        assert!(!is_command_authorized(
            "data/delete",
            TrustLevel::Provisional
        ));
        assert!(!is_command_authorized(
            crate::modules::grid::commands::TRUST,
            TrustLevel::Provisional
        ));
        // Unclassified (not AiSafe, no explicit rule) defaults to Owner → denied.
        assert!(!is_command_authorized(
            "genome/train",
            TrustLevel::Provisional
        ));
        // Blocked is denied even for AiSafe.
        assert!(!is_command_authorized("ping", TrustLevel::Blocked));
    }

    // what this catches: destructive data commands MUST stay Owner-only — never
    // reachable at Provisional (so never over an unauthenticated TCP socket nor by a
    // cross-grid Provisional peer). data/delete + data/update have explicit Owner
    // rules; data/truncate + data/clear-all are unclassified → Owner default-deny.
    // The footgun this defuses (adversarial review 2026-06-21): `ActionCommand`
    // defaults ACCESS to AiSafe, so migrating one of these to a command object and
    // forgetting `const ACCESS = Privileged` would silently make it
    // Provisional-reachable. If that happens, THIS test trips in CI.
    #[test]
    fn destructive_data_commands_stay_owner_only() {
        for cmd in [
            "data/delete",
            "data/update",
            "data/truncate",
            "data/clear-all",
        ] {
            assert!(
                !is_command_authorized(cmd, TrustLevel::Provisional),
                "{cmd} must NOT be reachable at Provisional — it's a destructive, \
                 Owner-only command (a remote/TCP caller must never run it)"
            );
            // Recall capture exposed the gap: Privileged resolves to Trusted,
            // so denying only Provisional did not prove these were Owner-only.
            assert!(
                !is_command_authorized(cmd, TrustLevel::Trusted),
                "{cmd} must not erase shared records as a trusted persona"
            );
            assert!(
                is_command_authorized(cmd, TrustLevel::Owner),
                "{cmd} stays runnable by the local owner"
            );
        }
    }

    // what this catches: irreversible / shared-resource ops must NOT sit on a local
    // persona's (Trusted) tool surface. These were declared `Privileged` → mapped to
    // `Trusted` → silently OFFERED to every local persona (glass-box, 2026-06-29: the
    // catalog held persona/instances/despawn, serving/unload, models/remove, …). A
    // persona must never autonomously delete a model from disk, kill herself or a
    // peer, rewrite a peer's brain, yank the shared inference lane, or seize the GPU
    // budget — the data/delete class. The owner (Owner trust, via cu/grid) keeps them.
    // NOTE: this is the AUTHORIZATION boundary only. Hiding the OTHER ~100 obscure-but-
    // callable verbs from the default catalog is the separate VISIBILITY axis (render
    // layer), NOT this gate — those stay authorized (persona will call them eventually).
    #[test]
    fn destructive_lifecycle_commands_stay_owner_only() {
        for cmd in [
            "persona/instances/despawn",
            "persona/reassign-model",
            "serving/unload",
            "serving/unpin",
            "models/remove",
            "gpu/set-budget",
        ] {
            assert!(
                !is_command_authorized(cmd, TrustLevel::Trusted),
                "{cmd} must NOT be reachable at Trusted — a local persona must never \
                 autonomously run an irreversible / shared-resource op (data/delete class)"
            );
            assert!(
                is_command_authorized(cmd, TrustLevel::Owner),
                "{cmd} stays runnable by the local owner"
            );
        }
        // The deliberately-NOT-locked siblings stay persona-callable (visibility, not
        // authorization): the self-improvement + author-your-own-tools surface.
        for cmd in [
            "command/new",
            "command/migrate",
            "genome/training-trigger/submit",
            "models/pull",
            "serving/load",
        ] {
            assert!(
                is_command_authorized(cmd, TrustLevel::Trusted),
                "{cmd} must STAY persona-callable — it's creative/non-destructive; \
                 declutter it on the visibility axis, never lock it here"
            );
        }
    }

    // what this catches: the L3 self-improvement loop's convert step. The
    // training-completion sentinel dispatches `forge/export` AS the persona
    // (local_persona → Trusted) to turn a freshly-trained MLX adapter into a
    // GGUF-lora. Before this rule, forge/export was unclassified → Owner →
    // DENIED ("substrate refused command `forge/export`: forbidden: no policy
    // grants access to URI: forge/export", glass-box 2026-06-30), silently
    // breaking the train→convert→eval→page-in chain at the convert seam. It must
    // be runnable at Trusted (a local persona converting its own genome) but NOT
    // at Provisional (a remote peer must not spawn a python convert with caller-
    // provided paths). Owner still works. Scoped to the exact command so the
    // network-publishing sibling forge/publish stays Owner-locked.
    #[test]
    fn forge_export_is_trusted_for_the_l3_convert_step() {
        // The local persona (Trusted) running its own genome convert: allowed.
        assert!(is_command_authorized("forge/export", TrustLevel::Trusted));
        assert!(is_command_authorized("forge/export", TrustLevel::Owner));
        // A Provisional remote peer must NOT spawn a python convert here.
        assert!(!is_command_authorized(
            "forge/export",
            TrustLevel::Provisional
        ));
        assert!(!is_command_authorized("forge/export", TrustLevel::Blocked));
        // The scoping is exact: forge/publish (network-publishing) stays Owner-
        // only — the prefix rule must not leak access to other forge/* verbs.
        assert!(!is_command_authorized("forge/publish", TrustLevel::Trusted));
        assert!(!is_command_authorized(
            "forge/publish",
            TrustLevel::Provisional
        ));
    }

    #[test]
    fn test_rules_sorted_by_specificity() {
        let rules = default_rules();
        // Verify longer prefixes come first
        for i in 1..rules.len() {
            assert!(
                rules[i - 1].prefix.len() >= rules[i].prefix.len(),
                "Rule {:?} should come after {:?}",
                rules[i - 1],
                rules[i]
            );
        }
    }

// what this catches: the kernel ACL names the grid's trust verbs by literal (it sits below
// the grid module); if the grid ever renames `grid/pair` or `grid/trust`, the verbs that
// decide who is trusted would fall to the declared-tier rules instead of staying Owner-only.
#[test]
fn the_trust_verbs_stay_owner_only() {
    for verb in [crate::modules::grid::commands::PAIR, crate::modules::grid::commands::TRUST] {
        assert!(!is_command_authorized(verb, TrustLevel::Trusted), "{verb} must need Owner");
        assert!(is_command_authorized(verb, TrustLevel::Owner), "{verb} must allow Owner");
    }
}
