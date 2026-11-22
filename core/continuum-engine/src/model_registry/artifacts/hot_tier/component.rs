//! The hot tier as a SAGE component on the node's bus: it watches which model the node serves, and
//! when that model sits on a slower drive than the home's it copies it up in the background while
//! serving goes on. The next launch reads the copy (the resolver prefers it), so nothing waits on a
//! copy and nothing relaunches for one. Where the served model is read from is published as state
//! (`serving/placement`) for anything to read: the health line, a peer, a human's screen.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use continuum_sage::component::{node, start, Component, Ctx, Key, Known, LastSeen, Novelty, Running, Urgency, Wiring};
use continuum_sage::dataflow::{OwnerId, Provenance, Publisher, RunId, Snapshot, StageId, StageIncarnation, TopicName};
use continuum_sage::Qos;

use super::Promotion;

/// The model this node serves now: its stored GGUF, as the launcher resolved it.
pub const SERVING: TopicName = TopicName("serving/model");
/// Where the served model is read from: the hot tier's last decision.
pub const PLACEMENT: TopicName = TopicName("serving/placement");

/// The stored GGUF of the model being served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServedModel(pub PathBuf);

/// The served model, as the hot tier is handed it.
pub struct Served(Arc<ServedModel>);

impl From<Snapshot<ServedModel>> for Served {
    fn from(snapshot: Snapshot<ServedModel>) -> Self {
        Served(snapshot.value)
    }
}

struct HotTier {
    placed: Publisher<Promotion>,
    /// The stored model it last placed: a launch of the same model asks nothing again.
    seen: LastSeen<PathBuf>,
}

impl Component for HotTier {
    type Input = Served;
    const NAME: &'static str = "hot-tier";
    const QOS: Qos = Qos::Utility;

    fn wire(&self, wiring: &mut Wiring<'_, Self>) {
        wiring.watch::<ServedModel>(SERVING, Key::ONE);
    }

    async fn handle(&mut self, Served(served): Served, ctx: &mut Ctx<Self>) {
        let stored = served.0.clone();
        if self.seen.novelty(&stored) == Novelty::KNOWN {
            ctx.need(Urgency::Content);
            return;
        }
        ctx.need(Urgency::Want);
        // A whole model's bytes: on the blocking pool, this worker waits for it, serving does not.
        let copying = stored.clone();
        let promotion = match tokio::task::spawn_blocking(move || super::promote(&copying)).await {
            Ok(promotion) => promotion,
            Err(join) => Promotion::Failed { stored: stored.clone(), why: join.to_string() },
        };
        ctx.publish(&mut self.placed, promotion);
        self.seen.learn(&stored);
        ctx.need(Urgency::Content);
    }
}

struct Launcher {
    served: Publisher<ServedModel>,
    _tier: Option<Running<HotTier>>,
}

static LAUNCHER: OnceLock<parking_lot::Mutex<Option<Launcher>>> = OnceLock::new();

/// The launcher says which model it serves; the hot tier (started with the first say) places it.
/// Never waits: a copy, if one is due, runs on the hot tier's own worker.
pub fn serve(stored: &Path) {
    let launcher = LAUNCHER.get_or_init(|| {
        let bus = node();
        let launcher = bus.publisher::<ServedModel>(SERVING, Key::ONE).ok().map(|served| {
            let tier = bus.publisher::<Promotion>(PLACEMENT, Key::ONE).ok().map(|placed| {
                start(HotTier { placed, seen: LastSeen::default() }, bus, OwnerId(uuid::Uuid::nil()), 0)
            });
            Launcher { served, _tier: tier }
        });
        parking_lot::Mutex::new(launcher)
    });
    let mut guard = launcher.lock();
    let Some(launcher) = guard.as_mut() else {
        continuum_base::probe!(class = "models.hot.unclaimed", stored = %stored.display(), "the serving topic was claimed elsewhere; the hot tier is not placing this model");
        return;
    };
    let by = Provenance {
        by: RunId { incarnation: StageIncarnation { stage: StageId::named("serving-launch"), owner: OwnerId(uuid::Uuid::nil()), epoch: 0 }, seq: 0 },
        from: Vec::new(),
    };
    launcher.served.publish(ServedModel(stored.to_path_buf()), by); // the topic keeps its own copy
}
