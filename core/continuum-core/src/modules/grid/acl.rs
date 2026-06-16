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

/// Determine the access level for a command using the rule table.
/// First matching rule wins (rules are sorted by specificity).
fn command_access_level(command: &str) -> CommandAccess {
    for rule in default_rules() {
        if rule.prefix.is_empty() || command.starts_with(rule.prefix) {
            return rule.access;
        }
    }
    // Should never reach here since "" prefix matches everything
    CommandAccess::LocalOnly
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
        // The Provisional rule must NOT leak access to other commands.
        assert!(!is_command_authorized("data/delete", TrustLevel::Provisional));
        assert!(!is_command_authorized("gpu/stats", TrustLevel::Provisional));
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
