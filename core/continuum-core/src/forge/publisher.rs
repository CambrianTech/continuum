//! `forge::publisher` — the Publisher adapter seam (#99 L4, slice 2b).
//!
//! Publishing a genome layer to a destination is an ADAPTER behind a trait, the
//! same way inference is an adapter over base models + cloud APIs and
//! [`ForgeCustodian`](super::custodian_client::ForgeCustodian) is an adapter over
//! local-vs-grid export (Joel 2026-07-13: "HF would be an adapter of trait
//! publishers … so we can take each on, including cross-grid, like we do our base
//! models and cloud APIs for inference"). The Owner-gated `forge/publish` command
//! depends only on `dyn Publisher`; WHERE a layer lands — HuggingFace, a trusted
//! grid peer, a private mirror — is a swappable impl, never baked into the caller.
//!
//! Outliers that prove the interface (per the methodical process): `HfPublisher`
//! (public HF, outlier A) and a future `GridPublisher` (peer-to-peer, outlier B).
//! Both consume the SAME validated [`PublishRequest`] — which can't exist unless it
//! passed every gate — so no adapter re-implements validation; they only deliver.

use async_trait::async_trait;

use super::publish_request::{PublishError, PublishRequest};

/// Where a published layer landed — the handle the market records so recall can
/// later find + fetch it. `location` is the transport-native address (an `https://
/// huggingface.co/…` URL, a grid peer URI, …); `transport` names the adapter that
/// placed it, for provenance + audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationReceipt {
    /// The adapter that published (`"huggingface"`, `"grid"`, …).
    pub transport: String,
    /// The transport-native address the layer now lives at.
    pub location: String,
}

/// A destination a validated genome layer can be published to. One trait, many
/// adapters — HF, grid peer, private mirror — each a swappable `dyn Publisher` the
/// `forge/publish` command selects by config/target. `Send + Sync` for the tokio
/// runtime; async because delivery is I/O.
#[async_trait]
pub trait Publisher: Send + Sync {
    /// Short, stable name for logs + selection (`"huggingface"`, `"grid"`, …).
    fn name(&self) -> &'static str;

    /// Deliver a validated layer to this transport's destination. The `req` is
    /// already gated + well-formed (a [`PublishRequest`] can't be built otherwise),
    /// so an adapter NEVER re-validates — it only uploads and reports where it
    /// landed. Failure is a LOUD [`PublishError::Transport`] carrying this
    /// transport's name + the cause ([[fallbacks-are-illegal-fail-loud]]); a
    /// caller may retry or try another publisher.
    async fn publish(&self, req: &PublishRequest) -> Result<PublicationReceipt, PublishError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::publish_request::{PublishInputs, PublishRequest};
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// A recording publisher — proves the trait shape end-to-end (a validated
    /// request goes in, a receipt comes out) without touching the network. The
    /// outlier-A stand-in until `HfPublisher` lands.
    struct RecordingPublisher {
        seen: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl Publisher for RecordingPublisher {
        fn name(&self) -> &'static str {
            "recording"
        }
        async fn publish(&self, req: &PublishRequest) -> Result<PublicationReceipt, PublishError> {
            self.seen
                .lock()
                .unwrap()
                .push(req.repo_id.as_str().to_string());
            Ok(PublicationReceipt {
                transport: self.name().to_string(),
                location: format!("recording://{}", req.repo_id.as_str()),
            })
        }
    }

    fn valid_request() -> PublishRequest {
        PublishRequest::build(
            &PublishInputs {
                repo_id: "continuum-ai/devstral-code-asha".to_string(),
                gene_path: PathBuf::from("/genome/asha/code/adapters.gguf"),
                base_model: "unsloth/Devstral-Small-2507-GGUF".to_string(),
                trait_kind: "code".to_string(),
                lift: 0.051,
                ..Default::default()
            },
            |_| true,
        )
        .expect("valid inputs build")
    }

    // what this catches: the Publisher seam accepts a validated request and returns
    // a receipt naming the transport + location — the contract every adapter (HF,
    // grid) satisfies, and the command depends on.
    #[tokio::test]
    async fn publisher_delivers_validated_request_and_receipts_it() {
        let pubr = RecordingPublisher {
            seen: Mutex::new(vec![]),
        };
        let receipt = pubr.publish(&valid_request()).await.expect("publish ok");
        assert_eq!(receipt.transport, "recording");
        assert_eq!(
            receipt.location,
            "recording://continuum-ai/devstral-code-asha"
        );
        assert_eq!(
            pubr.seen.lock().unwrap().as_slice(),
            &["continuum-ai/devstral-code-asha"]
        );
    }

    // what this catches: a transport failure is a LOUD, typed, transport-named
    // error — distinct from validation refusals so a caller can retry/re-route.
    #[tokio::test]
    async fn transport_failure_is_loud_and_named() {
        struct FailingPublisher;
        #[async_trait]
        impl Publisher for FailingPublisher {
            fn name(&self) -> &'static str {
                "grid"
            }
            async fn publish(
                &self,
                _: &PublishRequest,
            ) -> Result<PublicationReceipt, PublishError> {
                Err(PublishError::Transport {
                    transport: self.name().to_string(),
                    detail: "peer unreachable".to_string(),
                })
            }
        }
        let err = FailingPublisher
            .publish(&valid_request())
            .await
            .unwrap_err();
        assert!(matches!(err, PublishError::Transport { .. }));
        assert!(
            err.to_string().contains("grid"),
            "names the transport: {err}"
        );
    }
}
