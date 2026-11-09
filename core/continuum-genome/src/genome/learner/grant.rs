//! The grant a mind's learning steps are admitted under (card 4114a2c6).
//!
//! The core issues each grant and registers it with the engine before any step names it; a step
//! carries only the id, so no request authorizes its own share. A grant never changes: a new share is
//! a new id, and the old one is revoked. Before anything has been measured the share is
//! [`GrantShare::UncontendedOnly`]: learning runs only while no other slot serves, and every
//! contended step is refused, so nothing unmeasured can slow a resident. A measured share replaces it
//! once the engine has reported a learning chunk's time and the neighbours' latency under it.

use super::seat::{Seat, SeatRefusal};
use crate::inference::llama_server::{GrantId, GrantShare, GrantTerms, LedgerAnswer};

/// A grant registered with a seat's engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub id: GrantId,
    pub share: GrantShare,
}

/// Register a new grant of `share` with the seat's engine, expiring after `expires_in` unless
/// revoked first (the engine's own maximum, as it reports it, is the backstop the caller passes).
pub async fn issue(seat: &Seat, share: GrantShare, expires_in: std::time::Duration) -> Result<Grant, SeatRefusal> {
    let terms = GrantTerms { id: GrantId::issue(), engine: seat.ledger.clone(), share, expires_in };
    match seat.engine.register_learn_grant(&terms).await {
        Ok(LedgerAnswer::Ledger(registered)) => match registered.refused {
            None => Ok(Grant { id: terms.id, share }),
            Some(why) => Err(SeatRefusal::Unleased(format!("the engine refused the grant: {why:?}"))),
        },
        Ok(LedgerAnswer::NoLedger) => Err(SeatRefusal::NoLedger),
        Err(error) => Err(SeatRefusal::Unleased(format!("the grant was not registered: {error}"))),
    }
}

/// Replace `old` with a grant of `share`: the new one is registered first, then the old revoked, so
/// her learning is never without a grant and never under two.
pub async fn replace(seat: &Seat, old: &Grant, share: GrantShare, expires_in: std::time::Duration) -> Result<Grant, SeatRefusal> {
    let new = issue(seat, share, expires_in).await?;
    revoke(seat, old).await?;
    Ok(new)
}

/// Revoke `grant`: no new step is admitted under it; a step already started drains to restoration.
pub async fn revoke(seat: &Seat, grant: &Grant) -> Result<(), SeatRefusal> {
    match seat.engine.revoke_learn_grant(&grant.id.to_string()).await {
        Ok(LedgerAnswer::Ledger(_)) => Ok(()),
        Ok(LedgerAnswer::NoLedger) => Err(SeatRefusal::NoLedger),
        Err(error) => Err(SeatRefusal::Unleased(format!("the grant was not revoked: {error}"))),
    }
}
