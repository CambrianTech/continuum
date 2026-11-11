//! What holding context costs her acts (`continuum_context::price`), one running fit per engine and
//! model that served her: seconds per held token is a property of the engine and model, never of
//! her window, so a window change keeps the fit, and it is checkpointed with the rest of her
//! volatile mind, so a restart keeps it too (the dyno on #5822: a fit reset on every window change
//! and deploy was measured on only 14-53% of her acts).

use continuum_context::price::{ActSample, ActTimeFit};

/// Which engine and model served an act: the key her act time is measured under.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServedBy {
    /// The adapter's provider (`AIProviderAdapter::provider_id`): the engine on this node, or
    /// a remote seat's.
    pub provider: String,
    pub model: String,
}

/// Her act-time fits, one per [`ServedBy`].
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ActTimes(Vec<(ServedBy, ActTimeFit)>);

impl ActTimes {
    /// The fit measured under `served`; empty when none was.
    pub fn fit(&self, served: &ServedBy) -> ActTimeFit {
        self.0.iter().find(|(by, _)| by == served).map_or_else(ActTimeFit::default, |(_, fit)| *fit) // map_or_else: an engine and model never measured has an empty fit
    }

    /// Fold one completed act served by `served` into its fit.
    pub fn observe(&mut self, served: ServedBy, act: &ActSample) {
        match self.0.iter_mut().find(|(by, _)| *by == served) {
            Some((_, fit)) => fit.observe(act),
            None => {
                let mut fit = ActTimeFit::default();
                fit.observe(act);
                self.0.push((served, fit));
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl super::WorkingMemory {
    /// What holding context costs her acts served by `served`, as measured so far.
    pub fn act_time(&self, served: &ServedBy) -> ActTimeFit {
        self.act_times.lock().fit(served)
    }

    /// Fold one of her completed acts, served by `served`, into her act time.
    pub fn observe_act(&self, served: ServedBy, act: &ActSample) {
        self.touch();
        self.act_times.lock().observe(served, act);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the dyno on #5822): a fit keyed by her window, so every window change and
    // restart measured afresh. Acts on one engine and model accumulate whatever her window, survive a
    // checkpoint round trip, and another model's acts never join them.
    #[test]
    fn a_fit_is_kept_per_engine_and_model_across_a_checkpoint() {
        let served = |model: &str| ServedBy { provider: "llama-server".into(), model: model.into() };
        let act = |input: u32| ActSample { input, cached: input / 2, fresh: input / 2, prefill_ms: f64::from(input) / 10.0, output: 300, decode_ms: 20_000.0 + f64::from(input) };
        let mut times = ActTimes::default();
        for input in [10_000, 20_000, 30_000, 40_000] {
            times.observe(served("a"), &act(input));
        }
        times.observe(served("b"), &act(10_000));
        assert_eq!((times.fit(&served("a")).acts(), times.fit(&served("b")).acts(), times.fit(&served("c")).acts()), (4, 1, 0));
        let saved = serde_json::to_string(&times).expect("test: serializes"); // JUSTIFIED: test fixture
        let restored: ActTimes = serde_json::from_str(&saved).expect("test: restores"); // JUSTIFIED: test fixture
        assert_eq!(restored, times);
    }
}
