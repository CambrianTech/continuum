#![cfg(test)]
//! From `persona::training_producer`: the rejected mark needs the substrate's authority (the
//! REAL ACL).

use crate::persona::training_producer::*;

// what this catches (card ad4bfaeb): a passing review after a failure training the
// rejected turns as good. Kimi, 2026-10-10: review 5769a6c3 failed her card 63855189
// for unjustified production unwrap_or, she fixed it, and review 7845d153 passed it.
// Without verdict order the pass settled every staged row, the turns that wrote the
// mistake included, as passing work. A live failure marks the rows staged so far, so
// the pass admits only the correction; a failure her core never saw admits nothing;
// a card never failed admits all, as before.
// what this catches (card fd47a804, the 5090): the rejected mark written through her
// own identity. A local persona resolves to Trusted, and data/update is not authorized
// there, so every mark was refused (routing.acl.refused) and #4900 trained the rejected
// rows anyway. The mark goes through the substrate (Owner). If data/update ever opens
// to Trusted this fails, and the substrate connection can be revisited.
#[test]
fn the_rejected_mark_needs_the_substrates_authority_not_hers() {
    use crate::routing::command_acl::is_command_authorized;
    use crate::routing::trust_level::TrustLevel;
    assert!(!is_command_authorized("data/update", TrustLevel::Trusted), "her own identity cannot mark the row");
    assert!(is_command_authorized("data/list", TrustLevel::Trusted), "while she can still read her staged rows");
}
