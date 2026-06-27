//! `vision/description-status` — report whether a content key is cached. A pure read
//! → `AiSafe`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionKeyParams, VisionStatusResult};

crate::action_command! {
    /// Report the cache status of a content-addressed key: `cached` if a description
    /// is present in L1, `none` otherwise. A pure read used to decide whether to
    /// trigger inference.
    pub struct VisionDescriptionStatus { cache: Arc<VisionCache> }
    name: "vision/description-status",
    access: AiSafe,
    params: VisionKeyParams,
    output: VisionStatusResult,
    run(this, _ctx, p) => {
        Ok(this.cache.status(&p.content_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a pure read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionDescriptionStatus::NAME, "vision/description-status");
        assert!(matches!(
            VisionDescriptionStatus::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
