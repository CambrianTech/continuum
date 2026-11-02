//! Why a server has no KV slot pool for a request (card 394017ed, f3ba3b56 item 5).
//!
//! Discovery is required before a llama-server request can enter admission. A miss
//! refuses generation rather than letting the engine select an unowned slot.

/// The cause, one variant per way discovery can come back without a pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlotDiscoveryMiss {
    /// An earlier probe proved `/props` absent; affinity stays off for this server.
    LatchedUnsupported,
    /// `/props` answered a status that proves the endpoint does not exist (latched now).
    EndpointAbsent { status: u16 },
    /// The request to `/props` failed in transport (engine restarting, socket refused).
    Transport { error: String },
    /// `/props` answered a transient status, such as 503 while the model loads.
    NotReady { status: u16 },
    /// `/props` answered 200 with a body that did not parse.
    BodyUnparsed,
    /// The body carried no `total_slots`.
    NoGeometry,
    /// The engine was replaced while the probe was in flight.
    Superseded,
    /// The pool already installed for this server disagrees with what `/props` reports.
    GeometryMismatch { installed: u32, props: u32 },
    /// The page-admission negotiation refused this pool.
    PageAdmissionRefused,
}

impl SlotDiscoveryMiss {
    /// Probe field value for the cause.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::LatchedUnsupported => "latched_unsupported",
            Self::EndpointAbsent { .. } => "endpoint_absent",
            Self::Transport { .. } => "transport",
            Self::NotReady { .. } => "not_ready",
            Self::BodyUnparsed => "body_unparsed",
            Self::NoGeometry => "no_geometry",
            Self::Superseded => "superseded",
            Self::GeometryMismatch { .. } => "geometry_mismatch",
            Self::PageAdmissionRefused => "page_admission_refused",
        }
    }

    /// Say the miss once, where it is decided. A latched server was announced when it
    /// latched (`EndpointAbsent`), so it is not repeated on every request after.
    pub(crate) fn probe(&self, root: &str) {
        let detail = match self {
            Self::EndpointAbsent { status } | Self::NotReady { status } => status.to_string(),
            Self::Transport { error } => error.clone(),
            Self::GeometryMismatch { installed, props } => format!("installed {installed} slots, props {props}"),
            _ => String::new(),
        };
        match self {
            Self::LatchedUnsupported => {}
            Self::EndpointAbsent { .. } => continuum_base::probe!(
                class = "inference.slot_affinity.unsupported",
                root = root,
                cause = self.as_str(),
                detail = %detail,
                "props endpoint absent — generation refused; this server cannot supply slot admission"
            ),
            _ => continuum_base::probe!(
                class = "inference.slot_affinity.deferred",
                root = root,
                cause = self.as_str(),
                detail = %detail,
                "no slot pool for this request — generation refused; re-probed on the next request"
            ),
        }
    }
}
