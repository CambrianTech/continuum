//! Dispatch branches on these outcomes, never on diagnostic text. Successful completion
//! means the sealed save/erase proof was already accepted by the grant owner.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReclaimBusy {
    #[error("the resident or one of her forks is acting")]
    Mind,
    #[error("the physical slot is in use")]
    Slot,
}

#[derive(Debug, thiserror::Error)]
pub enum NotReclaimable {
    #[error("handoff requires an engine incarnation")]
    MissingEngine,
    #[error("invalid handoff endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("handoff endpoint port does not match the leased engine")]
    WrongPort,
    #[error("handoff endpoint incarnation changed")]
    ChangedEngine,
    #[error("handoff endpoint has no owned pool")]
    MissingPool,
    #[error("handoff requires a resident identity")]
    MissingIdentity,
    #[error("resident-only reclamation cannot free fork pages")]
    ForkPages,
    #[error("handoff has no attributed resident slot")]
    MissingSlot,
    #[error("handoff slot attribution changed before physical custody")]
    ChangedSlot,
    #[error("endpoint admission refused: {0}")]
    Endpoint(String),
    #[error("page IO admission refused: {0}")]
    PageIo(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ReclaimError {
    #[error("holder is busy: {0}")]
    Busy(ReclaimBusy),
    #[error("holder cannot be reclaimed: {0}")]
    NotReclaimable(#[from] NotReclaimable),
    #[error("engine refused to save the holder's page")]
    SaveRefused,
    #[error("engine refused to erase the saved page")]
    EraseRefused,
    #[error("physical reclamation is unproved; capacity remains charged: {0}")]
    Uncertain(String),
    #[error("grant owner refused publication: {0}")]
    Publication(String),
}
