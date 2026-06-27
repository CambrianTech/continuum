//! `vision/description-get` — read a cached vision description by content key. A
//! pure cache read → `AiSafe`.

use std::sync::Arc;

use crate::modules::vision::{VisionCache, VisionGetResult, VisionKeyParams};

crate::action_command! {
    /// Look up a cached vision description by its content-addressed key (e.g. the
    /// SHA-256 of the image bytes). Returns `{found}` plus the description and its
    /// provenance (model/provider/timing/confidence) on a hit; `found=false` on a
    /// miss. Sub-millisecond L1 read — never triggers inference.
    pub struct VisionDescriptionGet { cache: Arc<VisionCache> }
    name: "vision/description-get",
    access: AiSafe,
    params: VisionKeyParams,
    output: VisionGetResult,
    run(this, _ctx, p) => {
        Ok(this.cache.get(&p.content_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: name/access wiring — a pure cache read is on the AiSafe surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VisionDescriptionGet::NAME, "vision/description-get");
        assert!(matches!(
            VisionDescriptionGet::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }
}
