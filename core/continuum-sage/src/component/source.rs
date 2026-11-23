//! A state said from outside any component: a monitor's poll, a daemon's state, a driver's output.
//! It claims its topic on a bus under its own name and numbers what it says, so whoever hears it
//! reads one writer's sequence and nobody hand-builds a run id to say it.

use crate::dataflow::{OwnerId, Provenance, Publisher, RunId, StageId, StageIncarnation, TopicName};

use super::{Bus, Key, Taken};

/// One writer of one state topic, outside any component.
pub struct Source<T> {
    publisher: Publisher<T>,
    stage: StageId,
    said: u64,
}

impl<T: Send + Sync + 'static> Source<T> {
    /// Claim `topic` at `key` on `bus` as `name`. A topic already claimed is refused: one writer.
    pub fn on(bus: &Bus, topic: TopicName, key: Key, name: &str) -> Result<Self, Taken> {
        Ok(Self { publisher: bus.publisher::<T>(topic, key)?, stage: StageId::named(name), said: 0 })
    }

    /// Say `value`, the newest of this writer's sequence.
    pub fn say(&mut self, value: T) {
        self.said += 1;
        let by = RunId { incarnation: StageIncarnation { stage: self.stage, owner: OwnerId(uuid::Uuid::nil()), epoch: 0 }, seq: self.said };
        self.publisher.publish(value, Provenance { by, from: Vec::new() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a second writer on one state topic, or a writer whose sayings carry no
    // sequence (a reader could not tell its newest from a replay).
    #[test]
    fn a_source_is_the_one_writer_and_numbers_what_it_says() {
        const TOPIC: TopicName = TopicName("test/source");
        let bus = Bus::new();
        let mut source = Source::<u32>::on(&bus, TOPIC, Key::ONE, "test-source").expect("test: the first claim");
        assert!(Source::<u32>::on(&bus, TOPIC, Key::ONE, "test-source-2").is_err(), "a second writer is refused");
        let reader = bus.reader::<u32>(TOPIC, Key::ONE);
        source.say(7);
        source.say(9);
        let newest = reader.latest().expect("test: said");
        assert_eq!((*newest.value, newest.provenance.by.seq), (9, 2), "the newest value, second in its writer's sequence");
    }
}
