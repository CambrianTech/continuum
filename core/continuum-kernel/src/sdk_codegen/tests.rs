#![cfg(test)]
    use super::*;
    use std::path::Path;

    // what this catches (the review's CRITICAL #4): real ts-rs types carry the
    // `../../../protocol/typescript/...` export escape path; module_of must resolve
    // them to clean importable modules. Earlier demo types (no export_to) hid this.
    #[test]
    fn module_of_strips_the_ts_rs_export_escape_path() {
        assert_eq!(
            module_of(Path::new("../../../protocol/typescript/ai/ModelInfo.ts")),
            "ai/ModelInfo",
            "escape-hatch export_to → clean module relative to the TS root"
        );
        assert_eq!(
            module_of(Path::new(
                "../../../protocol/typescript/inference_llm/InferenceRequest.ts"
            )),
            "inference_llm/InferenceRequest"
        );
        // No export_to (bare derived name) → its stem.
        assert_eq!(module_of(Path::new("PingParams.ts")), "PingParams");
    }
