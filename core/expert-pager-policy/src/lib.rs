//! Expert-pager policy — the learned-residency half of the
//! mechanism/policy seam (#276). See Cargo.toml for why this is a leaf
//! crate (windows-msvc driver requirement) and
//! `docs/architecture/EXPERT-PAGING-CONTROL-LAW.md` for the control
//! law. continuum-core re-exports these modules under
//! `capacity::{expert_decay_policy, plan_file, bandit_plan_controller}`
//! — same paths as before the extraction.

pub mod controller;
pub mod decay;
pub mod division;
pub mod expert_id;
pub mod plan_file;
pub mod segment;

pub use controller::BanditPlanController;
pub use decay::{DecayBandit, EmaScoreboard, DECAY_ARMS, REWARD_ALPHA};
pub use division::{
    feasible_divisions, predict_tok_s, CoverageModel, DivisionBandit, DivisionConfig,
    HardwareBudget, MoeShape, ResidentTier,
};
pub use expert_id::ExpertId;
pub use plan_file::{write_plan_file, PlanFileDocument, PlanPin, PLAN_FILE_VERSION};
