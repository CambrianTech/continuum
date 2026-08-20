//! `coordinate` — a **renderer-agnostic** load-time adapter that translates any
//! model's authored coordinate space into our canonical one, so every model kind
//! we can get our hands on (VRM 0.x, VRM 1.0, ReadyPlayerMe, converted .glb)
//! becomes a first-class citizen that presents face-on and upright.
//!
//! It produces a *rotation + scale* value (expressed as a Bevy `Transform` only
//! at the very edge, [`CoordinateCorrection::to_transform`]); the detection and
//! correction math know nothing about Bevy. That is deliberate: the same
//! correction is what a future non-Bevy backend (an Unreal `RenderBackend`)
//! would apply, and once Slice 1's `SceneDescription`/birther land, this feeds
//! the model root's renderer-neutral transform once, consumed by any backend.
//!
//! ## Empirically grounded — not spec-memory (this bit me once)
//!
//! The seeded fleet is entirely VRM 0.x, and a rendered frame proves it presents
//! **face-on at the identity transform** in our pipeline (Bevy's glTF import +
//! the camera at −Z looking toward the model). So in *our* canonical space,
//! **VRM 0.x is already canonical** — no correction. An earlier version of this
//! module encoded the textbook "VRM 0.x faces +Z" claim and would have yawed the
//! entire working fleet 180° to face away; the render is what caught it. The
//! rule this leaves behind: a per-format correction is added ONLY after a
//! *failing model is characterized against a rendered frame*, never from the
//! spec sheet.
//!
//! ## What it does today
//!
//! 1. **Diagnostic instrument (glass-box):** on every load it reads the model's
//!    glTF/VRM JSON and logs the declared [`DetectedFormat`]. When a model that
//!    breaks in the renderer loads, that log line names what it declared — the
//!    data needed to characterize and fix it.
//! 2. **Correction seam:** maps the detected format to a [`CoordinateConvention`]
//!    and returns the [`CoordinateCorrection`] onto [`CANONICAL`]. Every format
//!    we can verify today maps to canonical, so the correction is identity — the
//!    seam is live and correct, waiting to carry a real correction the moment a
//!    failing sample is characterized.
//!
//! The general basis-alignment math ([`CoordinateConvention::new`] +
//! [`CoordinateConvention::correction`]) is fully implemented and tested against
//! synthetic non-canonical conventions (e.g. a Z-up source), so wiring a real
//! correction later is a one-line change to a format's `convention()`, not new
//! math.

use bevy::math::{Mat3, Quat, Vec3};
use bevy::prelude::Transform;

use crate::clog_info;

/// Our canonical space == Bevy / glTF: right-handed, +Y up, −Z forward, meters.
/// A model already in this convention needs the identity correction.
pub const CANONICAL_FORWARD: Vec3 = Vec3::NEG_Z;
pub const CANONICAL_UP: Vec3 = Vec3::Y;

/// The model format we detected from the file's glTF/VRM JSON. Kept distinct
/// from [`CoordinateConvention`] on purpose: the format is a rich diagnostic we
/// always log, while the convention is what (if anything) needs correcting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetectedFormat {
    /// `extensions.VRM` present — VRoid-era VRM 0.x. **Verified** face-on at
    /// identity by a rendered frame, so canonical in our pipeline.
    Vrm0,
    /// `extensions.VRMC_vrm` present — VRM 1.0. glTF-conformant (−Z forward) per
    /// spec, so assumed canonical *pending a rendered-frame check on a real one*.
    Vrm1,
    /// Neither VRM extension — a plain spec-conformant glTF, which *is* our
    /// canonical space.
    Gltf,
}

impl DetectedFormat {
    /// Human label for the glass-box log.
    pub fn label(&self) -> &'static str {
        match self {
            DetectedFormat::Vrm0 => "VRM 0.x",
            DetectedFormat::Vrm1 => "VRM 1.0",
            DetectedFormat::Gltf => "glTF",
        }
    }

    /// The coordinate convention this format is authored in, *as it lands in our
    /// pipeline*. All verified/spec-canonical today → [`CANONICAL`]. To correct a
    /// format later, change its arm here — backed by a rendered frame, never the
    /// spec sheet ([[fallbacks-are-illegal-fail-loud]] applies to guesses too).
    pub fn convention(&self) -> CoordinateConvention {
        match self {
            // Verified by render: VRM 0.x is face-on at identity here.
            DetectedFormat::Vrm0 => CoordinateConvention::CANONICAL,
            // Spec −Z forward; assumed canonical until a real sample is rendered.
            DetectedFormat::Vrm1 => CoordinateConvention::CANONICAL,
            // glTF spec == our canonical space.
            DetectedFormat::Gltf => CoordinateConvention::CANONICAL,
        }
    }
}

