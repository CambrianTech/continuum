//! The contract's ids: UUIDs in their 16 bytes, one type per kind, compared by exact equality only
//! (CONTINUUM-ENGINE 4.11.2). UUIDv7 time order is for reading logs, never for deciding freshness.

use llama::contract::ctm_id;
use uuid::Uuid;

macro_rules! wire_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Uuid);

        impl $name {
            pub fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }
            pub fn uuid(&self) -> Uuid {
                self.0
            }
            /// The frame field's form: the UUID's 16 bytes in RFC 9562 order.
            pub fn to_wire(self) -> ctm_id {
                ctm_id { bytes: *self.0.as_bytes() }
            }
            pub fn from_wire(id: &ctm_id) -> Self {
                Self(Uuid::from_bytes(id.bytes))
            }
        }
    };
}

wire_id!(
    /// The engine process's identity, minted by the engine at start and adopted at attach.
    EngineInstanceId
);
wire_id!(
    /// One act (a turn): minted once per act by the core; a retry keeps it; a learn pin is its act.
    ActId
);
wire_id!(
    /// One LoRA layer of a mind's genome.
    LayerId
);

impl ActId {
    /// A new act's id (UUIDv7).
    pub fn mint() -> Self {
        Self(Uuid::now_v7())
    }
}

impl LayerId {
    /// A new layer's id (UUIDv7).
    pub fn mint() -> Self {
        Self(Uuid::now_v7())
    }
}
