//! Physical custody for a grant transfer. Save is not reclamation: only a validated erase
//! receipt can produce ReclaimedGrant. Caller cancellation does not cancel endpoint-owned IO.
use super::{directory, page_io, EndpointAdmission, KvSlotPool, PageKey, SlotPermit, SlotPin};
use crate::capacity::window_grant::GrantHandoff;
use crate::inference::turn_admission::{kv_page_action, PageOutcome};
use std::sync::Arc;

/// Constructed only after a confirmed save followed by the engine's erase receipt.
pub(crate) struct ReclaimedGrant(GrantHandoff);
impl ReclaimedGrant {
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
/// A failure leaves retirement charged and the old holder fenced from new dispatch.
pub async fn reclaim_grant(
    client: reqwest::Client,
    root: String,
    ticket: GrantHandoff,
) -> Result<tokio::sync::oneshot::Receiver<Result<(), String>>, String> {
    let engine = ticket.lease().engine.ok_or_else(|| "handoff requires an engine incarnation".to_string())?;
    let url = reqwest::Url::parse(&root).map_err(|e| e.to_string())?;
    if url.port_or_known_default() != Some(engine.port) {
        return Err("handoff endpoint port does not match the leased engine".into());
    }
    let mind = super::MindTurn::reclaim(ticket.persona()).await;
    let endpoint = directory().endpoint(&root);
    let admission = endpoint.admit().await?;
    if admission.incarnation() != Some((engine.pid, engine.started_s)) {
        return Err("handoff endpoint incarnation changed".into());
    }
    let pool = admission.pool.clone().ok_or_else(|| "handoff endpoint has no owned pool".to_string())?;
    directory().discover_page_admission(&client, &root).await?;
    let key = PageKey::mind(ticket.persona()).ok_or_else(|| "handoff requires a resident identity".to_string())?;
    if pool.holders.lock().values().any(|resident| resident.persona() == ticket.persona() && *resident != key) {
        return Err("handoff still has fork pages; resident-only reclaim cannot free their claim".into());
    }
    let slot = pool.holders.lock().iter().find_map(|(slot, resident)| (*resident == key).then_some(*slot))
        .ok_or_else(|| "handoff has no attributed resident slot; no reclamation proved".to_string())?;
    let pin = pool.pin(&key);
    let permit = pool.acquire_slot(slot).await?;
    admission.check_ready()?;
    if pool.holders.lock().get(&slot) != Some(&key) {
        return Err("handoff slot attribution changed before physical custody".into());
    }
    let page_io = super::admit_page_io(page_io::gate()).map_err(|e| e.to_string())?;
    let custody = Custody { endpoint: admission, _mind: mind, _permit: permit, _pin: pin, _page_io: page_io, ticket: Some(ticket), pending: false };
    let (tx, rx) = tokio::sync::oneshot::channel();
    endpoint.retain_paging(async move {
        let result = reclaim(&client, &root, pool, slot, key, custody).await
            .and_then(crate::capacity::window_grant::record_reclaimed_grant);
        let _ = tx.send(result); // the owner already retained success even if the caller left
    });
    Ok(rx)
}

async fn reclaim(
    client: &reqwest::Client, root: &str, pool: Arc<KvSlotPool>, slot: u32,
    key: PageKey, mut custody: Custody,
) -> Result<ReclaimedGrant, String> {
    // An interrupted overwrite must not leave an older save marked valid.
    pool.note_page_lost(&key);
    custody.pending = true;
    match kv_page_action(client, root, slot, &key, "save").await {
        PageOutcome::Completed => {
            custody.pending = false;
            pool.note_saved(key);
        }
        PageOutcome::Rejected => {
            custody.pending = false;
            return Err("handoff save refused; resident capacity remains charged".into());
        }
        PageOutcome::Uncertain => return Err("handoff save has no verified receipt".into()),
        PageOutcome::Unresolved(operation) => {
            custody.pending = false; // UnresolvedPage owns quarantine and custody until its receipt
            operation.hold(custody);
            return Err("handoff save still unresolved; capacity remains charged".into());
        }
    }
    // No checkpoint or returner may call this erased/uncertain slot warm. The confirmed file
    // remains valid throughout erase and is never overwritten by a later empty-slot save.
    pool.forget_resident(slot, key);
    custody.pending = true;
    match kv_page_action(client, root, slot, &key, "erase").await {
        PageOutcome::Completed => {
            custody.pending = false;
            let ticket = custody.ticket.take().ok_or_else(|| "handoff ticket already consumed".to_string())?;
            Ok(ReclaimedGrant(ticket))
        }
        PageOutcome::Rejected => {
            custody.pending = false;
            Err("handoff erase refused; capacity remains charged".into())
        }
        PageOutcome::Uncertain => Err("handoff erase has no verified receipt".into()),
        PageOutcome::Unresolved(operation) => {
            custody.pending = false;
            operation.hold(custody);
            Err("handoff erase still unresolved; capacity remains charged".into())
        }
    }
}


#[cfg(test)]
mod tests;
