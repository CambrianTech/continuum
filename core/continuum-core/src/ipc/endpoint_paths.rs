//! Where the core's IPC endpoint and start log live — ONE definition, shared by every process
//! that has to agree on them (the `continuum` CLI, `continuum-mcp`, the server, start-server.sh).
//!
//! This existed as the literal `"/tmp/continuum-core.sock"` copied into four places. That is a
//! compression violation on its own, but on Windows it is also a correctness bug: a leading-slash
//! path is not absolute there, it resolves against the CURRENT DRIVE. Two processes started from
//! different drives resolve the same string to different files, so the CLI can create a start log
//! the operator cannot find and dial a socket the core never bound — which reads exactly like "the
//! core is broken on Windows" and is really "we never agreed on a path".
//!
//! Unix keeps the well-known `/tmp` locations verbatim so existing pairings, scripts and running
//! daemons are untouched. Only platforms without `/tmp` resolve through the real temp dir.

use std::path::PathBuf;

/// Default core IPC socket. Override with `CONTINUUM_CORE_SOCKET`.
pub fn default_core_socket() -> String {
    endpoint_path("continuum-core.sock")
}

/// Where `continuum start` writes the start script's output, for the failure diagnostic to tail.
pub fn core_start_logfile() -> String {
    endpoint_path("continuum-core-start.log")
}

/// Resolve the socket to use: the env override if set, else the platform default.
pub fn core_socket_path() -> String {
    std::env::var("CONTINUUM_CORE_SOCKET").unwrap_or_else(|_| default_core_socket())
}

/// Windows' primary listener and local providers must select the same TCP port.
pub fn core_tcp_port() -> u16 {
    tcp_port_from(std::env::var("CONTINUUM_CORE_TCP").ok().as_deref())
}

pub fn tcp_port_from(value: Option<&str>) -> u16 {
    value
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(9100)
}

/// A dialable endpoint for providers, rather than the Windows socket-path placeholder.
pub fn core_provider_endpoint() -> String {
    if cfg!(windows) {
        format!("tcp://127.0.0.1:{}", core_tcp_port())
    } else {
        core_socket_path()
    }
}

fn endpoint_path(name: &str) -> String {
    if cfg!(windows) {
        // std::env::temp_dir() is already the established pattern in this crate (airc endpoints,
        // forge custodian, eval roots) and yields a real absolute path with a drive letter.
        PathBuf::from(std::env::temp_dir())
            .join(name)
            .display()
            .to_string()
    } else {
        format!("/tmp/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches a launcher selecting a different port from the Windows listener,
    // including its existing invalid/zero-port fallback.
    #[test]
    fn provider_port_obeys_the_listener_contract() {
        for value in [None, Some(""), Some("invalid"), Some("0"), Some("65536")] {
            assert_eq!(tcp_port_from(value), 9100);
        }
        assert_eq!(tcp_port_from(Some("19234")), 19234);
        if cfg!(windows) {
            assert_eq!(
                core_provider_endpoint(),
                format!("tcp://127.0.0.1:{}", core_tcp_port())
            );
        }
    }

    // what this catches: the Windows regression this module exists for -- a path that is not
    // absolute resolves against the current drive, so two processes with different working
    // directories disagree about which file they mean. Every endpoint path must be absolute on
    // its own terms, with a drive prefix where the platform has one.
    #[test]
    fn endpoint_paths_are_absolute_on_every_platform() {
        for p in [default_core_socket(), core_start_logfile()] {
            let path = std::path::Path::new(&p);
            assert!(path.is_absolute(), "endpoint path must be absolute: {p}");
            if cfg!(windows) {
                assert!(
                    p.contains(':'),
                    "on Windows an endpoint path needs a drive prefix, else it resolves against \
                     whatever drive the process happens to be on: {p}"
                );
            }
        }
    }

    // what this catches: silently relocating the Unix socket would strand every already-running
    // core, MCP client and start-server.sh invocation that is pointed at the well-known path.
    #[test]
    fn unix_keeps_the_well_known_tmp_paths() {
        if !cfg!(windows) {
            assert_eq!(default_core_socket(), "/tmp/continuum-core.sock");
            assert_eq!(core_start_logfile(), "/tmp/continuum-core-start.log");
        }
    }

    // what this catches: the env override is the documented way to run two cores side by side;
    // if the default ever shadowed it, they would collide on one socket.
    #[test]
    fn env_override_wins_over_the_default() {
        // SAFETY: single-threaded test process; restored immediately below.
        let key = "CONTINUUM_CORE_SOCKET";
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, "/custom/core.sock") };
        assert_eq!(core_socket_path(), "/custom/core.sock");
        match prev {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}
