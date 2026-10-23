//! A requester that abandons a remote generation tells the seat to stop (card 83f774fb).
//!
//! IntelMac, 2026-10-23: Iris gave up on the M5 at her deadline and the M5 kept going,
//! finishing 2,520 tokens ~55 s later for no one and holding a lane the whole time. The
//! seat (`ipc::command_ingress`) aborts a served generation on `ai/generate/cancel` from
//! its requester; this guard sends that cancel on every path that abandons the wait.

use std::sync::Arc;

use airc_core::{Body, MentionTarget, PeerId};
use airc_lib::Airc;
use continuum_airc_protocol::{
    AircCommandRequest, GenerateCancel, COMMAND_PATH_AI_GENERATE_CANCEL, DEFAULT_COMMAND_DEADLINE,
    KIND_PEER,
};
use uuid::Uuid;

use crate::capacity::gossip::{
    capability_decision, CapabilityDecision, CapabilityId, CapacityOffer,
};
use crate::routing::airc_transport::AircTransport;

/// PURE: may a cancel be sent to this seat? Only when its fresh beacon affirmatively
/// advertises [`CapabilityId::RemoteGenerateCancel`]; no beacon, or one from a core that
/// predates the command, means it would be ignored, so it is never sent (#3941 doctrine).
pub(super) fn seat_accepts_cancel(offer: Option<&CapacityOffer>) -> bool {
    offer.is_some_and(|offer| {
        capability_decision(offer, CapabilityId::RemoteGenerateCancel) == CapabilityDecision::Allow
    })
}

/// Held while a remote generation is awaited. Dropped while armed (a deadline, an error,
/// or the turn dropping its future) it sends the seat a cancel for this correlation;
/// [`AbandonGuard::disarm`] once a reply arrived, because the seat is then already done.
pub(super) struct AbandonGuard {
    armed: Option<Cancel>,
}

struct Cancel {
    airc: Arc<Airc>,
    room: airc_lib::Room,
    seat: PeerId,
    correlation: Uuid,
}

impl AbandonGuard {
    /// Armed only for a seat that accepts the cancel ([`seat_accepts_cancel`]).
    pub(super) fn new(
        airc: Arc<Airc>,
        room: airc_lib::Room,
        seat: PeerId,
        correlation: Uuid,
        seat_accepts: bool,
    ) -> Self {
        Self {
            armed: seat_accepts.then_some(Cancel {
                airc,
                room,
                seat,
                correlation,
            }),
        }
    }

    /// A reply arrived: nothing to cancel.
    pub(super) fn disarm(mut self) {
        self.armed = None;
    }
}

impl Drop for AbandonGuard {
    fn drop(&mut self) {
        let Some(cancel) = self.armed.take() else {
            return;
        };
        // Drop cannot await: the cancel rides its own task. Outside a runtime (process
        // teardown) there is nothing to send it on; the seat's own deadline still ends it.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(cancel.send());
            }
            Err(_) => continuum_base::probe!(
                class = "remote_lane.cancel_unsent",
                peer = %cancel.seat.0,
                correlation = %cancel.correlation,
                "abandoned a remote generation outside a runtime; the seat was not told"
            ),
        }
    }
}

impl Cancel {
    async fn send(self) {
        let request = AircCommandRequest {
            path: COMMAND_PATH_AI_GENERATE_CANCEL.to_string(),
            kind: KIND_PEER.to_string(),
            env: None,
            params: serde_json::json!(GenerateCancel {
                correlation: self.correlation
            }),
        };
        let headers = AircTransport::build_headers(&request);
        let sent = match serde_json::to_value(&request) { // boundary: the airc wire, this request's body
            Ok(body) => self
                .airc
                .request_in(
                    &self.room,
                    MentionTarget::Peer(self.seat),
                    headers,
                    Body::Json(body),
                    DEFAULT_COMMAND_DEADLINE,
                )
                .await
                .map(|_pending| ())
                .map_err(|e| e.to_string()),
            Err(e) => Err(format!("serialize cancel: {e}")),
        };
        // The seat's reply (cancelled / not_serving / not_yours) is its own receipt; the
        // requester has already given up and does not wait for it.
        match sent {
            Ok(()) => continuum_base::probe!(
                class = "remote_lane.cancel_sent",
                peer = %self.seat.0,
                correlation = %self.correlation,
                "abandoned a remote generation; told the seat to stop"
            ),
            Err(error) => continuum_base::probe!(
                class = "remote_lane.cancel_unsent",
                peer = %self.seat.0,
                correlation = %self.correlation,
                error = %error,
                "abandoned a remote generation; the cancel could not be sent"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beacon(capabilities: serde_json::Value) -> CapacityOffer {
        serde_json::from_value(serde_json::json!({
            "gpuTotalBytes": 1u64, "gpuFreeBytesLive": 0u64, "systemRamFreeBytes": 0u64, "atMs": 0u64,
            "capabilities": capabilities,
        }))
        .expect("the wire shape of a seat's beacon")
    }

    // what this catches: a cancel goes only to a seat that advertises it. No beacon, an
    // older core's beacon with no capability, or another capability never earns one; a
    // seat that never learned ai/generate/cancel must not be sent it.
    #[test]
    fn a_cancel_is_sent_only_to_a_seat_that_advertises_it() {
        assert!(!seat_accepts_cancel(None), "no beacon, no cancel");
        assert!(
            !seat_accepts_cancel(Some(&beacon(serde_json::json!([])))),
            "an older core"
        );
        assert!(
            !seat_accepts_cancel(Some(&beacon(serde_json::json!([
                "placement_reservation_grant"
            ])))),
            "another capability does not stand in"
        );
        assert!(seat_accepts_cancel(Some(&beacon(serde_json::json!([
            "remote_generate_cancel"
        ])))));
    }
}
