//! A citizen's workspace directory: `citizens/peers/<uuid>/`, found by her roster name or her peer
//! id. Citizen identity, so it lives here beside the roster it reads; the benchmark harness and
//! genome/teach's `--from-experience` both resolve through it (card 2f2eb8b1).

use crate::sdk_codegen::CommandError;

/// Resolve a solver to the ONE matching `citizens/peers/<uuid>/` directory. Accepts a
/// citizen NAME (resolved through the live roster — the SAME `get_by_agent_name` identity
/// path `benchmark/dispatch` uses), a full peer UUID, or a hex prefix. 0 or >1 matches
/// fail loud with the candidates named — a grade against the wrong citizen's workspace is
/// a falsified result, never a best-effort. [[the-grid-identity-spine-durable-id-fluid-location]]
pub fn resolve_citizen_dir(
    home: &std::path::Path,
    solver: &str,
) -> Result<(String, std::path::PathBuf), CommandError> {
    let peers = home.join("citizens").join("peers");

    // Identity first: a NAME ("Asha") resolves through the live roster to her durable
    // peer_id → her workspace dir. A name is never a hex UUID prefix, so without this a
    // `--solver=Asha` always failed "no citizen workspace matches" — the identity gap this
    // fixes. Only fall through to peer-UUID-prefix matching if the name doesn't resolve
    // (offline citizen, or the caller genuinely passed a uuid). None in unit tests (no live
    // roster) → straight to the prefix path, so existing behavior is preserved.
    if let Some(rt) = crate::persona::PersonaAircRuntimeRegistry::try_global()
        .and_then(|reg| reg.get_by_agent_name(solver))
    {
        let uuid = rt.airc().peer_id().as_uuid().to_string();
        let dir = peers.join(&uuid);
        if dir.is_dir() {
            return Ok((uuid, dir));
        }
    }

    let needle = solver.to_ascii_lowercase().replace('-', "");
    if needle.len() < 4 {
        return Err(CommandError::Invalid(format!(
            "solver '{solver}' is too short — pass a full peer UUID or a hex prefix of ≥4 chars"
        )));
    }
    let mut matches: Vec<(String, std::path::PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(&peers).map_err(|e| {
        CommandError::NotFound(format!("no citizen peers dir at {}: {e}", peers.display()))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .to_ascii_lowercase()
            .replace('-', "")
            .starts_with(&needle)
        {
            matches.push((name, entry.path()));
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(CommandError::NotFound(format!(
            "no citizen workspace matches solver '{solver}' under {}",
            peers.display()
        ))),
        _ => Err(CommandError::Invalid(format!(
            "solver '{solver}' is ambiguous: {}",
            matches
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}
