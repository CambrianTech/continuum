//! Grid command access control.
//!
//! For now: Owner-trust nodes can execute ANY command remotely.
//! Default (untrusted): deny all.
//!
//! This is data-driven via a prefix→access map rather than hardcoded switch statements.
//! When we open to untrusted nodes (Phase 4+), we'll add per-command ACLs via
//! the CommandSchema metadata that modules already provide.

use super::commands;
use super::node::TrustLevel;
use std::sync::OnceLock;

/// Minimum trust level required to execute a command remotely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAccess {
    /// Can only run locally — never forwarded from remote nodes.
    LocalOnly,
    /// Requires at least Provisional trust level.
    Provisional,
    /// Requires at least Trusted trust level.
    Trusted,
    /// Requires Owner trust level (full access).
    Owner,
}

/// A rule mapping a command prefix to a required access level.
#[derive(Debug, Clone)]
pub struct AccessRule {
    /// Command prefix to match (e.g., "grid/pair", "gpu/", "*").
    pub prefix: &'static str,
    /// Required access level.
    pub access: CommandAccess,
}

/// The default ACL rules, sorted by specificity (longest prefix first).
/// More specific rules take priority over general ones.
///
/// This is the ONE place where ACL policy is defined.
/// To change what's allowed remotely, edit this list — not scattered match arms.
static DEFAULT_RULES: OnceLock<Vec<AccessRule>> = OnceLock::new();

fn default_rules() -> &'static Vec<AccessRule> {
    DEFAULT_RULES.get_or_init(|| {
        let mut rules = vec![
            // Owner nodes: explicit sensitive operations
            AccessRule {
                prefix: "data/delete",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "data/update",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: commands::PAIR,
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: commands::TRUST,
                access: CommandAccess::Owner,
            },
            // Owner-only DESTRUCTIVE / shared-resource operations.
            //
            // This is the AUTHORIZATION axis (can a caller run it), distinct from the
            // VISIBILITY axis (is it shown in the persona's default tool catalog —
            // handled at the render layer, not here). Most commands are persona-
            // callable by design; the 100+ obscure "internal" verbs are a clutter /
            // discoverability problem (hide from the default surface, keep callable),
            // NOT an authorization problem. So this list stays SMALL and reserved for
            // acts that are irreversible or pull a resource out from under every other
            // persona — the same class as data/delete: a persona must never be able to
            // autonomously delete a model from disk, kill herself or a peer, rewrite a
            // peer's brain, yank the shared inference lane, or seize the GPU budget.
            // The owner, at `Owner` trust via cu/grid, keeps full access (Owner >=
            // every tier). Deliberately NOT locked (persona will/should call these —
            // the self-improvement + author-your-own-tools vision): command/new+migrate,
            // genome/* training, models/pull, serving/load+pin, persona/allocate. Those
            // are creative/non-destructive; they belong on the visibility axis, hidden-
            // not-locked. `AccessLevel` has no `Owner` variant (AiSafe/Privileged/
            // Internal only), so the prefix rule is the canonical way to express Owner-
            // tier, exactly as data/delete does.
            AccessRule {
                prefix: "persona/instances/despawn",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "persona/reassign-model",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "serving/unload",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "serving/unpin",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "models/remove",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "gpu/set-budget",
                access: CommandAccess::Owner,
            },
            // Owner nodes get everything else too (via the wildcard below).
            // When we add untrusted-node support, we'll add Trusted/Provisional rules here.

            // Cross-grid inference: a remote peer may request generation from
            // this node's persona-provider over airc (AircRemoteInferenceAdapter
            // dispatches `ai/generate`, KIND_PEER). Provisional is the durable
            // substrate decision (vs the per-machine `grid/trust <peer> Owner`
            // dance) so every grid consumer — IntelMac, future nodes — is
            // admitted by policy, not a manual elevation. This is the same ACL
            // spot the future capability-discovery handshake will gate. Owner
            // still satisfies it (Owner >= Provisional); Blocked/Untrusted are
            // still denied. More specific than the `""` wildcard, so it wins.
            AccessRule {
                prefix: "ai/generate",
                access: CommandAccess::Provisional,
            },

            // L3 genome convert: the training-completion sentinel converts a
            // persona's freshly-trained MLX adapter → GGUF-lora by dispatching
            // `forge/export` AS that persona (`CallerIdentity::local_persona`,
            // resolves to `Trusted`). This is the local-operator / Privileged
            // class — it spawns a python convert subprocess and writes a new
            // GGUF (non-destructive; deletes nothing) — the same self-improvement
            // class as `genome/training-trigger/submit`, which is already
            // persona-callable at Trusted. A Trusted node (local persona, or a
            // trusted grid node placing forge work per #102) may run it; a
            // Provisional remote peer may NOT (spawning python with caller-
            // provided paths is not for an unelevated peer). `forge/export` is
            // still a Registry-A command (`modules/forge.rs`), so it can't carry
            // an `AccessLevel::Privileged` declaration yet — this explicit prefix
            // rule is the canonical stand-in until the forge module migrates onto
            // the DynCommand registry (consolidation plan), exactly as `ai/generate`
            // above does for cross-grid inference. Scoped to the exact command,
            // NOT the `forge/` prefix: `forge/publish` (uploads to HF — a network-
            // publishing act) stays Owner-locked under the wildcard.
            AccessRule {
                prefix: "forge/export",
                access: CommandAccess::Trusted,
            },

            // Wildcard: owner-trust nodes can run anything.
            // This means our own towers have full access across the grid.
            AccessRule {
                prefix: "",
                access: CommandAccess::Owner,
            },
        ];

        // Sort by prefix length descending (most specific first)
        rules.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));
        rules
    })
}

