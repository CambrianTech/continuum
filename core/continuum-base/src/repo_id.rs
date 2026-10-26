//! A validated repository id (`namespace/name`), shared by the forge (HF / GitHub publish
//! targets) and the card checkout (managed clones). One validator: both refuse the same
//! path-traversal and charset shapes, so a clone path built from one is as safe as a publish
//! target.

/// The id is not a well-formed `namespace/name`.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidRepoId {
    pub repo_id: String,
    pub reason: String,
}

impl std::fmt::Display for InvalidRepoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid repo id '{}' — {}", self.repo_id, self.reason)
    }
}

impl std::error::Error for InvalidRepoId {}

/// A validated repository id (`namespace/name`): an HF repo or a GitHub `owner/repo`. A
/// newtype so an unchecked string can never reach a transport. Both segments must be
/// non-empty and use only `[A-Za-z0-9._-]` (the set both providers allow), with no path
/// traversal. Provider-neutral: it names WHERE, and each publisher addresses it its own way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId(String);

impl RepoId {
    pub fn parse(raw: &str) -> Result<Self, InvalidRepoId> {
        let raw = raw.trim();
        let bad = |reason: &str| InvalidRepoId {
            repo_id: raw.to_string(),
            reason: reason.to_string(),
        };
        let (ns, name) = raw
            .split_once('/')
            .ok_or_else(|| bad("expected 'namespace/name'"))?;
        if name.contains('/') {
            return Err(bad("more than one '/' — expected exactly 'namespace/name'"));
        }
        for (seg, label) in [(ns, "namespace"), (name, "name")] {
            if seg.is_empty() {
                return Err(bad(&format!("{label} segment is empty")));
            }
            if !seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return Err(bad(&format!(
                    "{label} '{seg}' has characters outside [A-Za-z0-9._-]"
                )));
            }
            if seg == "." || seg == ".." {
                return Err(bad(&format!("{label} may not be '.' or '..'")));
            }
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: malformed targets never reach the network or a clone path
    // (moved from the forge's publish_request tests with the validator).
    #[test]
    fn malformed_repo_ids_are_refused() {
        for bad in ["noSlash", "too/many/slashes", "/name", "ns/", "ns/na me", "ns/../x"] {
            assert!(RepoId::parse(bad).is_err(), "'{bad}' should be rejected");
        }
        // a normal one parses.
        assert!(RepoId::parse("continuum-ai/qwen3-coder-30b").is_ok());
    }
}
