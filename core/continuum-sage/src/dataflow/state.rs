//! A state topic: the latest value of something, with its revision and the run that produced it.
//!
//! CBAR's invalidation (object model 5.7, "built as CBAR"): a change wakes exactly what depends on
//! it, and a reader never waits on the writer. One [`Publisher`] owns the topic (2.6: one writer);
//! any number of [`Reader`]s read the newest value by handle (`Arc`, never a copy). A worker that
//! depends on the state is [`attach`](Reader::attach)ed: it is handed the current value, then each
//! change. A change made while the worker is still busy with the previous one is coalesced into the
//! newest, because only the latest of a state matters. When the publisher goes away the readers see
//! [`Ended`], and an attached worker loses that producer (a stage ends when its last producer does).

use std::sync::Arc;

use tokio::sync::watch;

use super::topic::{InputRevision, Provenance, TopicName};
use crate::StageInput;

/// One published value: shared by handle, with the revision a consumer cites in its own provenance.
#[derive(Debug)]
pub struct Snapshot<T> {
    pub value: Arc<T>,
    pub revision: InputRevision,
    pub provenance: Provenance,
}

impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self { value: Arc::clone(&self.value), revision: self.revision, provenance: self.provenance.clone() }
    }
}

/// The publisher went away: no newer value will come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended;

/// The topic's one writer. Not `Clone`: a state has one owner.
pub struct Publisher<T> {
    topic: TopicName,
    published: u64,
    tx: watch::Sender<Option<Snapshot<T>>>,
}

/// A reader of the newest value. Cheap to clone.
pub struct Reader<T> {
    rx: watch::Receiver<Option<Snapshot<T>>>,
}

impl<T> Clone for Reader<T> {
    fn clone(&self) -> Self {
        Self { rx: self.rx.clone() }
    }
}

/// A state topic with nothing published yet.
pub fn state<T: Send + Sync + 'static>(topic: TopicName) -> (Publisher<T>, Reader<T>) {
    let (tx, rx) = watch::channel(None);
    (Publisher { topic, published: 0, tx }, Reader { rx })
}

impl<T: Send + Sync + 'static> Publisher<T> {
    /// Replace the value; every reader sees it, every attached worker is woken. Returns its revision.
    pub fn publish(&mut self, value: T, provenance: Provenance) -> InputRevision {
        self.published += 1;
        let revision = InputRevision { topic: self.topic, through: self.published };
        self.tx.send_replace(Some(Snapshot { value: Arc::new(value), revision, provenance }));
        revision
    }

    pub fn reader(&self) -> Reader<T> {
        Reader { rx: self.tx.subscribe() }
    }
}

impl<T: Send + Sync + 'static> Reader<T> {
    /// The newest value, if anything was published. Never waits on the writer.
    pub fn latest(&self) -> Option<Snapshot<T>> {
        self.rx.borrow().clone()
    }

    /// The next value this reader has not seen; the newest one when several were published.
    pub async fn changed(&mut self) -> Result<Snapshot<T>, Ended> {
        loop {
            self.rx.changed().await.map_err(|_| Ended)?;
            let newest = self.rx.borrow_and_update().clone();
            if let Some(snapshot) = newest {
                return Ok(snapshot);
            }
        }
    }

    /// Wake the worker behind `input` with the current value and then each change, until the
    /// publisher or the worker goes away. The delivery waits for the worker's room (backpressure),
    /// and changes made meanwhile coalesce into the newest.
    pub fn attach<I>(mut self, input: &StageInput<I>)
    where
        I: From<Snapshot<T>> + Send + 'static,
    {
        let input = input.clone(); // this delivery keeps its own handle to the worker
        tokio::spawn(async move {
            let current = self.rx.borrow_and_update().clone(); // the borrow ends before the await
            if let Some(current) = current {
                if input.send(I::from(current)).await.is_err() {
                    return;
                }
            }
            while let Ok(snapshot) = self.changed().await {
                if input.send(I::from(snapshot)).await.is_err() {
                    return;
                }
            }
        });
    }
}
