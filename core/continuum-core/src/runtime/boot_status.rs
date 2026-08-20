//! Honest startup contract — one line per load-bearing subsystem.
//!
//! Joel `[[reliable-startup-substrate-refuses-to-lie]]` (card `e9f50a36`):
//! the substrate's job at boot is to TELL the operator exactly which
//! subsystems are live, degraded, or failed. Silent boot lines that "look
//! fine" while masking a missing dependency / unreachable daemon / wrong
//! path are the lie this module refuses.
//!
//! ## Shape
//!
//! Every load-bearing subsystem emits exactly ONE line on stderr at boot
//! via [`boot_status`]:
//!
//! ```text
//! [continuum-core-server] <subsystem>: <icon> <detail>
//! ```
//!
//! - **icon** = `✓` / `⚠` / `✗` for [`BootStatusKind::Ok`] / `Degraded` / `Failed`.
//! - **subsystem** = kebab-case identifier (`airc`, `personas`, `adapter`, …).
//! - **detail** = one operator-actionable line (path, count, or the exact
//!   remediation command). No multi-line dumps / stack traces (those go to probes).
//!
//! Lines go to **stderr** so they survive `RUST_LOG` filtering, and are also
//! captured by the fmt layer's rolling-log sink.
//!
//! ## Probe emission (load-bearing)
//!
//! Every [`boot_status`] call ALSO fires a substrate-native [`crate::probe!`]
//! event with `class = "boot.status"` (fields `subsystem`/`kind`/`detail`).
//! The `probe!` macro sets `probe_class` as a FIELD (not a tracing `target`),
//! which is what `JsonlProbeFileSink` filters on (`CONTINUUM_PROBE_CLASSES=
//! boot.status`) and what `ProbeRouterLayer` fans to `debug/probes/*`
//! subscribers (sentinels, dashboards, the future `boot/status` module). A
//! plain `tracing::info!(target: "boot.status", …)` would write the rolling
//! log but silently skip BOTH structured-record paths — PR #1550 reviewer
//! round 1 caught exactly that. The boot line exists to be subscribed to, not
//! just read; routing through `probe!` is the seam that makes that work.

use std::fmt;

/// Boot health of one subsystem. Ordered `Ok < Degraded < Failed` so a
/// sentinel can compute "worst kind across all subsystems" via `.max()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BootStatusKind {
    /// Fully live. Operator action: none.
    Ok,
    /// Alive but reduced-capability (CPU fallback, quantized model, airc via
    /// env override, …). Operator action: see the detail line.
    Degraded,
    /// Failed to start. Per `[[no-fallbacks-ever]]` this is LOUD, and the
    /// detail line MUST name the exact remediation.
    Failed,
}

impl BootStatusKind {
    /// Stable single-character icon for the prefix.
    pub fn icon(self) -> &'static str {
        match self {
            BootStatusKind::Ok => "✓",
            BootStatusKind::Degraded => "⚠",
            BootStatusKind::Failed => "✗",
        }
    }

    /// Stable lowercase tag for structured outputs (the `kind` probe field).
    pub fn tag(self) -> &'static str {
        match self {
            BootStatusKind::Ok => "ok",
            BootStatusKind::Degraded => "degraded",
            BootStatusKind::Failed => "failed",
        }
    }
}

impl fmt::Display for BootStatusKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Process-wide stderr prefix on every boot line. Hard-coded so log scrapers
/// / sentinels can grep one literal across every subsystem.
const PREFIX: &str = "[continuum-core-server]";

/// Pure formatting half — produces the exact stderr text without touching IO
/// or `tracing`, so unit tests pin the shape. Newline is appended by the caller.
///
/// Format: `[continuum-core-server] <subsystem>: <icon> <detail>`
pub fn format_boot_status_line(subsystem: &str, kind: BootStatusKind, detail: &str) -> String {
    format!("{PREFIX} {subsystem}: {icon} {detail}", icon = kind.icon())
}

/// Emit a boot status line for `subsystem` to stderr AND as a substrate-native
/// `probe!` event with `class = "boot.status"`. Call ONCE per subsystem at
/// boot (not as a logging primitive).
pub fn boot_status(subsystem: &str, kind: BootStatusKind, detail: &str) {
    let line = format_boot_status_line(subsystem, kind, detail);
    // stderr: live readout that survives `RUST_LOG=warn`. The rolling-log sink
    // captures it too via the tracing fmt layer.
    eprintln!("{line}");
    // Structured emission via `probe!` (routing/macros.rs). The `probe_class`
    // FIELD is what JsonlProbeFileSink + ProbeRouterLayer key on — see the
    // module doc. A `tracing::info!(target: …)` would skip both consumers.
    crate::probe!(
        class = "boot.status",
        subsystem = subsystem,
        kind = kind.tag(),
        detail = detail,
        "boot status reported",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: the canonical line shape (prefix + check icon) so
    /// log scrapers / sentinels can grep one literal across subsystems.
    #[test]
    fn ok_line_uses_check_icon_and_canonical_prefix() {
        let line = format_boot_status_line("probes", BootStatusKind::Ok, "landing at /tmp/p.jsonl");
        assert_eq!(
            line,
            "[continuum-core-server] probes: ✓ landing at /tmp/p.jsonl"
        );
    }

    /// What this catches: degraded uses the warn icon (operator distinguishes
    /// "reduced" from "failed" at a glance).
    #[test]
    fn degraded_line_uses_warn_icon() {
        let line = format_boot_status_line(
            "airc",
            BootStatusKind::Degraded,
            "reachable via AIRC_DAEMON_SOCKET override",
        );
        assert_eq!(
            line,
            "[continuum-core-server] airc: ⚠ reachable via AIRC_DAEMON_SOCKET override"
        );
    }

    /// What this catches: failed uses the cross icon (the grep target for
    /// "what's broken at boot").
    #[test]
    fn failed_line_uses_cross_icon() {
        let line = format_boot_status_line(
            "model",
            BootStatusKind::Failed,
            "missing — download Qwen2.5-0.5B-Instruct.gguf to models/",
        );
        assert_eq!(
            line,
            "[continuum-core-server] model: ✗ missing — download Qwen2.5-0.5B-Instruct.gguf to models/"
        );
    }

    /// What this catches: stable lowercase tags — they're the `kind` probe
    /// field + structured-output value; a rename would break scrapers.
    #[test]
    fn kind_tags_are_stable_lowercase() {
        assert_eq!(BootStatusKind::Ok.tag(), "ok");
        assert_eq!(BootStatusKind::Degraded.tag(), "degraded");
        assert_eq!(BootStatusKind::Failed.tag(), "failed");
    }

    /// What this catches: `Ok < Degraded < Failed` so a sentinel's
    /// `.max()` "is anything failing" check can't be silently inverted by a
    /// future variant reorder.
    #[test]
    fn kind_ordering_is_ok_lt_degraded_lt_failed() {
        assert!(BootStatusKind::Ok < BootStatusKind::Degraded);
        assert!(BootStatusKind::Degraded < BootStatusKind::Failed);
    }

    /// What this catches: Display == tag (structured outputs that format the
    /// kind get the same stable string as the probe field).
    #[test]
    fn kind_display_matches_tag() {
        assert_eq!(BootStatusKind::Ok.to_string(), "ok");
        assert_eq!(BootStatusKind::Degraded.to_string(), "degraded");
        assert_eq!(BootStatusKind::Failed.to_string(), "failed");
    }
}
