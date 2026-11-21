//! The root of every component's lineage (MODULE-ARCHITECTURE.md §9.4, §12): a part is a thing
//! that is someone. Each generation below adds only its difference: a model backend adds loading
//! and release, a speech backend adds speech, a leaf adds its device. What every part shares is
//! written here once.

/// Who a part is: the name callers address it by and what it is, in a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub name: &'static str,
    pub description: &'static str,
}

/// A component with an identity. Every long-lived component descends from it, directly or through
/// the generations between.
pub trait Part {
    fn identity(&self) -> Identity;
}
