//! Why a boot phase refused to bring the core up. Each was a `panic!`/`expect` inside its
//! phase; now the phase returns the cause, `start_server` returns it, and `main` logs it and
//! exits non-zero, the path every other boot refusal already takes. The cause stays typed
//! inside the `io::Error` `start_server` returns (`io::Error::other` keeps it as the source).

use crate::ipc::positron_canvas_source::CanvasFeedInstalledTwice;

#[derive(Debug, thiserror::Error)]
pub(super) enum BootError {
    /// The model registry is the single source of truth for model ids; a catalog that does not
    /// load is a packaging bug, and nothing past this phase can be constructed without it.
    #[error("failed to load model_registry: {0}")]
    ModelRegistry(#[from] crate::model_registry::RegistryError),
    /// The one continuum home did not resolve, so the grid has nowhere to keep its state; the
    /// home's own refusal (no home, a retired CONTINUUM_ROOT) is the reason, never a guessed tree.
    #[error("the continuum home must resolve before the grid can keep its state: {0}")]
    ContinuumHome(String),
    /// The WebSocket ingress is on, but the executor carries no bus for the chat projection to
    /// subscribe to: a WS server with no state source is a boot bug, not a runtime condition.
    #[error(
        "CONTINUUM_CORE_WS is set but the command executor has no message bus — \
         the positron chat projection has no airc source to subscribe to"
    )]
    WsIngressWithoutBus,
    #[error(transparent)]
    RecipeLoad(#[from] crate::experience::source::RecipeLoadError),
    #[error(transparent)]
    CanvasFeed(#[from] CanvasFeedInstalledTwice),
}

impl From<BootError> for std::io::Error {
    fn from(e: BootError) -> Self {
        std::io::Error::other(e)
    }
}
