//! Claim-time provisioning receipts and recovery for an already-held claim.

use super::*;
use crate::work::card_staging::Staging;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, TS, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ClaimWorkspace {
    Ready {
        path: String,
    },
    NotRequired,
    Failed {
        stage: String,
        error: String,
        recovery: String,
    },
}

impl From<Staging> for ClaimWorkspace {
    fn from(staging: Staging) -> Self {
        match staging {
            Staging::Ready { path } => Self::Ready { path: path.to_string_lossy().into_owned() },
            Staging::Ordinary => Self::NotRequired,
            Staging::Failed { stage, error } => Self::Failed {
                stage: stage.to_owned(), error,
                recovery: "Your claim is held, but its workspace is unavailable. Report this substrate fault; after it is repaired, call work/claim on this card again to retry provisioning. Do not use code/shell to create or bind the missing workspace.".to_owned(),
            },
        }
    }
}

pub(super) async fn for_claim(
    airc: &Arc<Airc>,
    card_id: WorkCardId,
    already_held: bool,
) -> ClaimWorkspace {
    let Some((_room, card)) = card_in_subscribed_rooms(airc, card_id).await else {
        return Staging::Failed {
            stage: "board",
            error:
                "The claimed card could not be read from a subscribed room; no checkout was staged."
                    .to_owned(),
        }
        .into();
    };
    let peer = airc.peer_id().as_uuid();
    // A repeated claim is a repair opportunity, not a workspace transfer. Never
    // fetch, migrate or move an existing owner's tree while her hands may use it.
    let existing = already_held
        .then(|| crate::persona::card_checkout::checkout_path_for(&peer, &card))
        .flatten();
    let staging = provision(existing, || async {
        match crate::paths::continuum_home() {
            Ok(home) => crate::work::card_staging::stage_for_card(&home, peer, &card).await,
            Err(error) => Staging::Failed {
                stage: "home",
                error: error.to_string(),
            },
        }
    })
    .await;
    continuum_base::probe!(
        class = "work.claim.held", card_id = %airc_core::shown_id::shown_form(card_id.as_uuid()),
        claimer = %airc_core::shown_id::shown_form(peer), staging = ?staging,
        "claim held; workspace provisioning outcome is returned to the caller"
    );
    staging.into()
}

async fn provision<F, Fut>(existing: Option<PathBuf>, stage: F) -> Staging
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Staging>,
{
    match existing {
        Some(path) => Staging::Ready { path },
        None => stage().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_provision_can_retry_then_existing_workspace_is_not_touched() {
        let home = tempfile::tempdir().expect("temporary fixture");
        let checkout = home.path().join("card");
        let failed: ClaimWorkspace = provision(None, || async {
            Staging::Failed {
                stage: "worktree",
                error: "remote default unavailable".to_owned(),
            }
        })
        .await
        .into();
        let serialized = serde_json::to_value(&failed).expect("receipt serialization");
        assert_eq!(serialized["status"], "failed");
        assert_eq!(serialized["stage"], "worktree");
        assert!(serialized["recovery"]
            .as_str()
            .expect("recovery text")
            .contains("work/claim"));
        let ready = provision(None, || async {
            std::fs::create_dir(&checkout).expect("stage fixture");
            Staging::Ready {
                path: checkout.clone(),
            }
        })
        .await;
        assert_eq!(
            ready,
            Staging::Ready {
                path: checkout.clone()
            }
        );
        let work = checkout.join("uncommitted.txt");
        std::fs::write(&work, "owner work").expect("owner fixture");
        let reused = provision(Some(checkout.clone()), || async {
            panic!("reclaim must not stage, fetch, migrate or transfer an existing tree")
        })
        .await;
        assert_eq!(reused, ready);
        assert_eq!(
            std::fs::read_to_string(work).expect("preserved work"),
            "owner work"
        );
    }
}
