/// Logging module for continuum-core
///
/// Every macro here is a `tracing` event; `clog_layer` routes them to the per-category files.
/// Provides macros for performance timing and structured logging.
///
/// # Easy Logging (Zero Setup)
///
/// Auto-routing macros that derive category from module_path!():
///
/// ```rust
/// use crate::clog_info;
///
/// // Auto-routes to modules/voice.log based on module path
/// clog_info!("Session started for user {}", user_id);
/// clog_warn!("Rate limit approaching");
/// clog_error!("Connection failed: {}", err);
/// ```
///
/// # Explicit Logging (Full Control)
///
/// ```rust
/// use crate::log_info;
///
/// log_info!("personas/helper/cognition", "DecisionEngine", "Made decision: {}", decision);
/// ```
pub mod clog_layer;
pub mod timing;
pub mod preview;

pub use clog_layer::CLOG_TARGET;
pub use timing::TimingGuard;

// Re-export macros (they're already at crate root via #[macro_export])
pub use crate::{time_async, time_section};

// ============================================================================
// The macros are plain `tracing` events on `CLOG_TARGET`. They know nothing about
// files: `clog_layer::ClogLayer`, installed once by `routing::install_probe_tracing`,
// routes them to the per-category files. Category and component travel as fields when
// the macro is given them; otherwise the layer derives both from the event's module path.
// ============================================================================

/// Log to an explicit category and component at debug level.
#[macro_export]
macro_rules! log_debug {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::DEBUG,
            category = %$category,
            component = %$component,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log to an explicit category and component at info level.
#[macro_export]
macro_rules! log_info {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::INFO,
            category = %$category,
            component = %$component,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log to an explicit category and component at warn level.
#[macro_export]
macro_rules! log_warn {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::WARN,
            category = %$category,
            component = %$component,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log to an explicit category and component at error level.
#[macro_export]
macro_rules! log_error {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::ERROR,
            category = %$category,
            component = %$component,
            "{}",
            format_args!($($arg)*)
        )
    };
}

// ============================================================================
// Easy Auto-Routing Macros — Zero Setup Required
// ============================================================================
//
// These macros derive the category and component from module_path!().
// No need to specify category/component - just log!
//
// Category routing examples:
// - continuum_core::live::session::orchestrator → modules/live
// - continuum_core::modules::data → modules/data
// - continuum_core::orm::sqlite → modules/orm
// - continuum_core::ipc::* → system/ipc

/// Convert module_path!() to a log category.
///
/// Maps Rust module paths to concern-based log categories:
/// - `continuum_core::live::*` → `modules/live`
/// - `continuum_core::modules::data` → `modules/data`
/// - `continuum_core::orm::*` → `modules/orm`
/// - `continuum_core::ipc::*` → `system/ipc`
pub fn module_path_to_category(module_path: &str) -> &'static str {
    // Domains cut out of continuum-core keep their old categories.
    if module_path.starts_with("continuum_models::") {
        return "modules/models";
    }
    if module_path.starts_with("continuum_embedding::") {
        return "modules/embedding";
    }
    if module_path.starts_with("continuum_store::") {
        return "modules/data";
    }
    // Strip the crate prefix
    let path = module_path
        .strip_prefix("continuum_core::")
        .unwrap_or(module_path);

    // Match first segment to determine category
    if path.starts_with("modules::data") {
        "modules/data"
    } else if path.starts_with("modules::embedding") {
        "modules/embedding"
    } else if path.starts_with("commands::search") {
        "commands/search"
    } else if path.starts_with("modules::logger") {
        "modules/logger"
    } else if path.starts_with("modules::live") {
        "modules/live"
    } else if path.starts_with("modules::memory") {
        "modules/memory"
    } else if path.starts_with("modules::code") {
        "modules/code"
    } else if path.starts_with("modules::rag") {
        "modules/rag"
    } else if path.starts_with("modules::cognition") {
        "modules/cognition"
    } else if path.starts_with("modules::channel") {
        "modules/channel"
    } else if path.starts_with("modules::health") {
        "modules/health"
    } else if path.starts_with("modules::models") {
        "modules/models"
    } else if path.starts_with("live::") {
        "modules/live"
    } else if path.starts_with("orm::") {
        "modules/orm"
    } else if path.starts_with("ai::") || path.starts_with("inference::") {
        "modules/inference"
    } else if path.starts_with("memory::") {
        "modules/memory"
    } else if path.starts_with("rag::") {
        "modules/rag"
    } else if path.starts_with("code::") {
        "modules/code"
    } else if path.starts_with("ipc::") {
        "system/ipc"
    } else if path.starts_with("concurrency::") {
        "system/concurrency"
    } else if path.starts_with("runtime::") {
        "system/runtime"
    } else if path.starts_with("persona::") {
        "modules/persona"
    } else {
        "system/core"
    }
}

/// Extract component name from module path (last segment).
pub fn extract_component(module_path: &str) -> &str {
    module_path.rsplit("::").next().unwrap_or(module_path)
}

/// Log at debug level, routed to the category derived from the caller's module path.
#[macro_export]
macro_rules! clog_debug {
    ($($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::DEBUG,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log at info level, routed to the category derived from the caller's module path.
#[macro_export]
macro_rules! clog_info {
    ($($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::INFO,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log at warn level, routed to the category derived from the caller's module path.
#[macro_export]
macro_rules! clog_warn {
    ($($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::WARN,
            "{}",
            format_args!($($arg)*)
        )
    };
}

/// Log at error level, routed to the category derived from the caller's module path.
#[macro_export]
macro_rules! clog_error {
    ($($arg:tt)*) => {
        ::tracing::event!(
            target: $crate::logging::CLOG_TARGET,
            ::tracing::Level::ERROR,
            "{}",
            format_args!($($arg)*)
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_path_conversion() {
        assert_eq!(
            module_path_to_category("continuum_core::live::session::orchestrator"),
            "modules/live"
        );
        assert_eq!(
            module_path_to_category("continuum_store::data::data_state"),
            "modules/data",
            "the data layer keeps its category after the store cut (card 2709ec6d)"
        );
        assert_eq!(
            module_path_to_category("continuum_core::modules::data"),
            "modules/data"
        );
        assert_eq!(
            module_path_to_category("continuum_core::orm::sqlite"),
            "modules/orm"
        );
        assert_eq!(
            module_path_to_category("continuum_core::ipc::handler"),
            "system/ipc"
        );
    }

    #[test]
    fn test_extract_component() {
        assert_eq!(
            extract_component("continuum_core::live::session::orchestrator"),
            "orchestrator"
        );
        assert_eq!(extract_component("my_module"), "my_module");
    }
}
