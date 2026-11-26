//! Her experience, said on her own bus as it lands: `mind/experience` holds the newest Episodic
//! engram her store admitted, so a part of her that works on lived experience (her dream) wakes when
//! there is some, never on a clock scanning for it. A state, not an event: a part that was busy
//! while several landed wakes once, on the newest, and reads her store for the rest. Her admission store is its one owner: an
//! engram is hers once admitted here, through [`AdmissionState::admit`] or
//! [`AdmissionState::admit_reflection`], and nowhere else.
//!
//! Her bus is hers (`senses::start`), so the topic is claimed there and handed to her store with
//! [`AdmissionState::say_experience_on`]. A store with no bus (a test, a measurement fork) says
//! nothing and admits the same.

use continuum_sage::component::{Bus, Key, Source, Taken};
use continuum_sage::dataflow::TopicName;
use uuid::Uuid;

use super::AdmissionState;
use crate::cognition::engram::{Engram, EngramKind};

/// The newest Episodic engram her store admitted (watched: her parts read the newest).
pub const EXPERIENCE: TopicName = TopicName("mind/experience");

/// One lived moment of hers, now in her memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Experienced {
    pub engram: Uuid,
}

impl AdmissionState {
    /// Say the newest Episodic engram this store admits on `bus` from now on. Her experience topic has
    /// one writer: a second claim is refused.
    pub fn say_experience_on(&self, bus: &Bus) -> Result<(), Taken> {
        let source = Source::on(bus, EXPERIENCE, Key::ONE, "admission")?;
        *self.experience.lock() = Some(source);
        Ok(())
    }

    /// Her Episodic engrams admitted at or after admission index `from` (her store keeps admission
    /// order), oldest first: what her dream may consolidate.
    pub fn episodics_from(&self, from: usize) -> Vec<Engram> {
        let engrams = self.engrams.lock();
        // An index past her store (a restored, shorter store) has nothing lived since it.
        let Some(lived) = engrams.get(from..) else { return Vec::new() };
        lived.iter().filter(|e| e.kind == EngramKind::Episodic).cloned().collect()
    }

    /// Say `engram` if it is lived experience; her own distilled facts and her tool receipts are
    /// not, and a store with no bus says nothing.
    pub(super) fn say_experienced(&self, engram: &Engram) {
        if engram.kind != EngramKind::Episodic {
            return;
        }
        if let Some(source) = self.experience.lock().as_mut() {
            source.say(Experienced { engram: engram.id });
        }
    }
}

/// Where her experience is said: nothing until her bus claims it.
pub(super) type ExperienceSaid = parking_lot::Mutex<Option<Source<Experienced>>>;
