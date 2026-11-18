//! [`NextPolicy`]: the choice of the next mind, pluggable and learnable (an algorithm behind a
//! trait, OpenCV-style), never a fixed rotation baked into the pipeline.

use crate::persona::PersonaId;

pub trait NextPolicy {
    /// The mind to stage next from those waiting, given who is active now; `None` when nobody
    /// should be staged yet.
    fn choose(&mut self, waiting: &[PersonaId], active: Option<PersonaId>) -> Option<PersonaId>;
}
