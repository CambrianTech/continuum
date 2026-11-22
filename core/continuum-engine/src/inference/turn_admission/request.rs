//! Turn request — the single struct that carries all parameters through ~80 call sites.
//!
//! Before this, `admit_turn` took 8 positional parameters and every caller had to pass them in
//! exact order: concurrency, class, key, pool, client, root, approx_tokens, patience. That made
//! it easy to swap two fields silently (the compiler would accept a u64 where an Option was
//! expected) and hard to see what each call actually needed at a glance.
//!
//! The struct holds the concrete types *after* `Into` conversion so callers never have to
//! worry about trait bounds — they just supply whatever Into-compatible type they already have.

use std::sync::Arc;

use crate::inference::slots::{DecodeBound, KvSlotPool, PageKey, SlotClass};

use super::AdmissionPatience;

/// All parameters that travel through every `admit_turn` call site.
///
/// Fields are borrowed (not owned) because the underlying `admit()` function only borrows them:
/// `client` is a `&reqwest::Client`, `root` is a `&str`. A swapped u64/Option does not compile —
/// the struct's field types make order errors visible at the call site.
#[derive(Debug)]
pub struct TurnRequest<'a> {
    /// The endpoint's concurrency bound (DecodeBound).
    pub concurrency: DecodeBound,
    /// The slot class for this turn (Turn, Dream, Background).
    pub class: SlotClass,
    /// Optional page key — Some for keyed turns/dreams, None for unkeyed.
    pub key: Option<PageKey>,
    /// Optional KV slot pool — discovered at runtime from the endpoint.
    pub pool: Option<Arc<KvSlotPool>>,
    /// The HTTP client for engine communication (borrowed).
    pub client: &'a reqwest::Client,
    /// The endpoint root URL (borrowed).
    pub root: &'a str,
    /// Approximate token count for admission budgeting.
    pub approx_tokens: u64,
    /// How long to wait for readiness before refusing.
    pub patience: AdmissionPatience,
}

impl<'a> TurnRequest<'a> {
    /// Create a new TurnRequest from Into-compatible inputs.
    ///
    /// This constructor performs the `Into` conversions that callers would have done manually
    /// before passing to `admit_turn`. The concrete types end up in the struct fields, so
    /// borrow-checker errors are caught at construction time rather than deep inside admit().
    pub fn new(
        concurrency: impl Into<DecodeBound>,
        class: SlotClass,
        key: Option<PageKey>,
        pool: Option<Arc<KvSlotPool>>,
        client: &'a reqwest::Client,
        root: &'a str,
        approx_tokens: u64,
        patience: impl Into<AdmissionPatience>,
    ) -> Self {
        Self {
            concurrency: concurrency.into(),
            class,
            key,
            pool,
            client,
            root,
            approx_tokens,
            patience: patience.into(),
        }
    }

    /// Convert back to the positional arguments that `admit()` expects.
    ///
    /// This is used internally by `admit_turn` — callers never need it.
    pub(crate) fn as_args(&self) -> (DecodeBound, SlotClass, Option<PageKey>, Option<Arc<KvSlotPool>>, &'a reqwest::Client, &'a str, u64, AdmissionPatience) {
        (
            self.concurrency.clone(),
            self.class,
            self.key,
            self.pool.clone(),
            self.client,
            self.root,
            self.approx_tokens,
            self.patience,
        )
    }
}
