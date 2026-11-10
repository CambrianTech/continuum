//! The one HTTP client the core uses to talk to its local engines (paging, probes, gene pages,
//! learning admission), built once per process so every caller shares its connection pool.

/// The process's engine client.
pub fn engine_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}
