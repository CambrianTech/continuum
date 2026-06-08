//! Honest startup contract — one line per load-bearing subsystem.
//!
//! Joel `[[reliable-startup-substrate-refuses-to-lie]]` (card
//! `e9f50a36`): the substrate's job at boot is to TELL the operator
//! exactly which subsystems are live, which are degraded, and which
//! are failed. Silent boot lines that "look fine" but are actually
//! masking a missing dependency / unreachable daemon / wrong path
//! are the kind of lie this module exists to refuse.
//!
//! ## The shape
//!
//! Every load-bearing subsystem emits exactly ONE line on stderr at
//! boot via [`boot_status`]:
//!
//! ```text
//! [continuum-core-server] <subsystem>: <icon> <detail>
//! ```
//!
//! - **icon** = `✓` / `⚠` / `✗` for [`BootStatusKind::Ok`] /
//!   [`Degraded`](BootStatusKind::Degraded) /
//!   [`Failed`](BootStatusKind::Failed).
//! - **subsystem** = kebab-case identifier matching the URI / module
//!   name (`probes`, `logs`, `boot-mode`, `airc`, `personas`,
//!   `adapter`, `model`, etc).
//! - **detail** = one operator-actionable line. Path, version,
//!   count, or the exact remediation command. NO multi-line dumps,
//!   NO stack traces (those go to probes / the rolling log).
//!
//! Lines go to **stderr** so they appear regardless of `RUST_LOG`'s
//! current filter — the operator who just set `RUST_LOG=warn` to
//! quiet noise during a load test still gets the boot health
//! summary. The fmt layer's rolling-log sink (PR #1547) also
//! captures these lines, so future replay / postmortem reads them
//! back without needing the live console.
//!
//! ## Why a helper instead of bare `eprintln!`
//!
//! - **Consistent shape** — operators / sentinels / log scrapers can
//!   grep `[continuum-core-server] .*: ✗` for failures across every
//!   subsystem without per-module formatting quirks.
//! - **Observability seam** — see "Probe emission" below. Each call
//!   also fires a `tracing` event so the substrate's own
//!   [`debug/probes/*`](super::super::routing) URIs can subscribe to
//!   boot health (sentinel agents, dashboards, the `boot/status`
//!   service module). The Display sink prints the line; the probe
//!   sink lands the structured record. Same call, two consumers,
//!   per [`OBSERVABILITY-AS-SUBSTRATE.md`].
//! - **Test surface** — [`format_boot_status_line`] is pure, so the
//!   format is unit-testable without spinning up the binary.
//!
//! ## Probe emission
//!
//! Every [`boot_status`] call ALSO fires a substrate-native
//! [`crate::probe!`](probe) event with `class = "boot.status"` and
//! three fields: `subsystem`, `kind`, `detail`. The `probe!` macro
//! expands to a `tracing::event!` with `probe_class` set as a
//! FIELD (not a tracing `target`) — that's the load-bearing detail
//! that makes the event reachable by the substrate's structured-
//! record consumers:
//!
//! - `JsonlProbeFileSink` filters per-event on the `probe_class`
//!   field, so setting `CONTINUUM_PROBE_CLASSES=boot.status`
//!   captures every boot line into the operator's chosen JSONL
//!   path.
//! - `ProbeRouterLayer` fans events per-class to broadcast
//!   subscribers, which the `debug/probes/{open,next,close}`
//!   service-module URIs surface to substrate consumers (sentinel
//!   agents, dashboards, the `boot/status` future module).
//!
//! Using a plain `tracing::info!(target: "boot.status", ...)`
//! would write to the rolling-log file (the fmt layer's sink) but
//! silently skip BOTH structured-record paths above — PR #1550
//! reviewer round 1 caught exactly that mistake. The boot line
//! exists to be subscribed to, not just to be read by humans;
//! routing through `probe!` is the seam that makes that subscription
//! actually work.
//!
//! ## What this is NOT
//!
//! - Not a logging facade for runtime events. Use `tracing::info!`
//!   directly with module-appropriate targets for those.
//! - Not a health-check endpoint. That belongs in `boot/status`
//!   service module (future card) which reads these lines back from
//!   the probe stream and exposes a typed result.
//! - Not for messages an operator doesn't need to see at boot. If a
//!   subsystem is best-effort and its failure is captured in a
//!   probe stream, prefer the probe — don't burn a boot line on it.
//!
//! [`OBSERVABILITY-AS-SUBSTRATE.md`]: ../../docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md

use std::fmt;

/// Health state of a subsystem at boot. Kept small and totally
/// ordered so a sentinel reading a probe stream can compute "worst
/// kind across all subsystems" trivially.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BootStatusKind {
    /// Subsystem is fully live. Operator action: none.
    Ok,
    /// Subsystem is alive but in a reduced-capability mode (e.g.
    /// adapter fell back to CPU, model loaded but quantized, airc
    /// reachable but only via env-var override). Operator action:
    /// see the detail line for what's reduced and what fixes it.
    Degraded,
    /// Subsystem failed to start. Operator action: the detail line
    /// MUST name the exact remediation (path to create, env var to
    /// set, command to run). Per `[[no-fallbacks-ever]]` failure is
    /// LOUD, not silent.
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

    /// Stable lowercase tag for structured outputs (probe records,
    /// `boot/status` projections). Used as the `kind` field on the
    /// emitted `tracing` event.
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

