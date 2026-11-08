//! WHO A COMMIT IS BY: the one identity owner for every git write a citizen's hands make.
//!
//! Before this, `code/git/commit` ran a bare `git commit` and the act-end autosave set only the
//! author, so every resident commit fell to the node's global git config for its committer, and
//! her own commits for its author too: Aris's and Benchy's branches (e5c80c7f, b51f4b59, 2026-11)
//! were authored AND committed by the operator. A commit is by the peer acting, keyed to her airc
//! identity (`<name> <peer>@airc.citizen`, the convention since 2026-10-05), as both author and
//! committer. [`crate::code::git_bridge::run_git_input`] applies it, and refuses a committing git
//! that names none: there is no global config to fall back to.

use uuid::Uuid;


/// The identity a commit is made by: her display name and her airc address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitActor {
    name: String,
    email: String,
}

/// Why no actor could be named for a call: the call has no acting peer at all (a caller-less
/// session before the node's self-peers are online).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NoGitActor {
    #[error("no acting identity for this call: a commit is made by the peer acting, and none is known (the node's self-peers may not be online yet this boot; retry shortly)")]
    NoActingPeer,
    #[error("the hands key `{0}` names no peer: a commit is made by the peer acting")]
    NotAPeer(String),
}

impl From<NoGitActor> for crate::sdk_codegen::CommandError {
    fn from(e: NoGitActor) -> Self {
        crate::sdk_codegen::CommandError::Invalid(e.to_string())
    }
}

impl GitActor {
    /// `peer`, named `name` when a display name is known. The address carries the identity; a peer
    /// whose name this node cannot read (a remote peer, a layer provisioned before her runtime) is
    /// named by its shown id, never by anyone else's.
    pub fn citizen(peer: Uuid, name: Option<&str>) -> Self {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_string(),
            None => airc_core::shown_id::shown_form(peer),
        };
        Self { name, email: format!("{peer}@airc.citizen") }
    }

    /// The peer a hands key (`peer_id.to_string()`) names, for provisioning that runs on her behalf
    /// with no call context (her layer's sync, her workspace's first commit), named by `name_of`.
    pub fn of_key(key: &str, name_of: impl FnOnce(Uuid) -> Option<String>) -> Result<Self, NoGitActor> {
        let peer = Uuid::parse_str(key).map_err(|_| NoGitActor::NotAPeer(key.to_string()))?;
        Ok(Self::citizen(peer, name_of(peer).as_deref()))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    /// The environment that makes her both author and committer. Applied by the git runner after
    /// every other variable, so nothing a caller or the process inherited can override it.
    pub(crate) fn env(&self) -> [(&'static str, &str); 4] {
        [
            ("GIT_AUTHOR_NAME", self.name.as_str()),
            ("GIT_AUTHOR_EMAIL", self.email.as_str()),
            ("GIT_COMMITTER_NAME", self.name.as_str()),
            ("GIT_COMMITTER_EMAIL", self.email.as_str()),
        ]
    }

    /// The trailer that credits her on a commit someone else makes from her work (a squash merge).
    pub fn co_author_trailer(&self) -> String {
        format!("Co-Authored-By: {} <{}>", self.name, self.email)
    }
}

/// Does this git invocation write a commit (and so need an author and a committer)? The verb is the
/// first argument that is not a global option (`-c key=value`, `-C dir` and the like before it).
pub(crate) fn commits(args: &[&str]) -> bool {
    const COMMITTING: [&str; 10] = ["commit", "commit-tree", "merge", "rebase", "cherry-pick", "revert", "am", "pull", "stash", "notes"];
    const TAKES_VALUE: [&str; 5] = ["-c", "-C", "--git-dir", "--work-tree", "--namespace"];
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if TAKES_VALUE.contains(arg) {
            rest.next();
        } else if !arg.starts_with('-') {
            return COMMITTING.contains(arg);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an actor whose name or address could be anyone's but hers: a blank name
    // read as a value, or an address not keyed to her peer id.
    #[test]
    fn an_actor_is_her_peer_named_by_her_name_or_her_shown_id() {
        let peer = Uuid::from_u128(0x8044_b75b_0000_0000_0000_0000_0000_0001);
        let named = GitActor::citizen(peer, Some("Aris"));
        assert_eq!((named.name(), named.email()), ("Aris", format!("{peer}@airc.citizen").as_str()));
        assert_eq!(named.co_author_trailer(), format!("Co-Authored-By: Aris <{peer}@airc.citizen>"));
        let unnamed = GitActor::citizen(peer, Some("  "));
        assert_eq!(unnamed.name(), airc_core::shown_id::shown_form(peer));
        assert_eq!(unnamed.email(), named.email());
    }

    // what this catches: a committing verb slipping past the runner's actor check (a bare
    // `git commit` falls to the node's global config), or a read refused for want of one.
    #[test]
    fn committing_verbs_are_told_from_reads() {
        for verb in ["commit", "commit-tree", "merge", "cherry-pick", "stash"] {
            assert!(commits(&[verb, "-m", "x"]), "{verb}");
        }
        for verb in ["status", "push", "fetch", "rev-parse", "add", "apply", "update-ref"] {
            assert!(!commits(&[verb]), "{verb}");
        }
        assert!(!commits(&[]));
        assert!(commits(&["-c", "core.hooksPath=/h", "commit", "-m", "x"]), "a global option before the verb");
        assert!(!commits(&["-C", "commit", "status"]), "an option's value is not the verb");
    }
}