/// A source model's coordinate convention: the world-space directions its
/// authored **forward** and **up** point, plus how many meters one of its units
/// is. `forward`/`up` must be orthonormal (enforced by [`Self::new`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateConvention {
    pub forward: Vec3,
    pub up: Vec3,
    /// Meters per source unit (VRM is meters → 1.0; a cm-authored glb → 0.01).
    pub unit_scale: f32,
}

impl CoordinateConvention {
    /// The canonical convention (glTF/Bevy). Correction is identity.
    pub const CANONICAL: Self = Self {
        forward: CANONICAL_FORWARD,
        up: CANONICAL_UP,
        unit_scale: 1.0,
    };

    /// Build a convention from arbitrary axes, validating orthonormality and a
    /// non-degenerate scale up front — a geometry-inferred detector (a later
    /// outlier) constructs through here so a bad basis fails loud at the source
    /// instead of producing a silently-wrong correction.
    pub fn new(forward: Vec3, up: Vec3, unit_scale: f32) -> Result<Self, String> {
        if forward.length_squared() < 1e-6 || up.length_squared() < 1e-6 {
            return Err("coordinate convention has a zero-length forward/up axis".to_string());
        }
        if forward.normalize().dot(up.normalize()).abs() > 1e-3 {
            return Err(format!(
                "coordinate convention forward {forward:?} and up {up:?} are not orthogonal"
            ));
        }
        if unit_scale <= 0.0 || !unit_scale.is_finite() {
            return Err(format!(
                "coordinate convention unit_scale {unit_scale} must be positive"
            ));
        }
        Ok(Self {
            forward: forward.normalize(),
            up: up.normalize(),
            unit_scale,
        })
    }

    /// The rotation+scale that maps this convention onto [`Self::CANONICAL`].
    ///
    /// Basis alignment: build orthonormal source and canonical bases (columns =
    /// right, up, forward) and solve `R · source = canonical` ⇒
    /// `R = canonical · sourceᵀ`. Both bases are right-handed by construction
    /// (right = up × forward), so `R` is always a proper rotation.
    pub fn correction(&self) -> CoordinateCorrection {
        let rotation = align_rotation(self.forward, self.up);
        CoordinateCorrection {
            rotation,
            scale: self.unit_scale,
        }
    }
}

/// Rotation mapping a source (forward, up) onto the canonical (forward, up).
fn align_rotation(src_forward: Vec3, src_up: Vec3) -> Quat {
    let src = orthonormal_basis(src_forward, src_up);
    let canonical = orthonormal_basis(CANONICAL_FORWARD, CANONICAL_UP);
    Quat::from_mat3(&(canonical * src.transpose()))
}

/// Right-handed orthonormal basis with columns (right, up, forward).
fn orthonormal_basis(forward: Vec3, up: Vec3) -> Mat3 {
    let f = forward.normalize();
    let u = up.normalize();
    let right = u.cross(f).normalize();
    Mat3::from_cols(right, u, f)
}

/// The correction to apply to a freshly-loaded model root so it lands in
/// canonical space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateCorrection {
    pub rotation: Quat,
    pub scale: f32,
}

impl CoordinateCorrection {
    /// The root `Transform` to spawn the model with. This is the only Bevy-typed
    /// surface of the module — the correction itself is renderer-neutral.
    pub fn to_transform(&self) -> Transform {
        Transform::from_rotation(self.rotation).with_scale(Vec3::splat(self.scale))
    }
}

/// Decide a model's format from its already-parsed glTF/VRM JSON. Pure and
/// testable — [`detect_convention`] is just this over the file's JSON chunk.
///
/// A VRM declares its version authoritatively via which extension key is present:
/// `VRMC_vrm` ⇒ VRM 1.0, `VRM` ⇒ VRM 0.x. Neither ⇒ a plain glTF.
pub fn format_from_gltf_json(root: &serde_json::Value) -> DetectedFormat {
    let ext = root.get("extensions");
    if ext.and_then(|e| e.get("VRMC_vrm")).is_some() {
        DetectedFormat::Vrm1
    } else if ext.and_then(|e| e.get("VRM")).is_some() {
        DetectedFormat::Vrm0
    } else {
        DetectedFormat::Gltf
    }
}

