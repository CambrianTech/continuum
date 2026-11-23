//! A started component's handles: `Running` owns its life (dropping it stops the part), `Teller`
//! hands it acts from anywhere without owning anything.

use super::delivered::Delivered;
use super::Component;
use crate::{StageInput, StageWorker};

/// Tells a running part acts; holds no lifetime.
pub struct Teller<P: Component> {
    input: StageInput<Delivered<P::Input>>,
}

impl<P: Component> Clone for Teller<P> {
    fn clone(&self) -> Self {
        Self { input: self.input.clone() }
    }
}

impl<P: Component> Teller<P> {
    /// Hand the part an act; false once it has stopped.
    pub async fn tell(&self, input: P::Input) -> bool {
        self.input.send(Delivered::act(input)).await.is_ok()
    }

    /// Ask it to rest (its owner rests, or its memory is wanted elsewhere): it gives back what it
    /// holds, and its next input wakes it. False once it has stopped.
    pub async fn rest(&self) -> bool {
        self.input.send(Delivered::rest()).await.is_ok()
    }
}

/// A running part. Its owner tells it acts; dropping it stops the part.
pub struct Running<P: Component> {
    teller: Teller<P>,
    worker: StageWorker<Delivered<P::Input>>,
}

impl<P: Component> Running<P> {
    pub(super) fn new(input: StageInput<Delivered<P::Input>>, worker: StageWorker<Delivered<P::Input>>) -> Self {
        Self { teller: Teller { input }, worker }
    }

    /// Hand the part an act; false once it has stopped.
    pub async fn tell(&self, input: P::Input) -> bool {
        self.teller.tell(input).await
    }

    /// Ask it to rest; false once it has stopped.
    pub async fn rest(&self) -> bool {
        self.teller.rest().await
    }

    /// A handle to tell it acts from elsewhere, without owning its lifetime.
    pub fn teller(&self) -> Teller<P> {
        self.teller.clone()
    }
}

impl<P: Component> Drop for Running<P> {
    fn drop(&mut self) {
        self.worker.stop();
    }
}
