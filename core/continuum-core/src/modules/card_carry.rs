//! CARD CARRY — one owner keeps every resident's committed card work on origin.
//!
//! A resident's commits reached origin only through the turn's end, for the card that turn
//! was bound to. Measured on the M5 2026-10-17: Benchy wrote and committed her probe card
//! (bfbad4ace, 11:52Z) inside one directed turn still running at act 14 an hour later, then
//! her focus moved to another held card, and the commit never left the node; her own
//! `git push` fails by design (her shell carries no credentials). Every trigger tied to
//! turn shape missed a case, so the carry is owned HERE, keyed on the board and the disk:
//!
//! - WHAT: every card a live resident holds (board hold = Held, card not settled), at the
//!   checkout `card_staging::checkout_path_for` resolves, whose branch is ahead of origin
//!   with a clean tree. Never an agent's or operator's worktree; never uncommitted work.
//! - WHEN: [`request`] wakes it at once (an act that changed a checkout, a turn's end), and a
//!   [`CARRY_SWEEP`] tick is the net for anything else (a reboot, a shell commit, a moved
//!   focus, a turn that never ends). A periodic actuator, not detection by polling.
//! - HOW: under an admission to that claim's hands, through the turn's own bounded,
//!   per-checkout-locked sync; one `workspace.push` probe per carry.

use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};

/// The safety-net sweep. derived-or-floor: floor — the immediate path ([`request`]) carries
/// in seconds; this only bounds how long a commit no act announced (a reboot, a shell commit)
/// can sit local. Five minutes is the delay Joel will tolerate before work shows on origin;
/// each sweep is a board read per resident and two or three git reads per held card.
const CARRY_SWEEP: Duration = Duration::from_secs(300);

static WAKE: std::sync::LazyLock<tokio::sync::Notify> = std::sync::LazyLock::new(tokio::sync::Notify::new);

/// Ask the carry owner to sweep now (coalesced: many requests before it runs are one sweep).
pub fn request() {
    WAKE.notify_one();
}

pub struct CardCarryModule;

impl CardCarryModule {
    pub fn new() -> Self {
        Self
    }

    /// One pass over every live resident's held cards.
    pub async fn sweep() -> usize {
        let Some(registry) = crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global() else {
            return 0;
        };
        let now_ms = crate::time::EpochMs::now().millis();
        let mut carried = 0;
        for persona in registry.live_personas() {
            let Some(runtime) = registry.get(persona) else { continue };
            let Ok(held) = crate::persona::active_work_source::AircWorkReader::active_claims(runtime.as_ref()).await else {
                continue;
            };
            let name = runtime.agent_name().to_string();
            for card in held.iter().filter(|c| crate::persona::card_holder::hold_of(c, now_ms) == crate::persona::card_holder::Hold::Held) {
                let Some(claim) = card.claim_id else { continue };
                let Some(root) = crate::modules::card_staging::checkout_path_for(&persona, card) else { continue };
                let context = crate::code::hands::HandsContext::for_claim(persona, card.card_id.as_uuid(), claim.as_uuid());
                let lease = match crate::code::hands::admit(context) {
                    Ok(lease) => Arc::new(lease),
                    Err(why) => {
                        crate::probe!(class = "workspace.carry.refused", persona = %name, card = %card.card_id, admission = %why, "held card's hands are closing; not carried");
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
}

impl Default for CardCarryModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for CardCarryModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "card_carry",
            priority: ModulePriority::Normal,
            command_prefixes: &["work/carry"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 1,
            tick_interval: Some(CARRY_SWEEP),
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        // The immediate path: its own task, woken by `request`, one sweep per wake.
        tokio::spawn(async {
            loop {
                WAKE.notified().await;
                let carried = CardCarryModule::sweep().await;
                crate::probe!(class = "workspace.carry.sweep", trigger = "request", carried = carried as u64, "carry owner swept on request");
            }
        });
        crate::probe!(class = "workspace.carry.ready", sweep_secs = CARRY_SWEEP.as_secs(), "card carry owner up");
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        let carried = Self::sweep().await;
        crate::probe!(class = "workspace.carry.sweep", trigger = "tick", carried = carried as u64, "carry owner swept on its tick");
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        match command {
            // Operator/agent lever: sweep now and say how many checkouts were carried.
            "work/carry" => Ok(CommandResult::Json(json!({ "carried": Self::sweep().await }))),
            other => Err(format!("card_carry: unknown command {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
