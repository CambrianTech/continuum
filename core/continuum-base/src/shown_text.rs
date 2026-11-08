//! The displayed short form of an id that arrives as text: a transport identity, a call id,
//! a speaker id. Every log line and label that names one shows its leading
//! [`SHORT_ID_LEN`](crate::id_resolve::SHORT_ID_LEN) characters, the width every surface shows
//! a typed id at (`airc_core::shown_id::shown_form` renders a `Uuid`; this renders a `&str`).
//!
//! One owner for what forty call sites spelled `&id[..8.min(id.len())]`: a byte slice that
//! panics when the eighth byte falls inside a multi-byte character. This cuts on a character
//! boundary, so any text shortens and none panics.

use crate::id_resolve::SHORT_ID_LEN;

/// The leading [`SHORT_ID_LEN`] characters of `id`, or all of it when shorter.
pub fn shown_text(id: &str) -> &str {
    match id.char_indices().nth(SHORT_ID_LEN) {
        Some((end, _)) => &id[..end],
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::shown_text;

    // what this catches: the byte slice it replaced panicked on a multi-byte character at the
    // cut and showed a different width than a typed id's shown form. ASCII ids show their
    // first 8 characters, short ids show whole, and a non-ASCII id cuts on a character.
    #[test]
    fn shows_leading_characters_and_never_cuts_inside_one() {
        assert_eq!(shown_text("75ece751-aaaa-bbbb"), "75ece751");
        assert_eq!(shown_text("abc"), "abc");
        assert_eq!(shown_text("ééééééééé"), "éééééééé");
    }
}
