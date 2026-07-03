//! `hf/search-models` — find base models on the HuggingFace Hub.

use super::{search_hub, HfSearchParams, HfSearchResult, HubKind};

crate::action_command! {
    /// Search HuggingFace Hub for MODELS by keyword — base models, fine-tunes,
    /// GGUF builds. Returns the top repos with downloads, likes, task and tags,
    /// each with its Hub URL and the `<owner>/<name>` id you can hand to
    /// `models/pull`. Use this to find a base model to forge from, to compare
    /// candidates before pulling, or to discover what exists for a task. Example
    /// query: "qwen2.5 coder gguf". Reliable public search — no key needed.
    pub struct HfSearchModels;
    name: "hf/search-models",
    access: AiSafe,
    params: HfSearchParams,
    output: HfSearchResult,
    run(_this, _ctx, p) => { search_hub(HubKind::Models, p).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name + non-empty model-facing description — the
    // persona is offered "hf/search-models" as a tool whose guidance points at
    // models (not datasets), and the name is the routing key cu/grid/tool-surface
    // bind to. A name drift here unwires the hand silently.
    #[test]
    fn name_and_description() {
        assert_eq!(HfSearchModels::NAME, "hf/search-models");
        assert!(HfSearchModels::DESCRIPTION.contains("MODELS"));
        assert!(HfSearchModels::DESCRIPTION.contains("models/pull"));
    }
}
