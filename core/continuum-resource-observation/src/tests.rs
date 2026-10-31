use super::*;

fn observation(measurement: Measurement) -> PoolObservation {
    PoolObservation {
        pool: PoolId(1),
        topology: TopologyRevision(2),
        source: SourceId(3),
        at: ObservationTime {
            clock: ClockId(4),
            elapsed: Duration::from_secs(5),
        },
        measurement,
    }
}

#[test]
fn exhausted_pool_is_measured_but_failed_attempt_is_not_free_capacity() {
    let bytes = ByteMeasurement::new(100, 0);
    assert_eq!(bytes.map(ByteMeasurement::used), Ok(100));
    let Ok(bytes) = bytes else { return };
    let previous = observation(Measurement::Measured(bytes));
    let failed = PoolObservation {
        measurement: Measurement::Unavailable(UnavailableReason::ProbeFailed),
        ..previous
    };
    assert_eq!(
        previous.measured_at(
            previous.pool,
            previous.topology,
            previous.source,
            previous.at,
            Duration::ZERO
        ),
        Ok(bytes)
    );
    assert_eq!(
        failed.measured_at(
            failed.pool,
            failed.topology,
            failed.source,
            failed.at,
            Duration::ZERO
        ),
        Err(ObservationRefusal::Unavailable(
            UnavailableReason::ProbeFailed
        ))
    );
}

#[test]
fn impossible_measurements_are_rejected_instead_of_saturated_to_empty_usage() {
    for total in [0, 1, 4096, u64::MAX - 1] {
        assert_eq!(
            ByteMeasurement::new(total, total + 1),
            Err(AvailableExceedsTotal {
                total,
                available: total + 1
            })
        );
    }
    assert!(ByteMeasurement::new(u64::MAX, u64::MAX).is_ok());
}

#[test]
fn device_replacement_clock_restart_and_staleness_cannot_reuse_a_sample() {
    let sample = observation(Measurement::Unavailable(UnavailableReason::NotSampled));
    let read = |pool, topology, now| {
        sample.measured_at(pool, topology, sample.source, now, Duration::from_secs(1))
    };
    assert_eq!(
        sample.measured_at(
            sample.pool,
            sample.topology,
            SourceId(9),
            sample.at,
            Duration::ZERO
        ),
        Err(ObservationRefusal::WrongSource)
    );
    assert_eq!(
        read(PoolId(9), sample.topology, sample.at),
        Err(ObservationRefusal::WrongPool)
    );
    assert_eq!(
        read(sample.pool, TopologyRevision(9), sample.at),
        Err(ObservationRefusal::WrongTopology)
    );
    assert_eq!(
        read(
            sample.pool,
            sample.topology,
            ObservationTime {
                clock: ClockId(9),
                ..sample.at
            }
        ),
        Err(ObservationRefusal::DifferentClock)
    );
    assert_eq!(
        read(
            sample.pool,
            sample.topology,
            ObservationTime {
                elapsed: Duration::ZERO,
                ..sample.at
            }
        ),
        Err(ObservationRefusal::FutureObservation)
    );
    assert_eq!(
        read(
            sample.pool,
            sample.topology,
            ObservationTime {
                elapsed: Duration::from_secs(7),
                ..sample.at
            }
        ),
        Err(ObservationRefusal::Stale {
            age: Duration::from_secs(2)
        })
    );
}
