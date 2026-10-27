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

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::persona::card_carry::{sweep, wait_for_request, CARRY_SWEEP};
use crate::runtime::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};

pub struct CardCarryModule;

impl CardCarryModule {
    pub fn new() -> Self {
        Self
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
                wait_for_request().await;
                let carried = sweep().await;
                continuum_base::probe!(class = "workspace.carry.sweep", trigger = "request", carried = carried as u64, "carry owner swept on request");
            }
        });
        continuum_base::probe!(class = "workspace.carry.ready", sweep_secs = CARRY_SWEEP.as_secs(), "card carry owner up");
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        let carried = sweep().await;
        continuum_base::probe!(class = "workspace.carry.sweep", trigger = "tick", carried = carried as u64, "carry owner swept on its tick");
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        match command {
            // Operator/agent lever: sweep now and say how many checkouts were carried.
            "work/carry" => Ok(CommandResult::Json(json!({ "carried": sweep().await }))),
            other => Err(format!("card_carry: unknown command {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
