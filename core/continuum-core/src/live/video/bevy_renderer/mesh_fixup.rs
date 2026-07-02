//! `mesh_fixup` — one-shot post-load mesh corrections for imported avatars.
//!
//! ## The black-skin bug (characterized against rendered frames, not the spec)
//!
//! VRoid exports some VRM meshes carrying a degenerate `COLOR_0` vertex-color
//! attribute of `(0,0,0,0)` on every vertex. VRM/MToon ignores glTF vertex
//! colors entirely, but Bevy's stock mesh pipeline multiplies
//! `base_color × vertex_color × texture` in the shader — so a zeroed `COLOR_0`
//! zeroes out every surface and the avatar renders BLACK.
//!
//! Empirically pinned: `vroid-male-base.vrm` (renders correctly) has **no**
//! `COLOR_0` attribute; `vroid-sakurada.vrm` (skin/face render black) has
//! `COLOR_0 = (0,0,0,0)` on every primitive. The material JSON is byte-for-byte
//! equivalent between them (white `baseColorFactor`, valid 8-bit RGBA texture,
//! `KHR_materials_unlit`, `metallic=0`) — the vertex-color attribute is the sole
//! structural difference. So the fix belongs at the **mesh** layer, not the
//! material layer.
//!
//! ## Why removal is correct, not a fallback
//!
//! An all-zero vertex-color attribute can ONLY ever multiply a surface to black;
//! it is never intended geometry. Removing it lets Bevy fall back to
//! `base_color × texture` (the correct, MToon-equivalent result). We remove ONLY
//! when the attribute is provably degenerate (every component of every vertex is
//! zero); a mesh with any non-zero vertex color is legitimate data and is left
//! untouched. An encoding we don't classify is logged and left intact — we never
//! silently strip something we didn't prove degenerate (fail loud).

use bevy::mesh::{Mesh, VertexAttributeValues};
use bevy::prelude::*;

use crate::clog_warn;

/// Walk a loaded scene's descendants and remove any degenerate (all-zero)
/// `COLOR_0` vertex-color attribute. Returns the number of distinct meshes
/// corrected (asset handles are de-duplicated so a shared mesh is fixed once).
pub(super) fn strip_degenerate_vertex_colors(
    root: Entity,
    children: &Query<&Children>,
    mesh_handles: &Query<&Mesh3d>,
    meshes: &mut Assets<Mesh>,
) -> usize {
    let mut fixed = 0;
    let mut seen = std::collections::HashSet::new();
    strip_recursive(root, children, mesh_handles, meshes, &mut fixed, &mut seen);
    fixed
}

fn strip_recursive(
    entity: Entity,
    children: &Query<&Children>,
    mesh_handles: &Query<&Mesh3d>,
    meshes: &mut Assets<Mesh>,
    fixed: &mut usize,
    seen: &mut std::collections::HashSet<AssetId<Mesh>>,
) {
    if let Ok(mesh3d) = mesh_handles.get(entity) {
        let id = mesh3d.0.id();
        // Meshes are shared assets — only inspect/mutate each once.
        if seen.insert(id) {
            if let Some(mesh) = meshes.get_mut(&mesh3d.0) {
                if let Some(colors) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
                    match vertex_colors_all_zero(colors) {
                        Some(true) => {
                            mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
                            *fixed += 1;
                        }
                        Some(false) => {} // legitimate vertex colors — keep
                        None => clog_warn!(
                            "mesh_fixup: COLOR_0 on {:?} has an unhandled encoding; \
                             left intact (add the variant to vertex_colors_all_zero)",
                            id
                        ),
                    }
                }
            }
        }
    }
    if let Ok(child_list) = children.get(entity) {
        for child in child_list.iter() {
            strip_recursive(child, children, mesh_handles, meshes, fixed, seen);
        }
    }
}

/// Classify a `COLOR_0` attribute's payload:
/// - `Some(true)`  = every component of every vertex is exactly zero (degenerate)
/// - `Some(false)` = at least one non-zero component (legitimate vertex colors)
/// - `None`        = an encoding we don't classify (caller logs, never strips)
///
/// Bevy's glTF loader normalizes vertex colors to `Float32x{3,4}` on import, so
/// those are the encodings the VRoid fleet actually produces.
fn vertex_colors_all_zero(colors: &VertexAttributeValues) -> Option<bool> {
    match colors {
        VertexAttributeValues::Float32x4(v) => Some(v.iter().all(|c| *c == [0.0, 0.0, 0.0, 0.0])),
        VertexAttributeValues::Float32x3(v) => Some(v.iter().all(|c| *c == [0.0, 0.0, 0.0])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the classifier that decides whether a COLOR_0 attribute
    // is the degenerate VRoid (0,0,0,0) black-skin payload (strip) vs legitimate
    // vertex colors (keep). Regression for the vroid-sakurada black-skin bug.
    #[test]
    fn all_zero_vertex_colors_are_degenerate() {
        let degenerate = VertexAttributeValues::Float32x4(vec![[0.0; 4], [0.0; 4], [0.0; 4]]);
        assert_eq!(vertex_colors_all_zero(&degenerate), Some(true));

        let degenerate3 = VertexAttributeValues::Float32x3(vec![[0.0; 3], [0.0; 3]]);
        assert_eq!(vertex_colors_all_zero(&degenerate3), Some(true));
    }

    // what this catches: stripping legitimate vertex colors (any non-zero vertex
    // means the attribute is real geometry data and must be preserved).
    #[test]
    fn any_nonzero_vertex_color_is_legitimate() {
        let legit = VertexAttributeValues::Float32x4(vec![[0.0; 4], [1.0, 0.5, 0.25, 1.0]]);
        assert_eq!(vertex_colors_all_zero(&legit), Some(false));
    }

    // what this catches: an unclassified encoding being treated as degenerate —
    // it must return None so the caller leaves it intact and logs, never strips
    // something we didn't prove is all-zero.
    #[test]
    fn unhandled_encoding_is_not_classified() {
        let other = VertexAttributeValues::Uint16x4(vec![[0, 0, 0, 0]]);
        assert_eq!(vertex_colors_all_zero(&other), None);
    }
}
