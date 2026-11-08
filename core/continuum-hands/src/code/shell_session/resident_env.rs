//! What a resident's shell carries beyond the user's own login environment: her identity on
//! every commit, and her GitHub credential, supplied by the core for each command.
//!
//! A resident has the environment an agent session has (Joel, 2026-11-08: "Stop caging them"):
//! plain `git` and `gh` from her shell work as they do for an agent, under HER name. Two things
//! made them fail before: every commit fell to the node's global git config (the operator's
//! name, e5c80c7f / b51f4b59), and the core's GitHub token never reached her (the core starts
//! with its secrets in the environment and `strip_secret_env` removes them from her child, as
//! it must). The brokered-credential rule (Joel): the core supplies the credential to her
//! process for the one command, through the environment `gh` and git's credential helper read,
//! and never writes it to a file. Her identity and the credential are set after every other
//! variable, so neither the session nor the inherited environment can override them.

use crate::code::git_actor::GitActor;

/// The variables a resident's command runs with, and whether her GitHub credential is among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentEnv {
    pub vars: Vec<(String, String)>,
    pub github: GithubCredential,
}

/// Whether the core could supply her GitHub credential for this command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubCredential {
    Supplied,
    /// The core holds none it could hand her (no `gh` login on this host): her command still
    /// runs, and its output says why GitHub will refuse it.
    Unavailable(String),
}

/// A pending read of the core's GitHub credential.
pub type CredentialFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>;

/// Where her GitHub credential comes from, supplied by the layer that holds the core's `gh` login
/// (the command layer above this one), read fresh for each command.
pub type GithubCredentialSource = std::sync::Arc<dyn Fn() -> CredentialFuture + Send + Sync>;

/// Her identity as author and committer, and her GitHub credential when the core holds one, read
/// from `credential` for this command.
pub async fn resident_env(actor: &GitActor, credential: &GithubCredentialSource) -> ResidentEnv {
    let mut vars: Vec<(String, String)> = actor.env().iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let token = credential().await;
    let github = match token.map(|t| t.trim().to_string()) {
        Ok(token) if !token.is_empty() => {
            vars.extend(github_credential_env(&token));
            GithubCredential::Supplied
        }
        Ok(_) => GithubCredential::Unavailable("`gh auth token` answered no token".into()),
        Err(error) => GithubCredential::Unavailable(error),
    };
    ResidentEnv { vars, github }
}

/// The environment that hands `token` to `gh` (`GH_TOKEN`) and to git's HTTPS transport for
/// github.com: git's credential helper list is reset and set to `gh auth git-credential`, which
/// answers from `GH_TOKEN`. Configuration through `GIT_CONFIG_COUNT`, never a config file: the
/// token lives only in this process's environment.
pub fn github_credential_env(token: &str) -> Vec<(String, String)> {
    let helper = "credential.https://github.com.helper";
    [
        ("GH_TOKEN", token),
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", helper),
        ("GIT_CONFIG_VALUE_0", ""),
        ("GIT_CONFIG_KEY_1", helper),
        ("GIT_CONFIG_VALUE_1", "!gh auth git-credential"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}
