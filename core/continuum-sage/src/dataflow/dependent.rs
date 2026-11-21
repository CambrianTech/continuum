//! A stage that runs once every artifact it subscribes to has settled, and delivers its own.
//!
//! The act is a dependency graph (CONTINUUM-ENGINE.md section 2.4): a pass runs when its inputs
//! exist, never at a hand-written point in a sequence and never on a timer. A [`DependentStage`]
//! names what it produces and declares its inputs as a tuple of [`Subscribes`]; the substrate
//! ([`spawn_when_ready`]) gives it, once for every stage: its own task, the wake when the LAST
//! input settles, the inputs by handle, its output as an artifact any number of consumers
//! subscribe to (abandoned, never hung, if the run panics or is dropped), and its timing as a
//! probe.

use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use super::artifact::{artifact, Awaited, Subscribes};
use super::topic::TopicName;

/// What a dependent stage waits on: a tuple of subscriptions. Every input is waited until it
/// settles (ready, failed or abandoned); what a missing input means is the stage's to decide, from
/// its typed [`super::artifact::Settled`].
pub trait Inputs: Send + 'static {
    type Settled: Send + 'static;
    fn topics(&self) -> Vec<TopicName>;
    fn settled(self) -> impl Future<Output = Self::Settled> + Send;
}

impl Inputs for () {
    type Settled = ();
    fn topics(&self) -> Vec<TopicName> {
        Vec::new()
    }
    async fn settled(self) {}
}

macro_rules! inputs_for_tuples {
    ($(($($name:ident: $t:ident $f:ident),+)),+ $(,)?) => {$(
        impl<$($t: Send + Sync + 'static, $f: Send + Sync + 'static),+> Inputs for ($(Subscribes<$t, $f>,)+) {
            type Settled = ($(Awaited<Arc<$t>, Arc<$f>>,)+);
            fn topics(&self) -> Vec<TopicName> {
                let ($($name,)+) = self;
                vec![$($name.topic()),+]
            }
            async fn settled(self) -> Self::Settled {
                let ($(mut $name,)+) = self;
                tokio::join!($($name.settled()),+)
            }
        }
    )+};
}

inputs_for_tuples!((a: A FA), (a: A FA, b: B FB), (a: A FA, b: B FB, c: C FC));

/// A pass of the act graph: it runs ONCE, on its settled inputs, and produces one artifact. The
/// run consumes the stage, so what it was built with moves into the work, never copied.
#[async_trait]
pub trait DependentStage: Sized + Send + 'static {
    type Inputs: Inputs;
    type Produces: Send + Sync + 'static;
    type Fails: Send + Sync + 'static;
    /// The artifact it produces: its subscribers name it, and its probes carry it.
    fn produces(&self) -> TopicName;
    async fn run(
        self,
        inputs: <Self::Inputs as Inputs>::Settled,
    ) -> Result<Self::Produces, Self::Fails>;
}

/// Run `stage` on its own task once every input has settled; returns the artifact it will settle.
pub fn spawn_when_ready<S: DependentStage>(
    stage: S,
    inputs: S::Inputs,
) -> Subscribes<S::Produces, S::Fails> {
    let topic = stage.produces();
    let (produces, subscribes) = artifact(topic);
    let subscribed = inputs.topics();
    tokio::spawn(async move {
        let waiting = Instant::now();
        let settled = inputs.settled().await;
        let running = Instant::now();
        let result = stage.run(settled).await;
        continuum_base::probe!(
            class = "stage.dependent.ran",
            stage = topic.0,
            inputs = subscribed.len() as u64,
            waited_ms = running.duration_since(waiting).as_millis() as u64,
            ran_ms = running.elapsed().as_millis() as u64,
            produced = result.is_ok(),
            "a dependent stage ran on its settled inputs"
        );
        match result {
            Ok(value) => produces.produce(value),
            Err(why) => produces.fail(why),
        }
    });
    subscribes
}
