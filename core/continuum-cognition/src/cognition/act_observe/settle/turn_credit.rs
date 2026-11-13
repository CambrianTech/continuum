//! What a card-linked turn's credit owner receives from the settle: the acts so far, the
//! generations that produced them, and her report. The owner is persona's training producer
//! (`TurnCreditCapture`); the settle never assigns a verdict, it only hands over what
//! happened at its stage points and at the end.

use async_trait::async_trait;

#[async_trait]
pub trait TurnCredit: Send {
    /// Stage the turn as it stands. `final_snapshot` marks the settle's last call. Returns
    /// whether anything was staged.
    async fn record(
        &mut self,
        acts: &[(String, Vec<crate::ai::types::ToolCall>)],
        receipts: &[crate::ai::provenance::GenerationReceipt],
        report: Option<&str>,
        final_snapshot: bool,
    ) -> bool;
}
