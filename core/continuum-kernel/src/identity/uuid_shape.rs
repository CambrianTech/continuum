//! The one 36-char `8-4-4-4-12` hex shape check for citizen/persona ids (no uuid parse).

/// Check if a string matches the 36-char UUID shape `8-4-4-4-12` hex.
/// Intentionally simple — avoids pulling uuid crate just for a shape check.
/// The data layer's `resolve_handle` and `memory/*` derive the SAME per-persona handle
/// mapping from it (one shape check, one place).
pub fn is_uuid_shape(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    s.chars().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => c == '-',
        _ => c.is_ascii_hexdigit(),
    })
}
