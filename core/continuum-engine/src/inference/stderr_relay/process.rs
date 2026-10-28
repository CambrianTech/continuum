//! The relay as its own process: the arguments the spawning core passes it, the entry
//! `continuum-core-server --log-relay` dispatches to before config, tracing or sockets exist, and
//! the spawn that starts it in the engine's process group.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

use continuum_host::output_handoff::{self, Handoff};

use super::{run, Config, EngineIncarnation};

/// The core executable's relay mode.
pub const FLAG: &str = "--log-relay";

/// The relay's arguments after [`FLAG`]: the engine it relays, fixed by the core that spawned
/// both.
pub fn to_args(engine: EngineIncarnation) -> [String; 4] {
    [FLAG.to_string(), engine.port.to_string(), engine.pid.to_string(), engine.started_s.to_string()]
}

/// PURE: the engine named by the arguments after [`FLAG`].
pub fn parse(args: &[OsString]) -> Result<EngineIncarnation, String> {
    let [port, pid, started_s] = args else {
        return Err(format!("{FLAG} takes <port> <engine pid> <engine start s>, got {} arguments", args.len()));
    };
    let number = |name: &str, arg: &OsString| arg.to_str().and_then(|s| s.parse::<u64>().ok()).ok_or_else(|| format!("{FLAG}: {name} {arg:?} is not a number"));
    let port = u16::try_from(number("port", port)?).map_err(|_| format!("{FLAG}: port {port:?} is past 65535"))?;
    let pid = u32::try_from(number("engine pid", pid)?).map_err(|_| format!("{FLAG}: engine pid {pid:?} is past the pid range"))?;
    Ok(EngineIncarnation { pid, started_s: number("engine start", started_s)?, port })
}

/// The endpoint of the relay for the lane on `port`, beside its archive.
pub fn endpoint(port: u16) -> Option<PathBuf> {
    super::super::child_log::llama_server_log(port).map(|log| log.with_extension("sock"))
}

/// Relay this process's stdin, the engine's stderr, until every writer has closed it.
pub fn main(engine: EngineIncarnation) -> io::Result<()> {
    let log = super::super::child_log::llama_server_log(engine.port).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory for the engine's log"))?;
    let socket = endpoint(engine.port).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory for the relay's endpoint"))?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run(tokio::io::stdin(), engine, &log, &socket, Config::default())).map(|_| ())
}

/// Start a relay for `engine`, reading `stderr`, as this executable in relay mode inside the
/// engine's process group. Where the host has no handoff the stderr comes back unread.
pub fn spawn(stderr: tokio::process::ChildStderr, engine: EngineIncarnation) -> io::Result<Handoff> {
    output_handoff::spawn_reader(&std::env::current_exe()?, to_args(engine), stderr, engine.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the spawning core and the relay disagreeing on what the arguments say,
    // or a malformed argument read as some other engine instead of refused.
    #[test]
    fn the_arguments_name_the_engine_both_ways_and_refuse_anything_else() {
        let engine = EngineIncarnation { pid: 4242, started_s: 1_793_000_000, port: 8090 };
        let args: Vec<OsString> = to_args(engine).iter().skip(1).map(OsString::from).collect();
        assert_eq!(parse(&args), Ok(engine));
        assert!(parse(&args[..2]).is_err(), "too few");
        let bad: Vec<OsString> = ["70000", "1", "1"].iter().map(OsString::from).collect();
        assert!(parse(&bad).is_err(), "a port past 65535");
        let bad: Vec<OsString> = ["8090", "x", "1"].iter().map(OsString::from).collect();
        assert!(parse(&bad).is_err(), "a pid that is not a number");
    }
}
