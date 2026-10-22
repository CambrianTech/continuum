//! The CARRY of a resident's committed card work to origin: what to carry and when. The
//! mind owns it (her held cards, her checkouts, her hands); `modules::card_carry` is the
//! service module that runs the sweep on its tick and on request. The full rationale is on
//! that module.

use std::sync::Arc;
use std::time::Duration;

/// The safety-net sweep. derived-or-floor: floor — the immediate path ([`request`]) carries
/// in seconds; this only bounds how long a commit no act announced (a reboot, a shell commit)
/// can sit local. Five minutes is the delay Joel will tolerate before work shows on origin;
/// each sweep is a board read per resident and two or three git reads per held card.
pub const CARRY_SWEEP: Duration = Duration::from_secs(300);

static WAKE: std::sync::LazyLock<tokio::sync::Notify> = std::sync::LazyLock::new(tokio::sync::Notify::new);

/// Ask the carry owner to sweep now (coalesced: many requests before it runs are one sweep).
pub fn request() {
    WAKE.notify_one();
}

/// The carry task's wait: returns when [`request`] asks for a sweep.
pub async fn wait_for_request() {
    WAKE.notified().await;
}

/// One pass over every live resident's held cards.
pub async fn sweep() -> usize {
    let Some(registry) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() else {
        return 0;
    };
    let now_ms = continuum_base::time::EpochMs::now().millis();
    let mut carried = 0;
    for persona in registry.live_personas() {
        let Some(runtime) = registry.get(persona) else { continue };
        let Ok(held) = crate::persona::active_work_source::AircWorkReader::active_claims(runtime.as_ref()).await else {
            continue;
        };
        let name = runtime.agent_name().to_string();
        for card in held.iter().filter(|c| crate::persona::card_holder::hold_of(c, now_ms) == crate::persona::card_holder::Hold::Held) {
            let Some(claim) = card.claim_id else { continue };
            let Some(root) = crate::persona::card_checkout::checkout_path_for(&persona, card) else { continue };
            let context = crate::identity::hands_context::HandsContext::for_claim(persona, card.card_id.as_uuid(), claim.as_uuid());
            let lease = match crate::code::hands::admit(context) {
                Ok(lease) => Arc::new(lease),
                Err(why) => {
                    continuum_base::probe!(class = "workspace.carry.refused", persona = %name, card = %card.card_id, admission = %why, "held card's hands are closing; not carried");
                    continue;
                }
            };
            if crate::persona::workspace_transfer::carry_committed(lease, persona, &name, root, card.card_id.as_uuid()).await.is_some() {
                carried += 1;
            }
        }
    }
    carried
}
