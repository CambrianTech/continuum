//! A persona's identity and specialty as a turn carries it.

use uuid::Uuid;

/// Minimal persona handle: identity + specialty, not the full UserEntity.
#[derive(Debug, Clone)]
pub struct PersonaSlot {
    pub persona_id: Uuid,
    /// Stable specialty identifier (e.g. 'code', 'general').
    pub specialty: String,
    /// Human-readable name.
    pub display_name: String,
}
