//! `scene::library` — CWD- and deployment-independent resolution of a scene
//! reference to a parsed [`SceneDescription`].
//!
//! A scene has TWO producers (the [`super::birther`] generates one procedurally
//! from an identity; an author commits a `.ron` file), and this resolver is the
//! file side. Same reliability story as [`crate::cognition::gym`]: a committed
//! scene must resolve identically whether the core is launched from the crate
//! dir, the repo root, or a deployed binary with no checkout — so a committed
//! scene is baked into the binary, and a bare `fs::read` is never trusted alone.
//!
//! ## The resolution contract (deterministic, fail-loud — no silent degrade)
//!
//! For a reference `r`, in order:
//!   1. If `r` names an **existing file on disk** → read + parse it. An author
//!      iterating on a custom scene (absolute path, or one that resolves from
//!      their CWD) keeps full control and overrides the embedded copy.
//!   2. Else if `r`'s basename is a **committed scene** baked into the binary →
//!      parse the embedded text. CWD-/deployment-independent.
//!   3. Else → **fail loud**, naming the reference AND every embedded scene. A
//!      typo'd or vanished scene never silently degrades to a default
//!      ([[fallbacks-are-illegal-fail-loud]]).
//!
//! After parsing, the schema `version` is validated: a scene authored against a
//! DIFFERENT [`SCENE_DESCRIPTION_VERSION`] fails loud rather than being
//! misread — we never guess how to reinterpret an unknown layout.

use std::path::Path;

use super::description::{SceneDescription, SCENE_DESCRIPTION_VERSION};

/// Every committed scene, baked into the binary: `(basename, ron_text)`. Adding
/// a committed scene is one `include_str!` line here — that is the single edit
/// that makes it referenceable from any CWD or a deployed binary. Keyed by
/// basename so a reference may be a repo path (`assets/scenes/portrait.ron`) or
/// a bare name (`portrait.ron`) and resolve to the same bytes.
///
/// Path convention when adding one (this file is 7 levels below the repo root):
/// `include_str!("../../../../../../../assets/scenes/<name>.ron")`.
///
/// Empty today: the birther is the live producer of scenes, so no authored
/// scene is committed yet. The resolver + its contract exist so the moment an
/// authored scene lands it is embed-resolvable with a one-line edit.
const EMBEDDED_SCENES: &[(&str, &str)] = &[];

/// Look up a committed scene's embedded text by the basename of `reference`.
fn embedded_for(reference: &str) -> Option<(&'static str, &'static str)> {
    let base = Path::new(reference)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(reference);
    EMBEDDED_SCENES
        .iter()
        .find(|(name, _)| *name == base)
        .copied()
}

/// Comma-joined list of every embedded scene basename, for fail-loud
/// diagnostics.
fn embedded_names() -> String {
    if EMBEDDED_SCENES.is_empty() {
        return "(none committed yet)".to_string();
    }
    EMBEDDED_SCENES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Parse RON text into a [`SceneDescription`] and validate its schema version.
fn parse_and_validate(origin: &str, text: &str) -> Result<SceneDescription, String> {
    let scene: SceneDescription =
        ron::from_str(text).map_err(|e| format!("scene '{origin}' is not valid scene RON: {e}"))?;
    if scene.version != SCENE_DESCRIPTION_VERSION {
        return Err(format!(
            "scene '{origin}' is schema version {found}, but this build reads \
             version {current}. Re-author or migrate the scene; the layout is \
             not guessed.",
            found = scene.version,
            current = SCENE_DESCRIPTION_VERSION,
        ));
    }
    Ok(scene)
}

/// Resolve a scene reference to `(origin_label, SceneDescription)`.
///
/// `origin_label` is for reporting only — the on-disk path read, or
/// `embedded:<basename>` for a baked committed scene. See the module docs for
/// the resolution order. Fails loud (never silently degrades) naming the
/// reference and every candidate tried.
pub fn resolve_scene(reference: &str) -> Result<(String, SceneDescription), String> {
    // (1) An existing on-disk file wins — a custom scene the author points at.
    if Path::new(reference).is_file() {
        let text = std::fs::read_to_string(reference)
            .map_err(|e| format!("scene '{reference}' exists but could not be read: {e}"))?;
        let scene = parse_and_validate(reference, &text)?;
        return Ok((reference.to_string(), scene));
    }
    // (2) A committed scene baked into the binary — CWD-/deployment-independent.
    if let Some((name, text)) = embedded_for(reference) {
        let scene = parse_and_validate(&format!("embedded:{name}"), text)?;
        return Ok((format!("embedded:{name}"), scene));
    }
    // (3) Neither — fail loud with everything tried.
    Err(format!(
        "scene '{reference}' could not be resolved: no such file on disk \
         (cwd={cwd}), and it is not a committed scene. Committed scenes: {names}.",
        cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unknown>".to_string()),
        names = embedded_names(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::video::bevy_renderer::scene::description::{
        ColorDesc, NodePayload, SceneNode,
    };

    fn sample_scene() -> SceneDescription {
        SceneDescription {
            version: SCENE_DESCRIPTION_VERSION,
            backdrop: ColorDesc::BLACK,
            root: SceneNode::group("root")
                .with_child(SceneNode::group("a"))
                .with_child(SceneNode::leaf("cam", NodePayload::Group)),
        }
    }

    // what this catches: the on-disk override branch failing to read+parse a
    // real RON file — the author's custom-scene path silently breaking.
    #[test]
    fn resolves_an_on_disk_scene_file() {
        let dir = std::env::temp_dir().join(format!("scene_lib_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mine.ron");
        std::fs::write(&path, ron::ser::to_string(&sample_scene()).unwrap()).unwrap();

        let (origin, scene) = resolve_scene(path.to_str().unwrap()).expect("should resolve");
        assert_eq!(origin, path.to_str().unwrap());
        assert_eq!(scene, sample_scene());

        std::fs::remove_dir_all(&dir).ok();
    }

    // what this catches: a vanished/typo'd reference silently degrading to a
    // default instead of failing loud — the fallbacks-are-illegal invariant.
    #[test]
    fn unknown_reference_fails_loud() {
        let err = resolve_scene("does/not/exist/nope.ron").unwrap_err();
        assert!(
            err.contains("nope.ron"),
            "error must name the reference: {err}"
        );
        assert!(
            err.contains("Committed scenes"),
            "error must list candidates: {err}"
        );
    }

    // what this catches: a scene authored against a different schema version
    // being misread instead of rejected.
    #[test]
    fn wrong_version_fails_loud() {
        let dir = std::env::temp_dir().join(format!("scene_lib_ver_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("future.ron");
        let mut scene = sample_scene();
        scene.version = SCENE_DESCRIPTION_VERSION + 1;
        std::fs::write(&path, ron::ser::to_string(&scene).unwrap()).unwrap();

        let err = resolve_scene(path.to_str().unwrap()).unwrap_err();
        assert!(
            err.contains("schema version"),
            "error must cite the version: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
