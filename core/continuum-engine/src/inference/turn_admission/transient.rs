//! Transient slot admission: save and detach a resident before operator traffic borrows its slot.

use super::{KvSlotPool, PageIn, SlotPermit, TurnAdmission};
use std::sync::Arc;

impl TurnAdmission {
    /// Borrow physical `slot` for traffic that serves no page: wait for the slot's
    /// operation permit (event-driven, behind any live decode), detach its resident and
    /// SAVE the resident's page so its warmth survives, and leave the slot unattributed
    /// so no page later mistakes this traffic's KV for its own.
    ///
    /// The `page_io` permit is taken before the resident is detached, as on the keyed
    /// path: once a stop has closed page IO, a borrow neither detaches a resident nor
    /// starts a save, and a save it started holds the stop until its ledger update.
    pub(super) async fn borrow_slot_for_transient(
        mut self,
        pool: &Arc<KvSlotPool>,
        slot: u32,
        permit: SlotPermit,
        client: &reqwest::Client,
        root: &str,
        page_io: &'static crate::runtime::admission_gate::AdmissionGate,
    ) -> Result<Self, String> {
        self._slot_permit = Some(permit);
        self._endpoint.check_ready()?;
        let page_io = crate::inference::slots::admit_page_io(page_io).map_err(|closed| closed.to_string())?;
        let (previous, pin) = pool.take_resident(slot);
        self._pin = pin;
        if let Some(previous) = previous {
            let pool = pool.clone();
            let client = client.clone();
            let paging_root = root.to_owned();
            self = self.retain_paging(root, move |mut admission, _interested| async move {
                // Held until this save's ledger update: the stop waits for it.
                let _page_io = page_io;
                pool.note_page_lost(&previous);
                let saved;
                (admission, saved) = admission.page_action(&client, &paging_root, slot, &previous, "save").await?;
                if saved {
                    pool.note_saved(previous);
                    admission.saved_evictee = true;
                }
                Ok(admission)
            }).await?;
        }
        self.slot = Some(slot);
        Ok(self)
    }
}

/// Admit operator traffic — `serving/cache-probe` — onto one physical slot through the
/// same owner as every turn, never around it. It holds no lane (it serves no page);
/// it holds the slot's operation permit, so it waits behind a live decode instead of
/// queueing inside the server, and it saves and detaches the resident first.
///
/// Before this the probe posted straight to the slot: on a live node it queued behind a
/// persona's decode past its caller's patience (the M5 deploy self-check, 2026-09-25,
/// no answer in 280 s — read as `unknown` and held the fleet's deploys), overwrote her
/// warm KV without saving it, and restored its own filler under her attribution, so
/// her next save wrote filler under her name. An endpoint with no slot pool (external,
/// cloud) has no resident to protect; `slot()` is then `None`.
///
/// `slot: None` asks for the pool's SCRATCH slot when the server has one: a slot no
/// page holds, so the probe waits behind nobody and evicts nobody (Cormac's review
/// of #4388: behind a 7-minute turn even an admitted probe on slot 0 outwaits a 280 s
/// caller). Without a scratch slot it borrows slot 0 and saves its resident first.
pub async fn admit_transient(
    client: &reqwest::Client,
    root: &str,
    slot: Option<u32>,
) -> Result<TurnAdmission, String> {
    admit_transient_through(client, root, slot, crate::inference::slots::page_io::gate()).await
}

/// [`admit_transient`] through the page-IO gate `page_io`.
pub(super) async fn admit_transient_through(
    client: &reqwest::Client,
    root: &str,
    slot: Option<u32>,
    page_io: &'static crate::runtime::admission_gate::AdmissionGate,
) -> Result<TurnAdmission, String> {
    let endpoint = crate::inference::slots::directory()
        .endpoint(root)
        .admit()
        .await?;
    let pool = endpoint.pool.clone();
    let mut admission = TurnAdmission {
        slot: None,
        _pin: None,
        _slot_permit: None,
        released: None,
        page_confirmed: false,
        page_in: PageIn::None,
        saved_evictee: false,
        paging_in_flight: false,
        _permit: None,
        uncommitted: None,
        scratch: pool.as_ref().and_then(|pool| pool.scratch_slot()),
        _endpoint: endpoint,
        _mind: None,
        _lane: None,
    };
    let Some(pool) = pool else {
        return Ok(admission);
    };
    let slot = slot
        .or_else(|| pool.scratch_slot())
        .unwrap_or_else(|| pool.transient_slot()); // JUSTIFIED unwrap_or_else: no named slot and no scratch slot = the cheapest citizen slot, whose resident is saved and detached first
    if slot >= pool.n_slots() {
        return Err(format!(
            "slot {slot} does not exist on this endpoint ({} slots)",
            pool.n_slots()
        ));
    }
    // It holds no lane and no adapter permit, so waiting for the slot here holds nothing.
    let permit = pool.acquire_slot(slot).await?;
    admission = admission.borrow_slot_for_transient(&pool, slot, permit, client, root, page_io).await?;
    Ok(admission)
}

