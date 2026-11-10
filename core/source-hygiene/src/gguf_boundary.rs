//! GGUF metadata interpretation belongs to the typed artifact decoder.
//! This rule governs our engine readers, not the upstream model constructor.
//! Fingerprinting enumerates metadata for canonical identity; it does not decode
//! dimensions or supply missing values.
//! Scope is explicitly engine-only. A non-GGUF field named metadata will also
//! be flagged here; filesystem metadata() methods are distinct. Other crates
//! migrate through their owners before this invariant expands to them.
use crate::{SourceFile, SourceRule, Violation};

const ENGINE: &str = "core/continuum-engine/src/";
const DECODERS: &[&str] = &[
    "inference_capability/gguf_keys/architecture.rs",
    "inference_capability/gguf_keys/common.rs",
    "inference_capability/gguf_keys/residency.rs",
    "inference_capability/gguf_keys/split_schema.rs",
    "inference_capability/gguf_keys/projector_schema.rs",
    "inference_capability/gguf_keys/parameters_schema.rs",
];

pub(crate) fn is_decoder(file: &SourceFile) -> bool {
    file.repo.strip_prefix(ENGINE).is_some_and(|path| DECODERS.contains(&path))
}

pub struct GgufMetadataOwner;

impl SourceRule for GgufMetadataOwner {
    fn name(&self) -> &'static str { "gguf_metadata_owner" }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let Some(path) = file.repo.strip_prefix(ENGINE) else { return Vec::new(); };
        if is_decoder(file)
            || path == "inference_capability/gguf_keys.rs"
            || path.split('/').any(|part| part == "vendored")
            || path == "inference_capability/gguf_fingerprint.rs"
        {
            return Vec::new();
        }
        let tokens = crate::lexer::tokens::tokenize(&file.production);
        tokens.code.windows(3)
            .filter(|triple| triple[0].is('.')
                && (triple[1].is_ident("metadata") || triple[1].is_ident("r#metadata"))
                && !triple[2].is('('))
            .map(|triple| Violation {
                rule: self.name(),
                file: file.repo.clone(),
                line: triple[1].line,
                source: "raw GGUF metadata field outside the artifact decoder".into(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, source: &str) -> SourceFile {
        let mut file = SourceFile::fixture(path, source, source);
        file.repo = format!("{ENGINE}{path}");
        file
    }

    #[test]
    fn own_engine_readers_use_the_typed_boundary() {
        let violations = crate::scan(&[&GgufMetadataOwner]);
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn catches_multiline_alias_and_raw_identifier_access() {
        for source in [
            "ct.metadata.get(key)",
            "let md = &ct
 .metadata;",
            "let md = ct . r#metadata;",
        ] {
            assert_eq!(GgufMetadataOwner.check(&file("model_registry/hydrate.rs", source)).len(), 1);
        }
        for source in [
            "file.metadata()?",
            "// ct.metadata.get(key)",
            r#"let note = "ct.metadata.get(key)";"#,
        ] {
            assert!(GgufMetadataOwner.check(&file("model_registry/hydrate.rs", source)).is_empty());
        }
    }

    #[test]
    fn decoder_identity_is_exact_and_crate_bound() {
        assert!(!is_decoder(&file("inference_capability/gguf_keys.rs", "")));
        assert!(is_decoder(&file("inference_capability/gguf_keys/split_schema.rs", "")));
        assert!(!is_decoder(&file("inference_capability/gguf_keys/new_consumer.rs", "")));
        let mut foreign = file("inference_capability/gguf_keys.rs", "");
        foreign.repo = "core/other/src/inference_capability/gguf_keys.rs".into();
        assert!(!is_decoder(&foreign));
        assert!(!is_decoder(&file("inference_capability/gguf_fingerprint.rs", "")));
    }
}
