//! `ArtifactSource` for avatars — over the `AVATAR_CATALOG` (#1871). This is the
//! pattern the whole provisioning system generalizes from: a single-source catalog
//! carrying `{id, url, source_kind, license}` + a derived local path.

use super::{disk_state_of, ArtifactKind, ArtifactSource, ArtifactSpec, DiskState, SourceKind};
use crate::live::avatar::catalog::{avatar_model_path, AVATAR_CATALOG};

/// Provisions VRM/GLB avatars from `AVATAR_CATALOG`.
pub struct AvatarSource;

impl ArtifactSource for AvatarSource {
    fn kind(&self) -> ArtifactKind {
        ArtifactKind::Avatar
    }

    fn catalog(&self) -> Vec<ArtifactSpec> {
        AVATAR_CATALOG
            .iter()
            .map(|m| ArtifactSpec {
                id: m.id.to_string(),
                url: m.url.to_string(),
                source_kind: parse_source_kind(m.source_kind),
                // VRM sizes aren't pinned in the catalog; known once on disk.
                size_bytes: None,
                checksum: None,
                license: Some(m.license.to_string()),
            })
            .collect()
    }

    fn disk_state(&self, id: &str) -> DiskState {
        match AVATAR_CATALOG.iter().find(|m| m.id == id) {
            Some(m) => disk_state_of(avatar_model_path(m.filename)),
            None => DiskState::Absent,
        }
    }
}

/// Map the catalog's `source_kind` string onto the fetch strategy.
fn parse_source_kind(kind: &str) -> SourceKind {
    match kind {
        "vroid-zip" => SourceKind::Zip,
        _ => SourceKind::Direct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the avatar catalog projects cleanly onto ArtifactSpec — every
    // entry yields a non-empty id + url and a known source_kind, so the generic
    // Downloader/Provisioner can consume avatars with no avatar-specific code.
    #[test]
    fn avatar_source_projects_the_whole_catalog() {
        let src = AvatarSource;
        assert_eq!(src.kind(), ArtifactKind::Avatar);
        let specs = src.catalog();
        assert_eq!(specs.len(), AVATAR_CATALOG.len());
        assert!(specs.len() >= 8, "expected the 8 vroid avatars");
        for s in &specs {
            assert!(!s.id.is_empty(), "spec has no id");
            assert!(s.url.starts_with("http"), "{} has no url", s.id);
            assert_eq!(s.license.as_deref(), Some("CC0"));
        }
    }

    // what this catches: disk_state derives the path from the id (never hardcoded) and
    // reports Absent for an unknown id — the cache-reasoning primitive.
    #[test]
    fn avatar_disk_state_derives_and_reports_absent_for_unknown() {
        let src = AvatarSource;
        assert_eq!(
            src.disk_state("definitely-not-an-avatar-id"),
            DiskState::Absent
        );
        // A real catalog id resolves to a concrete path decision (present/absent
        // depending on whether it's been provisioned) — the point is it doesn't panic
        // and it's derived, not hardcoded.
        let real = AVATAR_CATALOG[0].id;
        let _ = src.disk_state(real);
    }
}
