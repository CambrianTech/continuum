//! Physical custody for a grant transfer. Save is not reclamation: only a validated erase
//! receipt can produce ReclaimedGrant. Caller cancellation does not cancel endpoint-owned IO.
use super::{directory, page_io, EndpointAdmission, KvSlotPool, PageKey, SlotPermit, SlotPin};
use crate::capacity::window_grant::{GrantHandoff, GrantLease};
use crate::inference::turn_admission::{kv_page_action, PageAction, PageOutcome, PageResolution};
use std::sync::Arc;
mod outcome;
pub use outcome::{NotReclaimable, ReclaimBusy, ReclaimError};

/// Constructed only after a confirmed save followed by the engine's erase receipt.
pub(crate) struct ReclaimedGrant(GrantHandoff);
impl ReclaimedGrant {
    pub(crate) fn into_ticket(self) -> GrantHandoff { self.0 }
}

/// The dyno's scripted actuator delivers its physical receipts through these same sealed proofs.
/// The HTTP save-then-erase ordering that constructs them in production is pinned by this
/// module's tests; a virtual-clock scenario scripts only when, and whether, a receipt arrives.
#[cfg(test)]
impl ReclaimedGrant {
    pub(crate) fn scripted(ticket: GrantHandoff) -> Self { Self(ticket) }
}
#[cfg(test)]
impl RefusedGrant {
    pub(crate) fn scripted(ticket: GrantHandoff) -> Self { Self(ticket) }
}

/// No save/erase was submitted, or the engine positively refused the action. This returns
/// the original charged claim to its holder; it is never a reclamation proof.
pub(crate) struct RefusedGrant(GrantHandoff);
impl RefusedGrant {
    pub(crate) fn into_ticket(self) -> GrantHandoff { self.0 }
}

struct Custody {
    endpoint: EndpointAdmission,
    _mind: tokio::sync::OwnedRwLockWriteGuard<()>,
    _permit: SlotPermit,
    _pin: Option<SlotPin>,
    _page_io: crate::runtime::admission_gate::Permit<'static>,
    ticket: Option<GrantHandoff>,
    pending: bool,
}
impl Drop for Custody {
    fn drop(&mut self) {
        if self.pending { self.endpoint.quarantine_paging(); }
    }
}

/// Starts one retained operation. Dropping this receiver never releases the slot or abandons
/// its IO; successful proof is recorded with the grant owner before notifying the caller.
/// A known refusal restores the charged holder at publication; uncertainty keeps it fenced.
pub fn reclaim_grant(
    client: reqwest::Client,
    root: String,
    ticket: GrantHandoff,
) -> tokio::sync::oneshot::Receiver<Result<(), ReclaimError>> {
    let endpoint = directory().endpoint(&root);
    let (tx, rx) = tokio::sync::oneshot::channel();
    endpoint.retain_paging(async move {
        let persona = ticket.persona();
        let result = match prepare(&client, &root, ticket.persona(), ticket.lease()).await {
            Ok((mut custody, pool, slot, key)) => {
                custody.ticket = Some(ticket);
                reclaim(&client, &root, pool, slot, key, custody).await
                    .and_then(|proof| crate::capacity::window_grant::record_reclaimed_grant(proof)
                        .map_err(ReclaimError::Publication))
            }
            Err(reason) => return_refused(ticket, reason),
        };
        continuum_base::probe!(class = "inference.grant_handoff.finished", persona = %persona,
            reclaimed = result.is_ok(), error = ?result.as_ref().err(),
            "retained grant handoff finished; reservation owner keeps the publication authority");
        let _ = tx.send(result); // owner records the outcome even if caller leaves during preparation
    });
    rx
}

fn return_refused<T>(ticket: GrantHandoff, reason: ReclaimError) -> Result<T, ReclaimError> {
    crate::capacity::window_grant::record_refused_grant(RefusedGrant(ticket))
        .map_err(ReclaimError::Publication)?;
    Err(reason)
}

