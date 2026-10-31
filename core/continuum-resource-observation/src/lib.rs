//! Immutable sampling attempts at the resource authority boundary.
//!
//! The topology owner supplies pool identities; adapters supply observations.
//! Neither missing measurements nor older successful samples imply free capacity.
//! Allocation and physical custody belong to the authority, not this crate.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

/// Identity assigned to one physical pool, shared by all of its aliases.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PoolId(pub u128);

/// A topology incarnation changes when a device or its alias mapping changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopologyRevision(pub u128);

/// A local observation clock incarnation. Durations from different clocks cannot
/// be compared (including across a process restart or a remote node).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockId(pub u128);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationTime {
    pub clock: ClockId,
    pub elapsed: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceId(pub u128);

/// Both quantities come from the same sampling attempt. Private fields prevent
/// callers from constructing a physically inconsistent pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteMeasurement {
    total: u64,
    available: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvailableExceedsTotal {
    pub total: u64,
    pub available: u64,
}

impl ByteMeasurement {
    pub fn new(total: u64, available: u64) -> Result<Self, AvailableExceedsTotal> {
        if available > total {
            return Err(AvailableExceedsTotal { total, available });
        }
        Ok(Self { total, available })
    }

    pub fn total(self) -> u64 {
        self.total
    }
    pub fn available(self) -> u64 {
        self.available
    }
    pub fn used(self) -> u64 {
        self.total - self.available
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableReason {
    NotSampled,
    ProbeFailed,
    DeviceRemoved,
    ProbeUncontained,
    InvalidMeasurement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Measurement {
    Measured(ByteMeasurement),
    Unavailable(UnavailableReason),
}

/// Publish this entire value together, never its fields through separate atomics.
/// A failed attempt replaces the current attempt; last-good history is diagnostic
/// data and cannot be substituted here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolObservation {
    pub pool: PoolId,
    pub topology: TopologyRevision,
    pub source: SourceId,
    pub at: ObservationTime,
    pub measurement: Measurement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationRefusal {
    WrongPool,
    WrongTopology,
    WrongSource,
    DifferentClock,
    FutureObservation,
    Stale { age: Duration },
    Unavailable(UnavailableReason),
}

impl PoolObservation {
    /// Freshness is supplied explicitly by the observation owner. There is no
    /// machine-specific expiry or fallback to an older observation in this API.
    pub fn measured_at(
        self,
        pool: PoolId,
        topology: TopologyRevision,
        source: SourceId,
        now: ObservationTime,
        max_age: Duration,
    ) -> Result<ByteMeasurement, ObservationRefusal> {
        if self.pool != pool {
            return Err(ObservationRefusal::WrongPool);
        }
        if self.topology != topology {
            return Err(ObservationRefusal::WrongTopology);
        }
        if self.source != source {
            return Err(ObservationRefusal::WrongSource);
        }
        if self.at.clock != now.clock {
            return Err(ObservationRefusal::DifferentClock);
        }
        let Some(age) = now.elapsed.checked_sub(self.at.elapsed) else {
            return Err(ObservationRefusal::FutureObservation);
        };
        if age > max_age {
            return Err(ObservationRefusal::Stale { age });
        }
        match self.measurement {
            Measurement::Measured(value) => Ok(value),
            Measurement::Unavailable(reason) => Err(ObservationRefusal::Unavailable(reason)),
        }
    }
}

#[cfg(test)]
mod tests;
