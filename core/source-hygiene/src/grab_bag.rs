//! **No grab-bag modules: a function lives with the type or package it is about.**
//!
//! Joel, 2026-10-12: "Utils are lazy. Think Java." continuum-core had a `utils/` of
//! eight unrelated files and two more under other names (`tts/audio_utils.rs`,
//! `grid/helpers.rs`). Agents could not find what already existed, so the copies
//! drifted. `tts/audio_utils.rs` re-implemented `utils/audio`'s `f32_to_i16` and kept
//! its own resampler beside a broken one. `grid/helpers.rs` held a hand-written trust-level
//! parser (now `FromStr for TrustLevel`) and a clock-hashed "unique" id. Each file
//! moved to its owner: PCM to `live::audio::pcm`, file replacement to `continuum_host::durable`,
//! JSON params to `runtime`, ports to `system_resources`, ASCII case to persona's
//! text analysis, and log previews to `logging`.
//!
//! A module name says what it owns. `utils`, `util`, `helpers`, `common`, `misc` and
//! `*_utils` say only that the owner was not decided.

use super::{SourceFile, SourceRule, Violation};

const GRAB_BAG_STEMS: [&str; 5] = ["utils", "util", "helpers", "common", "misc"];

pub struct NoGrabBagModules;

/// The module names a path declares: each directory and the file stem.
fn module_names(rel: &str) -> impl Iterator<Item = &str> {
    rel.split('/').map(|seg| seg.strip_suffix(".rs").unwrap_or(seg)) // unwrap_or: a directory segment has no .rs suffix
}

impl SourceRule for NoGrabBagModules {
    fn name(&self) -> &'static str {
        "no_grab_bag_modules"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let grab_bag = module_names(&file.rel)
            .any(|m| GRAB_BAG_STEMS.contains(&m) || m.ends_with("_utils") || m.ends_with("_helpers"));
        if !grab_bag {
            return Vec::new();
        }
        vec![Violation {
            rule: self.name(),
            file: file.rel.clone(),
            line: 1,
            source: "a grab-bag module name: move each item to the type or package it is about".to_string(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new utils/, helpers.rs or *_utils.rs anywhere in
    // continuum-core. Fix it by moving each item to its owner, never by renaming the
    // grab-bag.
    #[test]
    fn no_module_is_a_grab_bag() {
        let violations = scan(&[&NoGrabBagModules]);
        assert!(
            violations.is_empty(),
            "grab-bag modules:\n{}",
            violations.iter().map(|v| format!("  {}", v.file)).collect::<Vec<_>>().join("\n")
        );
    }

    // what this catches: the predicate drifting. It must flag a grab-bag directory, file
    // or suffix and spare a name that merely contains one of the words.
    #[test]
    fn the_predicate_flags_grab_bags_only() {
        let flagged = |rel: &str| NoGrabBagModules.check(&SourceFile::for_test(rel, "")).len();
        assert_eq!(flagged("utils/params.rs"), 1);
        assert_eq!(flagged("modules/grid/helpers.rs"), 1);
        assert_eq!(flagged("live/audio/tts/audio_utils.rs"), 1);
        assert_eq!(flagged("persona/common.rs"), 1);
        assert_eq!(flagged("live/audio/pcm.rs"), 0);
        assert_eq!(flagged("cognition/utility_scoring.rs"), 0, "a word containing 'util' is not a grab-bag");
    }
}
