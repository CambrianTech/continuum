//! `Verdict` — the typed result of the substrate's auth gate.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` (Slice P, RBAC
//! section): every external command crossing the substrate boundary
//! passes through ONE chokepoint where the typed gate evaluates
//! `policy(caller_peer_id, uri) -> Verdict`. There is no parallel
//! per-endpoint guard surface to drift; new commands inherit coverage
//! the moment their URIs are registered.
//!
//! ## Variants
//!
//! - [`Verdict::Allowed`] — caller may execute; dispatcher proceeds
//!   immediately
//! - [`Verdict::Forbidden`] — typed refusal with [`ForbiddenReason`]
//!   carrying the actionable cause (unknown peer, no permission for
//!   URI, admission denied, revoked). Never silent permission grant
//!   per [[no-fallbacks-ever]]; the substrate refuses loudly with the
//!   reason
//! - [`Verdict::Deferred`] — mediation required (e.g. "ask Maya's
//!   primary env before evicting her LoRA"). Carries the prompt target
//!   env. The dispatcher routes a consent prompt; the original URI
//!   proceeds when consent is granted
//!
//! ## Why typed, not boolean
//!
//! A boolean `allowed: bool` gate loses the WHY. The operator
//! debugging a denied request can't tell whether the caller's peer
//! is unknown, the URI is unmatched, the rate limit is exceeded, or
//! the target peer asked the gate to defer to consent. Typed
//! variants surface the reason at the gate; the audit log captures
//! it; the user-facing error message is actionable. Same compression
//! principle the rest of the substrate uses: one typed result, all
//! consumers exhaustively match.

use crate::routing::EnvironmentId;

/// Typed result of `policy(caller_peer_id, uri) -> Verdict`. Every
/// substrate-boundary dispatch consults this gate; the dispatcher
/// MUST match exhaustively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Caller may execute. Dispatcher proceeds.
    Allowed,
    /// Typed refusal. Dispatcher returns the error to caller and
    /// records the verdict in the audit log.
    Forbidden { reason: ForbiddenReason },
    /// Mediation required. Dispatcher routes a consent prompt to
    /// `prompt_target_env` on the target's substrate; the original
    /// URI proceeds iff consent is granted.
    Deferred {
        reason: DeferredReason,
        prompt_target_env: EnvironmentId,
    },
}

impl Verdict {
    /// `true` iff this verdict allows immediate execution.
    /// Convenience accessor for the dispatcher's hot path.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Verdict::Allowed)
    }

    /// Short kind label for log lines + audit rows.
    pub fn kind(&self) -> &'static str {
        match self {
            Verdict::Allowed => "allowed",
            Verdict::Forbidden { .. } => "forbidden",
            Verdict::Deferred { .. } => "deferred",
        }
    }
}

/// Why the gate refused this dispatch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ForbiddenReason {
    /// Caller's peer_id is not enrolled in this substrate's trust
    /// store. Cross-grid stranger making an anonymous request — see
    /// the design doc's "Cross-grid hostile-traffic story."
    #[error("caller peer not enrolled in this substrate")]
    UnknownPeer,

    /// Caller is enrolled but no policy row grants them access to
    /// the requested URI pattern. The doc-string carries the URI
    /// the request was made against so the audit log is actionable.
    #[error("no policy grants access to URI: {0}")]
    NoPermissionForUri(String),

    /// Admission system (PressureBroker, rate limiter, etc.)
    /// declined the dispatch even though policy would have allowed
    /// it. Typically a temporary condition.
    #[error("admission denied: {0}")]
    AdmissionDenied(String),

    /// Caller had access; the grant was revoked. Distinguishes
    /// "never had permission" from "permission was withdrawn" for
    /// the audit log.
    #[error("caller's permission for this URI was revoked")]
    Revoked,
}

/// Why the gate deferred this dispatch to a consent prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeferredReason {
    /// Target peer's policy says "ask the target's primary env
    /// before this operation runs." Typical for operations that
    /// alter persona state (genome paging, LoRA eviction,
    /// cognition-scope mutation).
    AskTargetEnv,

    /// Sentinel quorum required. Dispatch is queued until N
    /// sentinels sign off.
    SentinelQuorum { required: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_verdict_is_allowed() {
        assert!(Verdict::Allowed.is_allowed());
        assert_eq!(Verdict::Allowed.kind(), "allowed");
    }

    #[test]
    fn forbidden_verdicts_are_not_allowed() {
        let cases = [
            ForbiddenReason::UnknownPeer,
            ForbiddenReason::NoPermissionForUri("airc://maya/cognition/genome/lora-evict".into()),
            ForbiddenReason::AdmissionDenied("rate limit exceeded".into()),
            ForbiddenReason::Revoked,
        ];
        for reason in cases {
            let v = Verdict::Forbidden { reason };
            assert!(!v.is_allowed());
            assert_eq!(v.kind(), "forbidden");
        }
    }

    #[test]
    fn deferred_verdict_is_not_allowed() {
        let v = Verdict::Deferred {
            reason: DeferredReason::AskTargetEnv,
            prompt_target_env: EnvironmentId::Named("web".into()),
        };
        assert!(!v.is_allowed());
        assert_eq!(v.kind(), "deferred");
    }

    /// `ForbiddenReason::NoPermissionForUri` carries the URI in the
    /// display string — actionable for the operator looking at the
    /// audit log without having to cross-reference the surrounding
    /// context.
    #[test]
    fn no_permission_for_uri_includes_uri_in_display() {
        let r =
            ForbiddenReason::NoPermissionForUri("airc://maya/cognition/genome/lora-evict".into());
        let display = format!("{r}");
        assert!(display.contains("airc://maya/cognition/genome/lora-evict"));
    }

    #[test]
    fn admission_denied_includes_cause() {
        let r = ForbiddenReason::AdmissionDenied("rate limit exceeded: 100 req/min".into());
        let display = format!("{r}");
        assert!(display.contains("rate limit exceeded"));
    }
}
