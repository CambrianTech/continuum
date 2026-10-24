//! The guard that puts a cycle's genome back after a turn. It belongs to the cycle (it reads
//! and pages the cycle's own genome), so it lives with the workspace; the genome decides
//! which genes a turn runs with (`genome::gene_trial::live_genes`) and never names the cycle.

use super::*;

/// Puts her genome back when a work turn ends, however it ends: the snapshot taken before
/// the turn by default, or her promoted genome when the turn got far enough to say so.
/// Restores on drop, so a cancelled or unwinding turn cannot leave a trial gene on her
/// cycle for the conversation turns that follow.
pub struct GenomeRestore {
    cycle: Arc<WorkspaceCycle>,
    to: Vec<ActiveAdapterRequest>,
}

impl GenomeRestore {
    pub fn snapshot(cycle: Arc<WorkspaceCycle>) -> Self {
        let to = cycle.genome();
        Self { cycle, to }
    }

    pub fn restore_to(&mut self, genes: Vec<ActiveAdapterRequest>) {
        self.to = genes;
    }
}

impl Drop for GenomeRestore {
    fn drop(&mut self) {
        self.cycle.page_in(std::mem::take(&mut self.to));
    }
}
