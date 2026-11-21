//! Her senses: the worker that owns her perception region (object model 5.7, step 2a).
//!
//! CBAR, not a locked struct: each resident's [`PerceptionRegion`] is owned by one worker, her
//! Senses, which wakes only on an input: a room event from the one [`feed`], or one of her own
//! acts (`Sensed`). Nothing locks the region and nothing polls it. What she perceives is
//! published as state: her awareness strip, and what calls her now ([`Calling`]), which her
//! deliberation reads when it is free. A wake is never queued or dropped; the newest is the
//! truth (it replaced an `mpsc` that dropped wakes when full, `mind.feed.wake_dropped`).
//!
//! The address book ([`of`]) holds only handles (an input and two readers), never state: her
//! verbs reach her senses through it by persona id.
//!
//! [`PerceptionRegion`]: super::perception_region::PerceptionRegion

mod feed;
mod worker;

pub use feed::{classify, hear, install_room_attacher, Fed};
pub use worker::Sensed;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use continuum_base::time::EpochMs;
use continuum_sage::dataflow::{OwnerId, Reader, TopicName};
use continuum_sage::component::{start as start_part, Bus, Key, Running, Taken, Teller};
use parking_lot::RwLock;
use tokio::sync::watch;
use uuid::Uuid;

use super::awareness::AwarenessSnapshot;
use super::perception_region::PerceptionRegion;

use crate::cognition::conversation::persona_identity::PersonaIdentity;
use worker::Senses;

/// What calls her now, as her senses publish it on her mind's bus: the loudest wake, if any,
/// and every activity still loud and unread. A state, never a queue: only the newest matters,
/// and her deliberation reads it when it is free.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Calling {
    pub wake: Option<crate::cognition::conversation::Wake>,
    pub pending: std::collections::BTreeSet<Uuid>,
}

/// The topic `Calling` is published on.
pub const CALLING: TopicName = TopicName("mind/calling");

/// Where her verbs and readers reach her senses. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    senses: Teller<Senses>,
    awareness: watch::Receiver<AwarenessSnapshot>,
    calling: Reader<Calling>,
}

impl Handle {
    /// Her awareness strip as of `now_ms`: the newest one her senses published, read without
    /// waiting on them.
    pub fn awareness(&self, now_ms: u64) -> AwarenessSnapshot {
        // Published when something moved; read at the reader's time (never earlier), so a
        // window that closed since is read closed.
        let mut snapshot = self.awareness.borrow().clone();
        snapshot.at_ms = snapshot.at_ms.max(now_ms);
        snapshot
    }

    /// When anything she perceives last moved (the newest timed event across her
    /// activities); `None` before any has.
    pub fn moved_at_ms(&self) -> Option<u64> {
        self.awareness.borrow().lines.iter().map(|line| line.last_activity_ms).filter(|ms| *ms > 0).max()
    }

    /// What calls her now.
    pub fn calling(&self) -> Reader<Calling> {
        self.calling.clone()
    }

    /// Hand her senses one of her acts; false when they have stopped.
    pub async fn tell(&self, act: Sensed) -> bool {
        self.senses.tell(act).await
    }
}

/// A resident's senses, owned by her conversation: dropping it takes her off the feed. Her mind's
/// bus lives here, so the parts started on it later share it.
pub struct Resident {
    persona: Uuid,
    handle: Handle,
    bus: Arc<Bus>,
    _senses: Running<Senses>,
}

impl Resident {
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Her mind's bus: where her parts hear and say.
    pub fn bus(&self) -> &Arc<Bus> {
        &self.bus
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        RESIDENTS.write().remove(&self.persona);
    }
}

static RESIDENTS: LazyLock<RwLock<HashMap<Uuid, Handle>>> = LazyLock::new(Default::default);

/// Start her senses on `region`, a part of her on her own mind's bus, hearing the one feed.
/// `agent_name` is how a line addresses her (`PersonaIdentity::mentions`).
pub fn start(persona: Uuid, agent_name: &str, mut region: PerceptionRegion, awareness: watch::Receiver<AwarenessSnapshot>) -> Result<Resident, Taken> {
    // A saved continuation calls her with `Resume` first; asking the region once takes it.
    let resume_pending = matches!(region.wake_for(EpochMs::now().millis()), Some(crate::cognition::conversation::Wake::Resume));
    let bus = Arc::new(Bus::new());
    let calling = bus.reader::<Calling>(CALLING, Key::ONE);
    let says = bus.publisher::<Calling>(CALLING, Key::ONE)?;
    let senses = Senses {
        persona,
        peer: airc_core::PeerId::from_uuid(persona),
        identity: PersonaIdentity::new(persona, agent_name.to_string()),
        region,
        admission: None,
        calling: says,
        published: Calling::default(),
        due_at: None,
        resume_pending,
        served_due: None,
    };
    let running = start_part(senses, &bus, OwnerId(persona), EpochMs::now().millis());
    let handle = Handle { senses: running.teller(), awareness, calling };
    RESIDENTS.write().insert(persona, handle.clone());
    continuum_base::probe!(class = "mind.feed.registered", persona = %persona, "her senses are on the one feed");
    Ok(Resident { persona, handle, bus, _senses: running })
}

/// Her senses on this core; `None` when she is not resident here.
pub fn of(persona: Uuid) -> Option<Handle> {
    RESIDENTS.read().get(&persona).cloned()
}

/// Where her durable mind state lives: her peer dir beside the rest of what is hers.
pub fn mind_dir(persona: Uuid) -> Option<std::path::PathBuf> {
    crate::paths::continuum_home().ok().map(|home| home.join("personas").join(persona.to_string()))
}

/// She took in `room` up to this line: her view of it drains and her cursor advances. Her
/// verbs call this from synchronous code, so it is handed over without waiting; her senses
/// take it in order with everything else.
pub fn mark_perceived(persona: Uuid, room: Uuid, lamport: u64, event_id: Option<Uuid>, _now_ms: u64) {
    let Some(handle) = of(persona) else { return };
    let cursor = super::mind_state::ActivityCursor { chat_lamport: lamport, chat_event_id: event_id, views: Default::default() };
    tokio::spawn(async move {
        if !handle.tell(Sensed::Perceived { room, cursor }).await {
            continuum_base::probe!(class = "mind.senses.stopped", persona = %persona, "her senses stopped before taking in a room she perceived");
        }
    });
}

#[cfg(test)]
mod tests;