/// Check whether a command is authorized for a given trust level.
pub fn is_command_authorized(command: &str, trust: TrustLevel) -> bool {
    let required = command_access_level(command);
    match required {
        CommandAccess::LocalOnly => false,
        CommandAccess::Provisional => trust >= TrustLevel::Provisional,
        CommandAccess::Trusted => trust >= TrustLevel::Trusted,
        CommandAccess::Owner => trust >= TrustLevel::Owner,
    }
}

/// The minimum [`TrustLevel`] a caller needs to invoke `command` — the wire-typed
/// projection of the ACL's required tier, for surfacing on an affordance's
/// `who_may` field. `None` means the command is [`CommandAccess::LocalOnly`]: it is
/// never grantable to a non-local caller, so it must not be offered as a remote
/// affordance (a local human/persona may still invoke it directly). This is the
/// ONE place "who may invoke this" is derived for the interface layer — the same
/// resolver [`is_command_authorized`] enforces at the door, never a parallel table.
pub fn required_trust(command: &str) -> Option<TrustLevel> {
    match command_access_level(command) {
        CommandAccess::LocalOnly => None,
        CommandAccess::Provisional => Some(TrustLevel::Provisional),
        CommandAccess::Trusted => Some(TrustLevel::Trusted),
        CommandAccess::Owner => Some(TrustLevel::Owner),
    }
}

/// The set of commands a command declared `AccessLevel::AiSafe` in its own
/// CommandSpec — the single source of truth for "safe for an autonomous AI
/// caller." Collected ONCE from the (static, inventory-built) command registry.
/// This is the reconciliation the `AccessLevel` placeholder always called for:
/// the command's OWN declaration drives authorization, not a parallel allow-list.
fn ai_safe_commands() -> &'static std::collections::HashSet<String> {
    static AI_SAFE: OnceLock<std::collections::HashSet<String>> = OnceLock::new();
    AI_SAFE.get_or_init(|| {
        crate::sdk_codegen::command_registry()
            .iter()
            .filter(|d| d.access_level == crate::sdk_codegen::AccessLevel::AiSafe)
            .map(|d| d.name.to_string())
            .collect()
    })
}

/// The set of commands that declared `AccessLevel::Privileged` in their own
/// CommandSpec — the "local-operator" tier: powerful tools (shell, git push,
/// arbitrary file write) safe for a HIGH-trust local citizen (a local persona =
/// `Trusted`, the owner = `Owner`) but NOT for a remote `Provisional` peer. Same
/// single-source mechanism as [`ai_safe_commands`]: the command's own declaration
/// drives authorization, never a parallel allow-list.
fn privileged_commands() -> &'static std::collections::HashSet<String> {
    static PRIVILEGED: OnceLock<std::collections::HashSet<String>> = OnceLock::new();
    PRIVILEGED.get_or_init(|| {
        crate::sdk_codegen::command_registry()
            .iter()
            .filter(|d| d.access_level == crate::sdk_codegen::AccessLevel::Privileged)
            .map(|d| d.name.to_string())
            .collect()
    })
}

