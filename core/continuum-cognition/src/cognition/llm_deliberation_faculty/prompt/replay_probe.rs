//! What a replay of her real prompt path reads between acts (`tests/faculty_replay`), and the
//! one step of the request path it mirrors after a reply: fixtures only, never production.

use super::*;

impl LlmDeliberationFaculty {
    /// Her view's epoch, `None` before her first view: a change between two acts is a turnover
    /// (`epoch.rs` `fit_view`), the one re-prefill an epoch costs.
    pub fn view_epoch(&self) -> Option<u64> {
        self.context_view.lock().as_ref().map(|view| view.epoch_id())
    }

    /// The prompt built on `view` was served: its tail joins her view as sent, exactly as the
    /// request path does on a reply (`verdict/contribute.rs`, `view_served`).
    pub fn serve_view(&self, view: &DeliberationPromptView) {
        self.view_served(view);
    }
}
