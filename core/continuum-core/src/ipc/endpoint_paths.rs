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
    select_core_socket(
        std::env::var("CONTINUUM_CORE_SOCKET").ok(),
        installed_core_socket(),
    )
}

/// The endpoint, by precedence: an explicit `CONTINUUM_CORE_SOCKET`; else, on a Windows node
/// the installer set up, the socket its active release receipt records (the one the
/// supervisor hands the core it launches); else the platform default.
///
/// The default is a guess from THIS process's temp dir, and on Windows that differs between
/// processes: the installer records the socket from its own session (on the 5090,
/// `D:\continuum-cold\tmp`), while the S4U deploy task and an operator's shell resolve
/// `C:\Users\…\AppData\Local\Temp`. The handoff compares the two exactly, so measured on the
/// 5090 2026-10-10 09:17Z, every unattended deploy refused ("ContinuumCore does not select the
/// requested artifact/socket"), as did `continuum start`, all while the core answered on the
/// receipt's socket the whole time.
fn select_core_socket(explicit: Option<String>, installed: Option<String>) -> String {
    explicit
        .or(installed)
        .unwrap_or_else(default_core_socket)
}

/// The socket the active release receipt records, on Windows. A node with no receipt (a
/// developer checkout, any Unix host) has none and takes the default. A receipt that exists
/// but cannot be read is named on the probe stream rather than silently standing in for one.
fn installed_core_socket() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let home = crate::paths::continuum_home().ok()?;
    match receipt_socket_in(&home) {
        Ok(socket) => socket,
        Err(why) => {
            crate::probe!(
                class = "ipc.endpoint.receipt_unreadable",
                home = %home.display(),
                why = %why,
                "the active release receipt exists but names no readable socket; the platform default is used, and a service handoff will refuse on the mismatch"
            );
            None
        }
    }
}

/// `release.socket` from `<home>/install-active.json`: `Ok(None)` when there is no receipt.
fn receipt_socket_in(home: &std::path::Path) -> Result<Option<String>, String> {
    let path = home.join("install-active.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let receipt: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    match receipt.pointer("/release/socket").and_then(|s| s.as_str()) {
        Some(socket) if !socket.is_empty() => Ok(Some(socket.to_string())),
        _ => Err(format!("{}: no release.socket", path.display())),
    }
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

    // what this catches: the 5090's unattended deploys refusing every handoff (2026-10-10 09:17Z):
    // a CLI on an installed node must dial the socket the active release records, not a guess
    // from its own temp dir, while an explicit override still wins and a node with no receipt
    // keeps the default. A receipt that exists but names no socket is an error, never a default.
    #[test]
    fn an_installed_node_dials_the_socket_its_active_release_records() {
        let home = tempfile::tempdir().expect("test: home");
        assert_eq!(receipt_socket_in(home.path()), Ok(None), "no receipt: no installed socket");
        std::fs::write(
            home.path().join("install-active.json"),
            r#"{"release":{"artifact":"a","socket":"D:\\cold\\tmp\\continuum-core.sock"},"hashes":{}}"#,
        )
        .expect("test: receipt");
        let installed = receipt_socket_in(home.path()).expect("test: readable");
        assert_eq!(installed.as_deref(), Some("D:\\cold\\tmp\\continuum-core.sock"));
        assert_eq!(select_core_socket(None, installed.clone()), "D:\\cold\\tmp\\continuum-core.sock");
        assert_eq!(select_core_socket(Some("/custom/core.sock".into()), installed), "/custom/core.sock");
        assert_eq!(select_core_socket(None, None), default_core_socket());
        std::fs::write(home.path().join("install-active.json"), r#"{"release":{"artifact":"a"}}"#)
            .expect("test: receipt");
        assert!(receipt_socket_in(home.path()).is_err(), "a receipt with no socket is named, not defaulted");
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
