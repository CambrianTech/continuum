//! The generation between [`Part`] and each kind of audio model: a backend that loads a model,
//! reports whether it is loaded, takes parameters and releases its model. Speech-to-text and
//! text-to-speech add only what they do with it (MODULE-ARCHITECTURE.md §12).

use async_trait::async_trait;
use continuum_base::part::Part;

/// A loadable audio model backend. Each kind names its error once.
#[async_trait]
pub trait Backend: Part + Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Whether its model is loaded and ready.
    fn is_initialized(&self) -> bool;

    /// Load its model, connect to its API, or whatever makes it ready.
    async fn initialize(&self) -> Result<(), Self::Error>;

    fn get_param(&self, _name: &str) -> Option<String> {
        None
    }

    fn set_param(&self, _name: &str, _value: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Release its model and device memory. In-flight work that already holds the model finishes;
    /// the next `initialize` reloads it. A backend holding nothing heavy keeps this default.
    async fn shutdown(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}
