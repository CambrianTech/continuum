//! The publication rule: within one incarnation, an output derived from older input never
//! overwrites state derived from newer input (overlapping runs, a late result).

use std::collections::BTreeMap;

use super::topic::{InputRevision, TopicName};

/// The input revisions the currently published state was derived from, per topic.
#[derive(Debug, Clone, Default)]
pub struct Publication {
    derived_from: BTreeMap<TopicName, u64>,
}

/// What an attempt to publish did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Published {
    /// The output's inputs are at least as new as the published state's on every topic: it is
    /// now the published state.
    Accepted,
    /// Some topic it read is older than what the published state already reflects: refused.
    Stale,
}

impl Publication {
    /// Compare and set: publish only if no topic the output read is behind the published state.
    pub fn try_publish(&mut self, from: &[InputRevision]) -> Published {
        let behind = from.iter().any(|input| {
            self.derived_from
                .get(&input.topic)
                .is_some_and(|&published| input.through < published)
        });
        if behind {
            return Published::Stale;
        }
        for input in from {
            self.derived_from.insert(input.topic, input.through);
        }
        Published::Accepted
    }
}
