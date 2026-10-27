//! A mind's tool working set, learned from her own tool calls (plan goal 10, card ecd5dff3).
//!
//! # The decision
//!
//! The chat template renders the tool schemas right after the system prompt, so the schemas
//! are the HEAD of her prompt: every byte of them is prefilled once per epoch, and changing
//! them mid-epoch re-prefills everything after them. So the head is chosen once, at an epoch
//! turnover, the one moment it may change. A verb not in the head stays reachable: she finds
//! it and its schema is appended to the TAIL, which costs her one extra act.
//!
//! So each schema in the head is a bet. Holding tool `t` costs its schema tokens `s_t`
//! whenever she does not call it this epoch; leaving it out costs one extra act whenever she
//! does. With `p_t` the chance she calls it this epoch and `m` an extra act's cost in the
//! same unit, holding it is right exactly when `s_t (1 - p_t) < p_t m` ([`head::choose`]).
//! Every policy here is an ESTIMATOR of `p_t`; the decision rule and the price are shared, so
//! the policies differ only in how well they predict her.
//!
//! # The inputs (measured, none hand-coded)
//!
//! - her tool calls over time, decayed by age ([`policy::frequency`]);
//! - transitions between her calls, a Markov chain run forward from her last call
//!   ([`policy::transition`]);
//! - the activity she turns over in ([`features`]: occurrence within that activity);
//! - the society: the other minds' recent usage ([`History::population`]), the cold start a
//!   new mind gets for free;
//! - relevance of her recent acts' text to each tool's description, behind the
//!   [`relevance::Relevance`] seam (a lexical TF-IDF here; the core's embedding owner is the
//!   intended implementation in slice 2);
//! - the served window, which bounds the head's tokens (`budget_tokens`).
//!
//! The [`policy::blend`] policy LEARNS how to weigh those inputs, either fitted once on early
//! history or online, updated after every epoch it served.
//!
//! # Where this is used
//!
//! [`evaluate`] replays each mind's real history in time order (no policy ever sees a call at
//! or after the epoch it decides) and the `tool-working-set` binary prints the comparison.
//! Slice 2 puts the chosen policy behind [`WorkingSetPolicy`] in the mind's tool surface.

pub mod cost;
pub mod epoch;
pub mod evaluate;
pub mod features;
pub mod head;
pub mod history;
pub mod load;
pub mod logistic;
pub mod policy;
pub mod relevance;
pub mod report;
pub mod vocab;

pub use cost::CostModel;
pub use epoch::Epoch;
pub use head::HeadSet;
pub use history::{ActivityId, EpochMs, Call, History};
pub use vocab::{ToolId, Vocabulary};

/// Chooses the tool schemas a mind's prompt carries in its head for one epoch.
///
/// Asked once per epoch, at turnover: the head it returns is the same bytes on every act
/// until the next turnover, so the policy can never cause a mid-epoch re-prefill.
pub trait WorkingSetPolicy {
    /// A short name for the report.
    fn name(&self) -> String;

    /// The head for the epoch that starts at `history.at`, at most `budget_tokens` of schema.
    fn head_for_epoch(&self, history: &History<'_>, budget_tokens: u32) -> HeadSet;

    /// The epoch this policy chose a head for has ended and `used` are the tools she called in
    /// it. A learning policy updates here; every other policy reads only `history`.
    fn observe(&mut self, _history: &History<'_>, _used: &[ToolId]) {}
}
