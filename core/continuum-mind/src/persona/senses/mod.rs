//! Her senses: the worker that owns her perception region (object model 5.7, step 2a).
//!
//! CBAR, not a locked struct: each resident's [`PerceptionRegion`] is owned by one worker, her
//! Senses, which wakes only on an input: a room event from the one [`feed`](crate::cognition::room::feed), handed on by her
//! [`thread`](crate::cognition::room::thread) once it took the event in, or one of her own
//! acts (`Sensed`). Nothing locks the region and nothing polls it. What she perceives is
//! published as state: her awareness strip, and what calls her now ([`Calling`]), which her
//! deliberation reads when it is free. A wake is never queued or dropped; the newest is the
//! truth (it replaced an `mpsc` that dropped wakes when full, `mind.feed.wake_dropped`).
//!
//! The address book ([`of`]) holds only handles (an input and two readers), never state: her
//! verbs reach her senses through it by persona id.
//!
//! [`PerceptionRegion`]: super::perception_region::PerceptionRegion

mod digest;
mod worker;

pub use worker::Sensed;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use continuum_base::time::EpochMs;
use continuum_sage::dataflow::{Emitter, OwnerId, Reader, TopicName};
use continuum_sage::component::{Bus, Key, Need, Owner, Taken, Teller};
use parking_lot::RwLock;

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

/// Her awareness strip, on her mind's bus: every activity's moment and unread, her continuation, her
/// dial. The one copy her readers and her parts read (her digest watches it).
pub const AWARENESS: TopicName = TopicName("mind/awareness");

/// How many room events her parts keep unheard while busy: as many as her room view keeps
/// unperceived, so the stage before her senses never holds less than they do.
const UNHEARD: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(super::perception_region::UNPERCEIVED_CAP) {
    Some(unheard) => unheard,
    None => std::num::NonZeroUsize::MIN,
};

/// Where her verbs and readers reach her senses. Cheap to clone.
#[derive(Clone)]
pub struct Handle {
    senses: Teller<Senses>,
    /// Her thread of each room: told her memberships before her senses are.
    thread: Teller<crate::cognition::room::thread::Thread>,
    /// Her mind's bus, where her thread of each room is read.
    bus: Arc<Bus>,
    awareness: Reader<AwarenessSnapshot>,
    calling: Reader<Calling>,
    /// Where her time goes: her parts' measured numbers, read without holding them.
    profiler: continuum_sage::component::Profiler,
    /// What asks her dream for a pass (`mind/dream_asked`): one writer, shared by her callers.
    dream: Arc<parking_lot::Mutex<Emitter<crate::cognition::dream_consolidation::Ask>>>,
}

impl Handle {
    /// Her awareness strip as of `now_ms`: the newest one her senses published, read without waiting
    /// on them; `None` before her senses have taken anything in.
    pub fn awareness(&self, now_ms: u64) -> Option<AwarenessSnapshot> {
        // Published when something moved; read at the reader's time (never earlier), so a window
        // that closed since is read closed.
        let mut snapshot = (*self.awareness.latest()?.value).clone();
        snapshot.at_ms = snapshot.at_ms.max(now_ms);
        Some(snapshot)
    }

    /// When anything she perceives last moved (the newest timed event across her activities);
    /// `None` before any has.
    pub fn moved_at_ms(&self) -> Option<u64> {
        self.awareness.latest()?.value.lines.iter().map(|line| line.last_activity_ms).filter(|ms| *ms > 0).max()
    }

    /// What calls her now.
    pub fn calling(&self) -> Reader<Calling> {
        self.calling.clone()
    }

    /// Her thread of `room` as it is now: the revision a turn pins. `None` before she joined it.
    pub fn thread(&self, room: Uuid) -> Option<Arc<continuum_grounding::room_thread::RoomThread>> {
        let reader = self.bus.reader::<continuum_grounding::room_thread::RoomThread>(crate::cognition::room::thread::THREAD, Key::of(room));
        reader.latest().map(|snapshot| snapshot.value)
    }

    /// Hand her senses one of her acts; false when they have stopped. Her memberships reach her
    /// thread first, so a room she joins is threaded before her senses hear it.
    /// Ask her dream for a pass; heard by her dream part, never lost while a pass runs.
    pub fn ask_dream(&self, ask: crate::cognition::dream_consolidation::Ask) {
        self.dream.lock().emit(ask);
    }

    pub async fn tell(&self, act: Sensed) -> bool {
        if let Sensed::Joined(rooms) = &act {
            let rooms = rooms.iter().map(|(room, _)| *room).collect();
            if !self.thread.tell(crate::cognition::room::thread::Threading::Joined(rooms)).await {
                return false;
            }
        }
        self.senses.tell(act).await
    }
}

/// A resident: her identity, her handle, and the owner of her parts on her own mind's bus. Dropping
/// her takes her off the feed and stops every part.
pub struct Resident {
    persona: Uuid,
    handle: Handle,
    parts: Owner,
}

impl Resident {
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Her mind's bus: where her parts hear and say.
    pub fn bus(&self) -> &Bus {
        self.parts.bus()
    }

    /// Her need: the loudest of her parts', read now without waiting on any.
    pub fn need(&self) -> Need {
        self.parts.need()
    }

    /// Where her time goes: every part's measured waits and handle times, slowest-waiting first.
    pub fn profile(&self) -> Vec<(&'static str, continuum_sage::component::Telemetry)> {
        self.parts.profile()
    }

