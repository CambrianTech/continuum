//! `resources/board` — the machine's live resource-accounting board.
//!
//! Dep-holding: captures the one per-machine [`ResourceDaemon`](crate::resources::ResourceDaemon)
//! — the single authority over VRAM/RAM/disk/ports (#56). The daemon's background
//! poll refreshes each consumer's self-declared footprint every tick; this command
//! snapshots the resulting [`LeaseBoard`](crate::resources::LeaseBoard): per-kind
//! ledgers (capacity / granted / available), the live leases, and — the reason #79
//! needed a read surface — the per-consumer MEASURED attributions.
//!
//! ## Why this is the reporting half of #79
//!
//! Serving registers as a *measured* consumer (monitor-not-reserve): the daemon
//! attributes its resident VRAM without leasing it, so `available = capacity − granted`
//! stays honest while `attributions` carries the physical truth. That truth was only
//! ever emitted to the `resource_drift` observability probe — invisible to an operator,
//! a persona, or a grid peer. This command makes the board queryable so the drift
//! *reporting* is actually readable: `granted:0` yet multi-GB `attributions` for serving
//! is exactly the picture that proves the un-inversion is working.
//!
//! ## Gating
//!
//! `AiSafe` — a pure read of the accounting snapshot. No lease is granted, no reclaim
//! is fired, no capacity is set; a persona may legitimately inspect what the machine
//! is holding (grid self-awareness) the same way it may inspect memory pressure.

use std::sync::Arc;

use crate::resources::{LeaseBoard, ResourceDaemon};

use super::ResourcesQuery;

crate::action_command! {
    /// Snapshot the machine's resource-accounting board: per-kind capacity, granted,
    /// and available bytes; the live leases; and each consumer's measured residency
    /// (what it physically holds right now, independent of what it leased). A pure
    /// read — observes the board without granting, reclaiming, or setting capacity.
    pub struct ResourcesBoard { daemon: Arc<ResourceDaemon> }
    name: "resources/board",
    access: AiSafe,
    params: ResourcesQuery,
    output: LeaseBoard,
    run(this, _ctx, _p) => {
        Ok(this.daemon.board())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{DaemonConfig, ResourceDaemon};
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    fn command() -> ResourcesBoard {
        ResourcesBoard {
            daemon: ResourceDaemon::start(Vec::new(), Vec::new(), DaemonConfig::default()),
        }
    }

    // what this catches: name/access wiring — the board is a read-only accounting
    // snapshot, so it belongs on the AiSafe surface (a persona may inspect what the
    // machine holds). A drift to Privileged/Owner here would silently hide grid
    // self-awareness from the persona tool surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ResourcesBoard::NAME, "resources/board");
        assert!(matches!(ResourcesBoard::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: the command returns the daemon's own LeaseBoard shape — the
    // three axes (kinds / leases / attributions) that #79's measured-not-reserved model
    // reports through. A serde-shape drift here would feed the TS mixin / grid peer a
    // board missing the attribution axis (the honest physical-residency truth).
    #[tokio::test]
    async fn returns_board_with_the_three_axes() {
        let out = command()
            .run(&Ctx::default(), ResourcesQuery {})
            .await
            .expect("board snapshot never errors");
        let json = serde_json::to_value(&out).unwrap();
        assert!(json["kinds"].is_array(), "kinds axis missing");
        assert!(json["leases"].is_array(), "leases axis missing");
        assert!(
            json["attributions"].is_array(),
            "attributions (measured-residency) axis missing"
        );
    }
}