/// Determine the access level for a command.
///
/// Order (security-significant):
/// 1. Explicit grid rules EXCEPT the `""` wildcard — sensitive ops a persona must
///    NEVER do remotely (`data/delete`, `data/update`, `grid/pair`, `grid/trust`)
///    and `ai/generate` (Provisional). Most-specific-prefix wins.
/// 2. The command's OWN declared `AccessLevel::AiSafe` → `Provisional` (a persona's
///    curated, safe-for-AI tool surface — what makes its hands actually work).
/// 3. The command's OWN declared `AccessLevel::Privileged` → `Trusted` (the
///    local-operator tier — shell/git/write: a Trusted local persona or a Trusted
///    grid node may run it; a Provisional remote peer may not).
/// 4. Default → `Owner` (the `""` wildcard): unknown/unclassified = locked down.
fn command_access_level(command: &str) -> CommandAccess {
    for rule in default_rules() {
        if rule.prefix.is_empty() {
            continue; // the wildcard is the LAST resort, after the declared-tier checks
        }
        if command.starts_with(rule.prefix) {
            return rule.access;
        }
    }
    // The command's declared destiny: AiSafe = safe for an AI caller → Provisional.
    if ai_safe_commands().contains(command) {
        return CommandAccess::Provisional;
    }
    // Privileged = local-operator tier → Trusted (local persona / trusted node).
    if privileged_commands().contains(command) {
        return CommandAccess::Trusted;
    }
    // Unclassified → Owner (the `""` wildcard's level): default-deny for personas.
    CommandAccess::Owner
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!is_command_authorized("grid/grant/issue", TrustLevel::Trusted));
        assert!(!is_command_authorized("grid/grant/issue", TrustLevel::Provisional));
        assert!(is_command_authorized("grid/grant/issue", TrustLevel::Owner));
    }

    // what this catches: cross-grid inference (ai/generate) is admitted at
    // Provisional+ (so an enrolled non-Owner consumer like IntelMac can request
    // generation) WITHOUT opening everything else — a durable policy decision,
    // not a per-machine manual trust elevation. Owner still works; Blocked is
    // still denied; a sibling sensitive command stays Owner-only.
    #[test]
    fn ai_generate_is_provisional_for_cross_grid_consumers() {
        assert!(is_command_authorized("ai/generate", TrustLevel::Provisional));
        assert!(is_command_authorized("ai/generate", TrustLevel::Trusted));
        assert!(is_command_authorized("ai/generate", TrustLevel::Owner));
        assert!(!is_command_authorized("ai/generate", TrustLevel::Blocked));
        // The Provisional rule must NOT leak access to Owner-gated commands. Use a
        // genuinely unclassified op (genome/train → defaults to Owner): a non-AiSafe
        // sibling stays denied. (gpu/stats is itself declared AiSafe, so it IS
        // Provisional-authorized — it is not a valid "should be denied" example.)
        assert!(!is_command_authorized("data/delete", TrustLevel::Provisional));
        assert!(!is_command_authorized("genome/train", TrustLevel::Provisional));
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
        assert!(!is_command_authorized("data/delete", TrustLevel::Provisional));
        assert!(!is_command_authorized(commands::TRUST, TrustLevel::Provisional));
        // Unclassified (not AiSafe, no explicit rule) defaults to Owner → denied.
        assert!(!is_command_authorized("genome/train", TrustLevel::Provisional));
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
        for cmd in ["data/delete", "data/update", "data/truncate", "data/clear-all"] {
            assert!(
                !is_command_authorized(cmd, TrustLevel::Provisional),
                "{cmd} must NOT be reachable at Provisional — it's a destructive, \
                 Owner-only command (a remote/TCP caller must never run it)"
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
        assert!(!is_command_authorized("forge/export", TrustLevel::Provisional));
        assert!(!is_command_authorized("forge/export", TrustLevel::Blocked));
        // The scoping is exact: forge/publish (network-publishing) stays Owner-
        // only — the prefix rule must not leak access to other forge/* verbs.
        assert!(!is_command_authorized("forge/publish", TrustLevel::Trusted));
        assert!(!is_command_authorized("forge/publish", TrustLevel::Provisional));
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
}