    /// Rest her: every part gives back what it holds and wakes on its next input.
    pub async fn rest(&self) -> bool {
        self.parts.rest().await
    }
}

impl Drop for Resident {
    fn drop(&mut self) {
        RESIDENTS.write().remove(&self.persona);
        crate::cognition::turn_ledger::presence::forget(self.persona); // her bus goes with her
    }
}

static RESIDENTS: LazyLock<RwLock<HashMap<Uuid, Handle>>> = LazyLock::new(Default::default);

/// Start her parts on her own mind's bus: her thread of each room, hearing the one feed and reading
/// a room's history through `reader` (her airc runtime; `None` where she has none here); her senses
/// on `region`, hearing what her thread took in; and her channel digest watching what her senses
/// publish. `agent_name` is how a line addresses her (`PersonaIdentity::mentions`).
pub fn start(
    persona: Uuid,
    agent_name: &str,
    mut region: PerceptionRegion,
    reader: Option<Arc<dyn crate::cognition::conversation::transcript_reader::AircTranscriptReader>>,
) -> Result<Resident, Taken> {
    let now = EpochMs::now().millis();
    // A saved continuation calls her with `Resume` first; asking the region once takes it.
    let resume_pending = matches!(region.wake_for(now), Some(crate::cognition::conversation::Wake::Resume));
    let bus = Arc::new(Bus::new());
    let calling = bus.reader::<Calling>(CALLING, Key::ONE);
    let awareness = bus.reader::<AwarenessSnapshot>(AWARENESS, Key::ONE);
    // Her thread starts with the rooms her saved mind state holds, so no line in them can reach the
    // feed before she threads it.
    let thread = crate::cognition::room::thread::Thread::new(Arc::clone(&bus), reader, UNHEARD, &region.activities())?;
    let senses = Senses {
        persona,
        peer: airc_core::PeerId::from_uuid(persona),
        identity: PersonaIdentity::new(persona, agent_name.to_string()),
        region,
        admission: None,
        calling: bus.publisher::<Calling>(CALLING, Key::ONE)?,
        published: Calling::default(),
        strip: bus.publisher::<AwarenessSnapshot>(AWARENESS, Key::ONE)?,
        strip_published: None,
        due_at: None,
        resume_pending,
        served_due: None,
    };
    let mut parts = Owner::with_bus(Arc::clone(&bus), OwnerId(persona), now);
    // Her senses first, so they are listening before her thread hands anything on.
    let senses = parts.start(senses);
    let thread = parts.start(thread);
    // Her channel digest; absent where no runtime registry exists to page her room from (a test).
    if let Some(registry) = crate::persona::PersonaAircRuntimeRegistry::try_global() {
        parts.start(digest::Digest {
            persona,
            builder: crate::cognition::channel_substrate::global_channel_digest_builder(),
            personas: Arc::new(registry),
            digests: crate::cognition::channel_substrate::global_channel_digest_buffer(),
            built: continuum_sage::component::LastSeen::default(),
        });
    }
    // Her dream (card d56dc734), woken by what her mind says on her bus: her presence (her turn
    // ledger), her experience (her admission store) and what asks it (`ask_dream`).
    crate::cognition::turn_ledger::presence::say_on(persona, &bus)?;
    let workspace = crate::cognition::persona_workspace::global();
    match workspace.reflector_handles(&persona) {
        Some((admission, _, _)) => admission.say_experience_on(&bus)?,
        None => continuum_base::probe!(class = "mind.dream.experience_unheard", persona = %persona,
            "her workspace is not registered yet: her dream wakes on her rest and what asks it, not on each experience"),
    }
    let dream = Arc::new(parking_lot::Mutex::new(bus.emitter::<crate::cognition::dream_consolidation::Ask>(crate::cognition::dream_consolidation::DREAM_ASKED)?));
    parts.start(crate::cognition::dream_consolidation::DreamPart::new(persona, workspace as Arc<dyn crate::cognition::dream_consolidation::PersonaReflectionSource>));
    let handle = Handle { senses, thread, bus, awareness, calling, profiler: parts.profiler(), dream };
    RESIDENTS.write().insert(persona, handle.clone());
    continuum_base::probe!(class = "mind.feed.registered", persona = %persona, "her senses are on the one feed");
    Ok(Resident { persona, handle, parts })
}

/// Every resident's parts' numbers, the longest-waiting part first in each.
pub fn profiles() -> Vec<(Uuid, Vec<(&'static str, continuum_sage::component::Telemetry)>)> {
    RESIDENTS.read().iter().map(|(persona, handle)| (*persona, handle.profiler.read())).collect()
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

/// Her resident dream, asked on her own bus; a mind not resident on this core is not asked (her
/// next rest or experience there wakes her dream, and an acceptance stays durable for its pass).
pub struct ResidentDreams;

impl crate::cognition::dream_consolidation::AskDream for ResidentDreams {
    fn ask(&self, persona: Uuid, ask: crate::cognition::dream_consolidation::Ask) {
        match of(persona) {
            Some(her) => her.ask_dream(ask),
            None => continuum_base::probe!(class = "mind.dream.not_resident", persona = %persona, ask = ?ask,
                "her dream was asked where she is not resident; her next pass there takes it from durable state"),
        }
    }
}
