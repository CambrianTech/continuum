use super::*;

pub(super) fn check_restoring_startup(
    snapshot: &RecoverySnapshot,
    tasks: &[TaskSnapshot],
) -> Result<(), String> {
    if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
        return Err("scheduler exclusion remains active".into());
    }
    snapshot.check_identity(tasks)?;
    if snapshot.tasks.iter().zip(tasks).any(|(saved, now)| {
        now.enabled != (saved.enabled && saved.name == super::super::super::super::CORE_TASK)
    }) {
        return Err(
            "restoration startup requires original core state and disabled deploy task".into(),
        );
    }
    Ok(())
}

pub(super) fn publish_restoring(
    journal: &Path,
    old_pin: File,
    snapshot: &RecoverySnapshot,
) -> Result<File, String> {
    snapshot.validate()?;
    if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
        return Err("only restoration phase can replace maintenance evidence".into());
    }
    let bytes = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?; // Durable maintenance phase wire.
    let temporary = journal.with_extension("json.restoring");
    crate::installed_release::unredirected(&temporary)?;
    let mut staged = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(&temporary)
        .map_err(|e| format!("cannot stage restoration phase: {e}"))?;
    if !staged.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("restoration stage is not a file".into());
    }
    let mut prior = Vec::new();
    (&mut staged)
        .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut prior)
        .map_err(|e| e.to_string())?;
    // The exact named stage may contain a crash-short prefix of this same
    // snapshot. Unknown/different bytes are retained, never overwritten.
    if !bytes.starts_with(&prior) {
        return Err("restoration staging identity differs; evidence retained".into());
    }
    staged
        .seek(SeekFrom::Start(0))
        .and_then(|_| staged.write_all(&bytes))
        .and_then(|_| staged.sync_all())
        .map_err(|e| e.to_string())?;
    drop(staged);
    drop(old_pin);
    continuum_host::durable::replace(&temporary, journal)
        .map_err(|e| format!("cannot publish restoration phase: {e}"))?;
    let (pin, actual) = read_snapshot(journal.parent().ok_or("journal root absent")?)?;
    let actual_bytes = serde_json::to_vec(&actual).map_err(|e| e.to_string())?; // Compare exact committed phase payload.
    if actual_bytes != bytes {
        return Err("published restoration evidence changed".into());
    }
    Ok(pin)
}

pub(super) fn read_snapshot(root: &Path) -> Result<(File, RecoverySnapshot), String> {
    let journal = root.join(JOURNAL);
    crate::installed_release::unredirected(&journal)?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&journal)
        .map_err(|e| format!("cannot pin maintenance evidence: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("maintenance recovery evidence is not a regular file".into());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > crate::installed_release::MAX_RECEIPT_BYTES {
        return Err("oversized maintenance recovery evidence retained".into());
    }
    let snapshot: RecoverySnapshot = serde_json::from_slice(&bytes)
        .map_err(|e| format!("corrupt maintenance evidence retained: {e}"))?;
    snapshot.validate()?;
    Ok((file, snapshot))
}

pub(super) fn require_idle_claim(root: &Path) -> Result<(), String> {
    let path = crate::deploy_claim::claim_path(root);
    match path.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("cannot establish deploy ownership: {e}")),
        Ok(_) => {}
    }
    let text = crate::installed_release::read_receipt(&path)?;
    let claim: crate::deploy_claim::DeployClaim = serde_json::from_str(&text)
        .map_err(|e| format!("unreadable deploy claim blocks maintenance: {e}"))?;
    if claim.pid <= 0 || crate::deploy_claim::owner_alive(claim.pid) {
        return Err("existing deploy owner must finish before legacy maintenance".into());
    }
    Ok(())
}

pub(super) fn previous_receipt(root: &Path) -> Result<Option<String>, String> {
    let path = root.join("install-previous.json");
    match path.symlink_metadata() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
        Ok(_) => crate::installed_release::read_receipt(&path).map(Some),
    }
}

pub(super) async fn check_legacy_sources(
    root: &Path,
    purpose: &LegacyPurpose,
    manifest: bool,
) -> Result<(), String> {
    require_idle_claim(root)?;
    if crate::installed_release::read_receipt(&root.join("install-active.json"))?
        != purpose.active
        || previous_receipt(root)? != purpose.previous
    {
        return Err("installed selections changed during legacy maintenance".into());
    }
    let core = super::super::super::super::task_report(super::super::super::super::CORE_TASK).await?;
    let envelope = crate::installed_release::envelope(&core.description)?
        .ok_or("protected bootstrap absent")?;
    if envelope.bootstrap != purpose.bootstrap {
        return Err("protected bootstrap changed during maintenance".into());
    }
    if manifest
        && crate::installed_release::read_receipt(
            &Path::new(&purpose.bootstrap)
                .parent()
                .ok_or("bootstrap parent absent")?
                .join("bootstrap-hashes.json"),
        )? != purpose.bootstrap_manifest
    {
        return Err("protected bootstrap closure changed during maintenance".into());
    }
    Ok(())
}

pub(super) fn canonical_source(source: &Path) -> Result<String, String> {
    if !source.is_absolute()
        || source
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("legacy source must be absolute and normalized".into());
    }
    crate::installed_release::unredirected(source)?;
    let physical = match source.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Successful retirement removes only the source leaf. Its existing
            // physical parent still binds recovery to the exact saved location.
            let parent = source.parent().ok_or("legacy source parent absent")?;
            let name = source.file_name().ok_or("legacy source filename absent")?;
            parent
                .canonicalize()
                .map_err(|e| format!("cannot resolve retired source parent: {e}"))?
                .join(name)
        }
        Err(error) => return Err(error.to_string()),
    };
    physical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "legacy source is not UTF-8".into())
}

pub(super) async fn refuse_own_image(root: &Path, purpose: &LegacyPurpose) -> Result<(), String> {
    let own = std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .map_err(|e| e.to_string())?;
    let bootstrap = Path::new(&purpose.bootstrap)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if own == bootstrap {
        return Err("maintenance must run outside the protected bootstrap".into());
    }
    let sid = super::super::super::super::caller_sid().await?;
    for text in std::iter::once(&purpose.active).chain(purpose.previous.iter()) {
        let validated = crate::installed_release::validate_receipt(
            text,
            root,
            Path::new(&purpose.payload),
            &sid,
        )?;
        let release = &validated.release;
        for image in [
            &release.artifact,
            &release.cli,
            &release.launcher,
            &release.engine,
        ] {
            if own == Path::new(image).canonicalize().map_err(|e| e.to_string())? {
                return Err("maintenance must run outside the managed release images".into());
            }
        }
    }
    Ok(())
}
