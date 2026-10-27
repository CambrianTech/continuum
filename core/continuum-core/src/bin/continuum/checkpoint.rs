//! Legacy checkpoint inspect/adopt commands (offline, digest-bound plans).

use super::*;

/// Explicit legacy selection, independent of a live core's command registry.
/// Inspect emits a digest-bound plan; adopt consumes that exact plan offline.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CheckpointCommand {
    Inspect {
        source: PathBuf,
        persona_id: uuid::Uuid,
        plan: PathBuf,
    },
    Adopt {
        plan: PathBuf,
    },
}

impl CheckpointCommand {
    pub(super) fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let verb = args.next().ok_or("checkpoint requires inspect or adopt")?;
        let mut source = None;
        let mut persona_id = None;
        let mut plan = None;
        let mut legacy_writers_stopped = false;
        while let Some(flag) = args.next() {
            if verb == "adopt" && flag == "--legacy-writers-stopped" && !legacy_writers_stopped {
                legacy_writers_stopped = true;
                continue;
            }
            let slot = match (verb.as_str(), flag.as_str()) {
                ("inspect", "--source") => &mut source,
                ("inspect", "--persona-id") => &mut persona_id,
                ("inspect" | "adopt", "--plan") => &mut plan,
                _ => {
                    return Err(format!(
                        "unknown or repeated checkpoint {verb} option {flag}"
                    ))
                }
            };
            if slot.is_some() {
                return Err(format!("duplicate checkpoint option {flag}"));
            }
            *slot = Some(
                args.next()
                    .filter(|value| !value.is_empty() && !value.starts_with('-'))
                    .ok_or_else(|| format!("{flag} requires a value"))?,
            );
        }
        let plan = PathBuf::from(plan.ok_or("checkpoint requires --plan <path>")?);
        match verb.as_str() {
            "inspect" => Ok(Self::Inspect {
                source: PathBuf::from(source.ok_or("inspect requires --source <volatile.json>")?),
                persona_id: persona_id.ok_or("inspect requires --persona-id <uuid>")?
                    .parse().map_err(|error| format!("invalid persona UUID: {error}"))?,
                plan,
            }),
            "adopt" if legacy_writers_stopped => Ok(Self::Adopt { plan }),
            "adopt" => Err("adopt requires --legacy-writers-stopped: stop legacy cores and their automatic launchers before applying the inspected plan".into()),
            _ => Err(format!("unknown checkpoint operation {verb}; use inspect or adopt")),
        }
    }
}

pub(super) fn checkpoint(command: CheckpointCommand) -> Result<(), String> {
    use continuum_core::cognition::persona_workspace::checkpoint_adoption::{
        adopt, inspect, AdoptionPlan,
    };
    use std::io::{Read, Write};

    match command {
        CheckpointCommand::Inspect {
            source,
            persona_id,
            plan,
        } => {
            let selection = inspect(&source, persona_id).map_err(|error| error.to_string())?;
            let plan =
                checkpoint_plan_output(&plan, &selection.source.path, &selection.destination_path)?;
            let bytes = serde_json::to_vec_pretty(&selection).map_err(|error| error.to_string())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&plan)
                .map_err(|error| format!("create inspection plan {}: {error}", plan.display()))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|error| format!("write inspection plan {}: {error}", plan.display()))?;
            println!("{}", String::from_utf8_lossy(&bytes));
            eprintln!(
                "checkpoint plan saved to {}; no checkpoint changed",
                plan.display()
            );
        }
        CheckpointCommand::Adopt { plan } => {
            // context-budget-exempt: bounds offline plan decoding, not model input.
            const MAX_PLAN_BYTES: u64 = 1024 * 1024;
            let mut bytes = Vec::new();
            std::fs::File::open(&plan)
                .and_then(|file| file.take(MAX_PLAN_BYTES + 1).read_to_end(&mut bytes))
                .map_err(|error| format!("read inspection plan {}: {error}", plan.display()))?;
            if bytes.len() as u64 > MAX_PLAN_BYTES {
                return Err("checkpoint plan exceeds the offline decoding limit".into());
            }
            let selection: AdoptionPlan = serde_json::from_slice(&bytes)
                .map_err(|error| format!("invalid checkpoint plan: {error}"))?;
            let receipt =
                adopt(&selection, ensure_checkpoint_offline).map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())?
            );
        }
    }
    Ok(())
}

pub(super) fn ensure_checkpoint_offline() -> std::io::Result<()> {
    // A launcher can have written its PID before the executable-name probe
    // identifies it. Read failure or a malformed PID is uncertainty, not absence.
    let recorded_pid = match std::fs::read_to_string(pidfile_for(&socket_path())) {
        Ok(contents) => {
            let pid = contents
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|pid| *pid > 0)
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid core PID file")
                })?;
            Some(pid)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    core_process_evidence()?.ensure_offline(recorded_pid)
}

/// A plan is metadata, never a checkpoint. In particular an absent destination
/// must not be poisoned with plan JSON by an otherwise successful create_new.
pub(super) fn checkpoint_plan_output(
    output: &Path,
    source: &Path,
    destination: &Path,
) -> Result<PathBuf, String> {
    let absolute = std::path::absolute(output).map_err(|error| error.to_string())?;
    let parent = absolute
        .parent()
        .ok_or("plan output has no parent")?
        .canonicalize()
        .map_err(|error| format!("plan output directory: {error}"))?;
    let filename = absolute.file_name().ok_or("plan output has no filename")?;
    if ["volatile.json", ".volatile.lock"]
        .iter()
        .any(|reserved| filename.to_string_lossy().eq_ignore_ascii_case(reserved))
    {
        return Err("inspection metadata cannot use a reserved checkpoint filename".into());
    }
    #[cfg(windows)]
    if parent.components().any(|component| {
        matches!(component,
        std::path::Component::Prefix(prefix) if matches!(prefix.kind(),
            std::path::Prefix::UNC(_, _) | std::path::Prefix::VerbatimUNC(_, _)))
    }) {
        // An SMB alias can identify the same Persona directory through a
        // different namespace, which lexical containment cannot establish.
        return Err("write the inspection plan to a local path, not a network share".into());
    }
    let output = parent.join(filename);
    let native_personas = destination
        .parent()
        .and_then(Path::parent)
        .ok_or("checkpoint destination has no Persona store")?;
    let source_personas = source
        .parent()
        .and_then(Path::parent)
        .ok_or("checkpoint source has no Persona store")?;
    if output.starts_with(native_personas) || output.starts_with(source_personas) {
        return Err("write the inspection plan outside managed Persona checkpoint storage".into());
    }
    Ok(output)
}