/// Process-wide stderr prefix on every boot line. Hard-coded so log
/// scrapers / sentinels can grep one literal across every subsystem.
const PREFIX: &str = "[continuum-core-server]";

/// Pure formatting half — produces the exact text written to stderr,
/// without touching IO or `tracing`. Kept separate so unit tests
/// pin the shape without coordinating on `std::io::stderr`.
///
/// Format: `[continuum-core-server] <subsystem>: <icon> <detail>`
/// (newline appended by the caller; this function returns the body
/// only so the test can `assert_eq` against a known string).
pub fn format_boot_status_line(subsystem: &str, kind: BootStatusKind, detail: &str) -> String {
    format!("{PREFIX} {subsystem}: {icon} {detail}", icon = kind.icon())
}

/// Emit a boot status line for `subsystem` with `detail` to stderr
/// AND as a substrate-native `probe!` event with
/// `class = "boot.status"`. Always returns; callers use the return
/// `kind` as the "did boot succeed" signal for the subsystem.
///
/// Call this ONCE per subsystem at boot — not as a logging
/// primitive. See the module-level doc for the "what this is not"
/// list.
pub fn boot_status(subsystem: &str, kind: BootStatusKind, detail: &str) {
    let line = format_boot_status_line(subsystem, kind, detail);
    // Use stderr directly: the rolling-log sink (PR #1547) writes
    // to the file via the tracing fmt layer; eprintln goes to the
    // live stderr stream (terminal / supervisor pipe). Two channels
    // give the operator both the live readout AND the persisted
    // record. eprintln also survives `RUST_LOG=warn` quieting,
    // which is the whole point of a boot-health channel.
    eprintln!("{line}");
    // Structured probe emission via the substrate's `probe!` macro
    // (defined in `routing/macros.rs`). The macro expands to
    // `tracing::event!(Level::INFO, probe_class = "boot.status", ...)`.
    //
    // The `probe_class` FIELD (not the tracing `target`) is what
    // `JsonlProbeFileSink::on_event` filters on per
    // `probe_file_sink.rs::class_passes_filter` — so this is the
    // load-bearing detail that makes the boot.status events
    // actually reachable by the JSONL sink when
    // `CONTINUUM_PROBE_CLASSES=boot.status` is set, AND by
    // `ProbeRouterLayer`'s per-class broadcast that the
    // `debug/probes/{open,next,close}` URIs surface to substrate
    // consumers (sentinel agents, dashboards). PR #1550 reviewer
    // round 1 caught the prior `tracing::info!(target: ...)`
    // approach silently skipping both consumers — the file got
    // the line via the fmt layer's rolling sink, but the
    // structured-record channels were a no-op. Routing through
    // `probe!` fixes that without changing the operator-facing
    // stderr line.
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

    #[test]
    fn ok_line_uses_check_icon_and_canonical_prefix() {
        let line = format_boot_status_line("probes", BootStatusKind::Ok, "landing at /tmp/p.jsonl");
        assert_eq!(line, "[continuum-core-server] probes: ✓ landing at /tmp/p.jsonl");
    }

    #[test]
    fn degraded_line_uses_warn_icon() {
        let line = format_boot_status_line(
            "airc",
            BootStatusKind::Degraded,
            "reachable via AIRC_DAEMON_SOCKET override (task #79 pending)",
        );
        assert_eq!(
            line,
            "[continuum-core-server] airc: ⚠ reachable via AIRC_DAEMON_SOCKET override (task #79 pending)"
        );
    }

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

    #[test]
    fn kind_tags_are_stable_lowercase() {
        assert_eq!(BootStatusKind::Ok.tag(), "ok");
        assert_eq!(BootStatusKind::Degraded.tag(), "degraded");
        assert_eq!(BootStatusKind::Failed.tag(), "failed");
    }

    /// The total ordering exists so a sentinel computing "worst kind
    /// across boot lines" can use `.max()`. Pins it so a future
    /// refactor can't silently reorder the variants and invert the
    /// sentinel's "is anything failing" check.
    #[test]
    fn kind_ordering_is_ok_lt_degraded_lt_failed() {
        assert!(BootStatusKind::Ok < BootStatusKind::Degraded);
        assert!(BootStatusKind::Degraded < BootStatusKind::Failed);
    }

    #[test]
    fn kind_display_matches_tag() {
        assert_eq!(BootStatusKind::Ok.to_string(), "ok");
        assert_eq!(BootStatusKind::Degraded.to_string(), "degraded");
        assert_eq!(BootStatusKind::Failed.to_string(), "failed");
    }
}
