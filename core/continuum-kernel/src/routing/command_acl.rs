//! Command access control: which trust level may invoke which command. A kernel
//! policy (Session) over the command registry's own declarations, read by the socket
//! gate, the grid, a persona's tool surface and an activity's `who_may`.
//!
//! For now: Owner-trust nodes can execute ANY command remotely.
//! Default (untrusted): deny all.
//!
//! This is data-driven via a prefix→access map rather than hardcoded switch statements.
//! When we open to untrusted nodes (Phase 4+), we'll add per-command ACLs via
//! the CommandSchema metadata that modules already provide.

use super::trust_level::TrustLevel;
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

/// The rule table, in priority order (read by the core's policy tests).
pub fn default_rules() -> &'static Vec<AccessRule> {
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
                prefix: "data/truncate",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "data/clear-all",
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "grid/pair", // grid::commands::PAIR; the core's acl tests pin the agreement
                access: CommandAccess::Owner,
            },
            AccessRule {
                prefix: "grid/trust", // grid::commands::TRUST
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
            // Who may do what is the owner's decision (Joel, 2026-09-28): access/set changes
            // the capability threshold and per-citizen decisions; access/get stays AiSafe.
            AccessRule {
                prefix: "access/set",
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
            // A PEER MAY ASK A SEAT FOR A SLOT (card c84d885a, S1b). The reservation ask
            // precedes the cross-grid generate it guards; without this rule every ask fell
            // to the ""=Owner wildcard and the seat refused it — measured 2026-09-19
            // 03:45:24Z on the 5090: routing.acl.refused path=persona/placement/reserve
            // trust=Provisional had_capabilities=false, six asks, zero grants, no node could
            // ever spill onto or return to a seat (fail-closed, correctly). The ask commits
            // one slot for one cooldown, bounded by the seat's own ledger; it mutates no data.
            // Exact verb, not the persona/ namespace: persona/instances/hold and friends stay
            // Owner.
            AccessRule {
                prefix: "persona/placement/reserve",
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
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.prefix.len()));
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
        crate::sdk_codegen::ext::command_registry_live()
            .iter()
            .filter(|d| d.access_level == crate::sdk_codegen::AccessLevel::AiSafe)
            // ALIASES AUTHORIZE TOO: the gate checks the raw invoked path, and
            // an alias is the same command — persona/list (alias of
            // persona/roster) fell through to the Owner wildcard and the
            // operator's own desktop was refused a read-only roster
            // (live-found 2026-08-30). One command, every name it answers to.
            .flat_map(|d| {
                std::iter::once(d.name.to_string()).chain(d.aliases.iter().map(|a| a.to_string()))
            })
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
        crate::sdk_codegen::ext::command_registry_live()
            .iter()
            .filter(|d| d.access_level == crate::sdk_codegen::AccessLevel::Privileged)
            // Same alias rule as ai_safe_commands: one command, every name.
            .flat_map(|d| {
                std::iter::once(d.name.to_string()).chain(d.aliases.iter().map(|a| a.to_string()))
            })
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
