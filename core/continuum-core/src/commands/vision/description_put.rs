//! `vision/description-put` — store a vision description and notify watchers. Mutates
//! the shared cache and publishes an event → `Privileged`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionPutParams, VisionPutResult};

crate::action_command! {
    /// Store a vision description under a content-addressed key (with optional
    /// model/provider/timing/confidence provenance). Publishes
    /// `vision:description:ready` so any TS consumer awaiting this key is notified —
    /// no polling. LRU-evicts the least-recently-used entry if the L1 cache is full.
    pub struct VisionDescriptionPut { cache: Arc<VisionCache> }
    name: "vision/description-put",
    access: Privileged,
    params: VisionPutParams,
    output: VisionPutResult,
    run(this, _ctx, p) => {
        Ok(this.cache.put(&p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a cache mutator + event publisher is Privileged.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionDescriptionPut::NAME, "vision/description-put");
        assert!(matches!(
            VisionDescriptionPut::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }
}