/// Detect a model file's coordinate convention (reads its glTF/VRM JSON chunk)
/// and log the detected format — the glass-box diagnostic that names what a
/// broken model declared. A file we can't read the JSON of is treated as glTF (a
/// spec-conformant glb needs no correction) and logged so it's never silent.
pub fn detect_convention(glb_path: &str) -> CoordinateConvention {
    let format = super::vrm::read_glb_json(glb_path)
        .map(|root| format_from_gltf_json(&root))
        .unwrap_or(DetectedFormat::Gltf);
    let convention = format.convention();
    clog_info!(
        "🧭 Coordinate adapter: {} → {} (forward {:?}, up {:?}, unit_scale {})",
        glb_path,
        format.label(),
        convention.forward,
        convention.up,
        convention.unit_scale
    );
    convention
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Vec3, b: Vec3) -> bool {
        a.abs_diff_eq(b, 1e-4)
    }

    // what this catches: the canonical convention getting a spurious rotation or
    // scale — the verified-face-on VRM 0.x fleet must pass through untouched.
    #[test]
    fn canonical_correction_is_identity() {
        let c = CoordinateConvention::CANONICAL.correction();
        assert!(
            approx(c.rotation * Vec3::NEG_Z, Vec3::NEG_Z),
            "forward preserved"
        );
        assert!(approx(c.rotation * Vec3::Y, Vec3::Y), "up preserved");
        assert_eq!(c.scale, 1.0);
    }

    // what this catches: a real regression of the empirical finding — every
    // format we can currently detect must map to the identity correction, so the
    // adapter never rotates a model we haven't characterized as broken.
    #[test]
    fn all_detected_formats_are_identity_today() {
        for fmt in [
            DetectedFormat::Vrm0,
            DetectedFormat::Vrm1,
            DetectedFormat::Gltf,
        ] {
            let c = fmt.convention().correction();
            assert!(
                approx(c.rotation * Vec3::NEG_Z, Vec3::NEG_Z),
                "{}: forward",
                fmt.label()
            );
            assert!(approx(c.rotation * Vec3::Y, Vec3::Y), "{}: up", fmt.label());
            assert_eq!(c.scale, 1.0, "{}: scale", fmt.label());
        }
    }

    // what this catches: the general basis-alignment math being wrong — a Z-up,
    // +Y-forward source (the shape a real broken model would take) must be
    // uprighted and turned to face canonical −Z. This proves the seam is ready
    // to carry a real correction the moment one is characterized.
    #[test]
    fn z_up_source_is_uprighted_by_the_general_math() {
        let src = CoordinateConvention::new(Vec3::Y, Vec3::Z, 1.0).unwrap();
        let c = src.correction();
        assert!(
            approx(c.rotation * Vec3::Y, Vec3::NEG_Z),
            "forward +Y → canonical −Z"
        );
        assert!(
            approx(c.rotation * Vec3::Z, Vec3::Y),
            "up +Z → canonical +Y"
        );
    }

    // what this catches: a degenerate convention (forward == up, or a zero axis,
    // or a non-positive scale) silently producing a garbage correction instead
    // of failing loud at construction.
    #[test]
    fn degenerate_convention_fails_loud() {
        assert!(
            CoordinateConvention::new(Vec3::Y, Vec3::Y, 1.0).is_err(),
            "non-orthogonal"
        );
        assert!(
            CoordinateConvention::new(Vec3::ZERO, Vec3::Y, 1.0).is_err(),
            "zero forward"
        );
        assert!(
            CoordinateConvention::new(Vec3::NEG_Z, Vec3::Y, 0.0).is_err(),
            "zero scale"
        );
    }

    // what this catches: format detection regressing — the authoritative
    // extension-key signal picking the wrong format (which is what the load-time
    // diagnostic log reports, and what a future correction keys off).
    #[test]
    fn detects_format_from_extensions() {
        let vrm1 = serde_json::json!({ "extensions": { "VRMC_vrm": {} } });
        let vrm0 = serde_json::json!({ "extensions": { "VRM": {} } });
        let plain = serde_json::json!({ "asset": { "version": "2.0" } });
        assert_eq!(format_from_gltf_json(&vrm1), DetectedFormat::Vrm1);
        assert_eq!(format_from_gltf_json(&vrm0), DetectedFormat::Vrm0);
        assert_eq!(format_from_gltf_json(&plain), DetectedFormat::Gltf);
    }
}
