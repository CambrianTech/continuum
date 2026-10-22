//! One-per-persona exclusion on a resident's hands, held by anything that works one of her
//! trees (workspace reclaim, cargo-target observation) so two never overlap. A machine fact
//! the kernel's pools take, so it lives here rather than in the work module above them.
//!
//! (Not the per-context `identity::hands_gate::HandsLease`, which is a different admission.)

/// One-per-persona hands lease: a workspace reclaim takes the resident's hands so no
/// two reclaims (or a reclaim and anything else that takes them) work one tree at once.
/// RAII: dropping the lease (any exit path, panics included) frees the persona.
pub struct PersonaHandsLease(uuid::Uuid);

fn busy_hands() -> &'static std::sync::Mutex<std::collections::HashSet<uuid::Uuid>> {
    static BUSY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
        std::sync::OnceLock::new();
    BUSY.get_or_init(Default::default)
}

impl PersonaHandsLease {
    /// Take this persona's hands, or `None` while another holder has them.
    pub fn try_take(persona: uuid::Uuid) -> Option<Self> {
        let mut g = busy_hands().lock().expect("busy-hands lock never poisoned"); // expect: guards a HashSet op only
        if g.insert(persona) {
            Some(Self(persona))
        } else {
            None
        }
    }
}

impl Drop for PersonaHandsLease {
    fn drop(&mut self) {
        if let Ok(mut g) = busy_hands().lock() {
            g.remove(&self.0);
        }
    }
}