async fn prepare(client: &reqwest::Client, root: &str, persona: uuid::Uuid, lease: GrantLease)
    -> Result<(Custody, Arc<KvSlotPool>, u32, PageKey), ReclaimError> {
    let engine = lease.engine.ok_or(NotReclaimable::MissingEngine)?;
    let url = reqwest::Url::parse(&root).map_err(|e| NotReclaimable::InvalidEndpoint(e.to_string()))?;
    if url.port_or_known_default() != Some(engine.port) {
        return Err(NotReclaimable::WrongPort.into());
    }
    let mind = super::MindTurn::try_reclaim(persona)
        .ok_or(ReclaimError::Busy(ReclaimBusy::Mind))?;
    let endpoint = directory().endpoint(&root);
    let admission = endpoint.admit().await.map_err(NotReclaimable::Endpoint)?;
    if admission.incarnation() != Some((engine.pid, engine.started_s)) {
        return Err(NotReclaimable::ChangedEngine.into());
    }
    let pool = admission.pool.clone().ok_or(NotReclaimable::MissingPool)?;
    directory().discover_page_admission(&client, &root).await.map_err(NotReclaimable::Endpoint)?;
    let key = PageKey::mind(persona).ok_or(NotReclaimable::MissingIdentity)?;
    if pool.holders.lock().values().any(|resident| resident.persona() == persona && *resident != key) {
        return Err(NotReclaimable::ForkPages.into());
    }
    let slot = pool.holders.lock().iter().find_map(|(slot, resident)| (*resident == key).then_some(*slot))
        .ok_or(NotReclaimable::MissingSlot)?;
    let pin = pool.pin(&key);
    let permit = pool.try_acquire_slot(slot).ok_or(ReclaimError::Busy(ReclaimBusy::Slot))?;
    admission.check_ready().map_err(NotReclaimable::Endpoint)?;
    if pool.holders.lock().get(&slot) != Some(&key) {
        return Err(NotReclaimable::ChangedSlot.into());
    }
    let page_io = super::admit_page_io(page_io::gate()).map_err(|e| NotReclaimable::PageIo(e.to_string()))?;
    let custody = Custody { endpoint: admission, _mind: mind, _permit: permit, _pin: pin, _page_io: page_io, ticket: None, pending: false };
    Ok((custody, pool, slot, key))
}

async fn reclaim(
    client: &reqwest::Client, root: &str, pool: Arc<KvSlotPool>, slot: u32,
    key: PageKey, mut custody: Custody,
) -> Result<ReclaimedGrant, ReclaimError> {
    // An interrupted overwrite must not leave an older save marked valid.
    pool.note_page_lost(&key);
    custody.pending = true;
    let (returned, saved) = resolve(kv_page_action(client, root, slot, &key, PageAction::Save).await, custody).await?;
    custody = returned;
    if !saved {
        let ticket = custody.ticket.take().ok_or_else(|| ReclaimError::Uncertain("handoff ticket already consumed".into()))?;
        return return_refused(ticket, ReclaimError::SaveRefused);
    }
    pool.note_saved(key);
    // No checkpoint or returner may call this erased/uncertain slot warm. The confirmed file
    // remains valid throughout erase and is never overwritten by a later empty-slot save.
    pool.forget_resident(slot, key);
    pool.note_overwritten(slot);
    custody.pending = true;
    let (mut custody, erased) = resolve(kv_page_action(client, root, slot, &key, PageAction::Erase).await, custody).await?;
    if !erased {
        let ticket = custody.ticket.take().ok_or_else(|| ReclaimError::Uncertain("handoff ticket already consumed".into()))?;
        return return_refused(ticket, ReclaimError::EraseRefused);
    }
    let ticket = custody.ticket.take().ok_or_else(|| ReclaimError::Uncertain("handoff ticket already consumed".into()))?;
    Ok(ReclaimedGrant(ticket))
}

async fn resolve(outcome: PageOutcome, mut custody: Custody) -> Result<(Custody, bool), ReclaimError> {
    match outcome {
        PageOutcome::Completed => { custody.pending = false; Ok((custody, true)) }
        PageOutcome::Rejected => { custody.pending = false; Ok((custody, false)) }
        PageOutcome::Uncertain => Err(ReclaimError::Uncertain("page operation has no verified receipt".into())),
        PageOutcome::Unresolved(operation) => {
            custody.pending = false; // retained page owner now owns quarantine and custody
            match operation.resume(custody).await {
                Ok(PageResolution::Completed(custody)) => Ok((custody, true)),
                Ok(PageResolution::Rejected(custody)) => Ok((custody, false)),
                Ok(PageResolution::Fenced) => Err(ReclaimError::Uncertain("page operation fenced".into())),
                Err(_) => Err(ReclaimError::Uncertain("page continuation lost".into())),
            }
        }
    }
}

#[cfg(test)]
mod tests;

/// Shared HTTP fixture exercises production preparation and IO, without a process-global
/// grant owner that would make unrelated parallel tests fence one another's reservations.
#[cfg(test)]
pub(crate) async fn fixture_for_publication() -> (String, Arc<KvSlotPool>, PageKey, tokio::task::JoinHandle<()>) {
    let (root, _, pool, key, custody, server) = tests::fixture(false).await;
    drop(custody);
    pool.mark_incarnation((7, 123));
    (root, pool, key, server)
}

#[cfg(test)]
pub(crate) async fn reclaim_for_publication(root: &str, ticket: GrantHandoff) -> Result<ReclaimedGrant, String> {
    let client = reqwest::Client::new();
    let (mut custody, pool, slot, key) = prepare(&client, root, ticket.persona(), ticket.lease()).await.map_err(|e| e.to_string())?;
    custody.ticket = Some(ticket);
    reclaim(&client, root, pool, slot, key, custody).await.map_err(|e| e.to_string())
}
