//! Online copying belongs to the same placement transaction. Its receipt lives
//! beside cold staging, never at the serving resolver's source-side boundary.
use super::*;
use std::collections::HashMap;

/// Read-only restart reconciliation. A lost preparation cannot reuse its prior
/// positive hash observation: Frozen cold bytes are verified again and retained.
pub struct RestorationPlacement {
    _source_lease: File,
    _destination_lease: Option<File>,
    _directory: Option<File>,
    _files: HashMap<PathBuf, File>,
}

pub fn hold_restoration(root: &Path, expected: &str) -> Result<RestorationPlacement, String> {
    let (root, source_lease) = shared_lease(root)?;
    let Some(placement) = read(&root)? else {
        return Ok(RestorationPlacement {
            _source_lease: source_lease,
            _destination_lease: None,
            _directory: None,
            _files: HashMap::new(),
        });
    };
    if placement.state != State::Frozen || operation_digest(&placement)? != expected {
        return Err("placement is not this operation's reconciled Frozen state".into());
    }
    match std::fs::symlink_metadata(&root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
        Ok(_) => {
            return Err(
                "source retirement remains incomplete; exact recovery required before restoration"
                    .into(),
            )
        }
    }
    let destination = placement.destination.decode()?;
    let destination_lease = exclusive(&sibling(&destination, ".placement.lock")?)?;
    let directory = directory_handle(&destination)?;
    let files = retain_verified(&destination, &placement.files)?;
    still_retained(&destination, &directory, &placement.files, &files)?;
    Ok(RestorationPlacement {
        _source_lease: source_lease,
        _destination_lease: Some(destination_lease),
        _directory: Some(directory),
        _files: files,
    })
}

/// Retained producer exclusion for the complete closure. Readers may continue.
/// On POSIX this must exclude legacy writers too: flock is only advisory.
/// Windows can instead acquire kernel custody through `prepare_online_windows`.
pub trait ProducerCustody: Sync {
    fn revalidate(&self, root: &Path) -> Result<(), String>;
}

/// A live preparation, not a serializable authorization. Losing these handles
/// requires re-hashing on restart. The cold receipt only owns staging recovery.
pub struct OnlinePreparation<'a> {
    placement: Placement,
    source: HashMap<PathBuf, File>,
    destination: HashMap<PathBuf, File>,
    source_directory: File,
    destination_directory: File,
    source_lease: File,
    _destination_lease: File,
    producer: ProducerAuthority<'a>,
}

// Acquisition intent is private. Windows custody is not established until every
// source member and its directory have been opened and the closure revalidated.
enum ProducerAuthority<'a> {
    External(&'a dyn ProducerCustody),
    #[cfg(windows)]
    WindowsHandles,
}

impl ProducerAuthority<'_> {
    fn revalidate(
        &self,
        root: &Path,
        directory: &File,
        files: &[Entry],
        held: &HashMap<PathBuf, File>,
    ) -> Result<(), String> {
        still_retained(root, directory, files, held)?;
        match self {
            Self::External(owner) => owner.revalidate(root),
            #[cfg(windows)]
            Self::WindowsHandles => Ok(()),
        }
    }
}

fn directory_handle(path: &Path) -> Result<File, String> {
    if !plain(path)?.is_dir() {
        return Err("container directory changed".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
        options
            .share_mode(1)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    options.open(path).map_err(|e| e.to_string())
}

fn identity(file: &File) -> Result<continuum_host::file_identity::FileIdentity, String> {
    continuum_host::file_identity::of(file).map_err(|e| e.to_string())
}

fn retain_verified(root: &Path, files: &[Entry]) -> Result<HashMap<PathBuf, File>, String> {
    verify_tree(root, files, false, false)?;
    files
        .iter()
        .map(|entry| {
            let name = entry_name(entry)?;
            let mut file = open_source(&root.join(&name))?;
            if digest(&mut file)? != (entry.bytes, entry.sha256.clone()) {
                return Err("prepared container content changed".into());
            }
            Ok((name, file))
        })
        .collect()
}

fn still_retained(
    root: &Path,
    directory: &File,
    files: &[Entry],
    held: &HashMap<PathBuf, File>,
) -> Result<(), String> {
    if crate::paths::cargo::physical_root(root)? != root
        || identity(directory)? != identity(&directory_handle(root)?)?
    {
        return Err("prepared container directory identity changed".into());
    }
    verify_tree(root, files, false, false)?;
    for entry in files {
        let name = entry_name(entry)?;
        let retained = held.get(&name).ok_or("prepared member handle missing")?;
        if identity(retained)? != identity(&open_source(&root.join(name))?)? {
            return Err("prepared container member identity changed".into());
        }
    }
    Ok(())
}

/// Prepare using configured cold placement and actual volume admission while
/// readers remain admitted. No source receipt or source deletion occurs here.
pub fn prepare_online<'a>(
    root: &Path,
    producer: &'a dyn ProducerCustody,
) -> Result<OnlinePreparation<'a>, String> {
    prepare_configured(root, ProducerAuthority::External(producer))
}

/// Acquire Windows kernel producer exclusion while permitting existing readers.
/// Every exact member and directory remains open without write/delete sharing.
/// The returned owner cannot authorize final reader/spawn exclusion by itself.
#[cfg(windows)]
pub fn prepare_online_windows(root: &Path) -> Result<OnlinePreparation<'static>, String> {
    prepare_configured(root, ProducerAuthority::WindowsHandles)
}

fn prepare_configured<'a>(
    root: &Path,
    producer: ProducerAuthority<'a>,
) -> Result<OnlinePreparation<'a>, String> {
    let storage =
        crate::paths::configured_storage_root().ok_or("cold storage is not configured")?;
    if !storage.is_absolute() || !storage.is_dir() {
        return Err("configured cold storage is unavailable".into());
    }
    let cold = std::fs::canonicalize(storage).map_err(|e| e.to_string())?;
    let root = crate::paths::cargo::physical_root(root)?;
    let source_volume = crate::paths::cargo::volume_id(&root)?;
    let cold_volume = crate::paths::cargo::volume_id(&cold)?;
    if source_volume == cold_volume {
        return Err("cold placement would not relieve source volume".into());
    }
    prepare_owned(
        &root,
        &cold.join("models/expert-containers"),
        producer,
        &|bytes| {
            if crate::paths::cargo::volume_id(&cold)? != cold_volume {
                return Err("cold volume changed".into());
            }
            let drives = crate::system_resources::drives::detect_drives();
            let drive = crate::system_resources::drives::drive_holding(&drives, &cold)
                .ok_or("cold volume unavailable")?;
            crate::paging::pool::ensure_projected_disk_capacity(
                "expert-containers",
                drive.total_bytes.saturating_sub(drive.available_bytes),
                drive.total_bytes,
                bytes,
            )
            .map_err(|e| e.to_string())
        },
    )
}

#[cfg(test)]
pub(super) fn prepare_in<'a>(
    root: &Path,
    cold: &Path,
    producer: &'a dyn ProducerCustody,
    admit: &dyn Fn(u64) -> Result<(), String>,
) -> Result<OnlinePreparation<'a>, String> {
    prepare_owned(root, cold, ProducerAuthority::External(producer), admit)
}

#[cfg(all(test, windows))]
pub(super) fn prepare_windows_in(
    root: &Path,
    cold: &Path,
    admit: &dyn Fn(u64) -> Result<(), String>,
) -> Result<OnlinePreparation<'static>, String> {
    prepare_owned(root, cold, ProducerAuthority::WindowsHandles, admit)
}

fn prepare_owned<'a>(
    root: &Path,
    cold: &Path,
    producer: ProducerAuthority<'a>,
    admit: &dyn Fn(u64) -> Result<(), String>,
) -> Result<OnlinePreparation<'a>, String> {
    let (root, source_lease) = reader_lease(root)?;
    match &producer {
        ProducerAuthority::External(owner) => owner.revalidate(&root)?,
        #[cfg(windows)]
        ProducerAuthority::WindowsHandles => {}
    }
    let source_directory = directory_handle(&root)?;
    let observed = ExpertContainer::observe_complete(&root).map_err(|e| e.to_string())?;
    let mut source = HashMap::new();
    let mut files = Vec::new();
    for member in observed.files {
        let name = PathBuf::from(member.name);
        let mut file = open_source(&root.join(&name))?;
        let (bytes, sha256) = digest(&mut file)?;
        if bytes != member.bytes {
            return Err("container changed during preparation".into());
        }
        files.push(Entry {
            name: NativePath::encode(&name),
            bytes,
            sha256,
        });
        source.insert(name, file);
    }
    let cold = crate::paths::cargo::physical_root(cold)?;
    let destination = cold.join(closure_identity(&files)?);
    if root.starts_with(&destination) || destination.starts_with(&root) {
        return Err("overlapping container placement".into());
    }
    let placement = Placement {
        schema: 1,
        source: NativePath::encode(&root),
        destination: NativePath::encode(&destination),
        manifest: observed.manifest_bytes,
        files,
        state: State::Copying,
    };
    validate_manifest_provenance(&placement)?;
    producer.revalidate(&root, &source_directory, &placement.files, &source)?;
    admit(0)?;
    std::fs::create_dir_all(&cold).map_err(|e| e.to_string())?;
    if std::fs::canonicalize(&cold).map_err(|e| e.to_string())? != cold {
        return Err("cold preparation root redirected".into());
    }
    let destination_lease = exclusive(&sibling(&destination, ".placement.lock")?)?;
    let staging = sibling(&destination, ".copying")?;
    let preparation_receipt = sibling(&destination, ".preparation.json")?;
    if !destination.try_exists().map_err(|e| e.to_string())? {
        let prior = read_at(&root, &preparation_receipt)?;
        if prior.as_ref().is_some_and(|p| {
            p.state != State::Copying
                || p.files != placement.files
                || p.destination != placement.destination
                || p.manifest != placement.manifest
        }) {
            return Err("cold preparation ownership changed".into());
        }
        let partial = if staging.try_exists().map_err(|e| e.to_string())? {
            if prior.is_none() {
                return Err("unowned cold staging".into());
            }
            interrupted_members(&staging, &placement.files)?
        } else {
            Vec::new()
        };
        let mut missing = 0u64;
        for entry in &placement.files {
            let name = entry_name(entry)?;
            if !staging
                .join(&name)
                .try_exists()
                .map_err(|e| e.to_string())?
                || partial.contains(&name)
            {
                missing = missing
                    .checked_add(entry.bytes)
                    .ok_or("container size overflow")?;
            }
        }
        admit(missing)?;
        producer.revalidate(&root, &source_directory, &placement.files, &source)?;
        publish_at(&preparation_receipt, &placement)?;
        std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        if std::fs::canonicalize(&staging).map_err(|e| e.to_string())? != staging {
            return Err("cold staging redirected".into());
        }
        for name in partial {
            std::fs::remove_file(staging.join(name)).map_err(|e| e.to_string())?;
        }
        copy_members(&staging, &placement.files, &mut source, true)?;
        verify_tree(&staging, &placement.files, false, true)?;
        continuum_host::durable::replace(&staging, &destination).map_err(|e| e.to_string())?;
    }
    // Existing published destinations are verified completely and never repaired.
    let destination_directory = directory_handle(&destination)?;
    let destination_handles = retain_verified(&destination, &placement.files)?;
    producer.revalidate(&root, &source_directory, &placement.files, &source)?;
    Ok(OnlinePreparation {
        placement,
        source,
        destination: destination_handles,
        source_directory,
        destination_directory,
        source_lease,
        _destination_lease: destination_lease,
        producer,
    })
}

impl OnlinePreparation<'_> {
    /// Bind maintenance recovery to this exact source, destination and immutable
    /// closure. The digest grants no custody and survives the Frozen transition.
    pub fn operation_digest(&self) -> Result<String, String> {
        super::operation_digest(&self.placement)
    }

    pub fn source_root(&self) -> Result<PathBuf, String> {
        self.placement.source.decode()
    }

    /// Called only after the existing maintenance owner drains readers and
    /// excludes all current/rollback/manual legacy launches. No long copy or
    /// content hash occurs while this final exclusion is held.
    pub fn freeze(self, custody: &dyn ContainerMigrationCustody) -> Result<u64, String> {
        let Self {
            mut placement,
            source,
            destination,
            source_directory,
            destination_directory,
            source_lease,
            _destination_lease,
            producer,
        } = self;
        let root = placement.source.decode()?;
        let cold = placement.destination.decode()?;
        custody.revalidate(&root, MigrationStage::Observe)?;
        producer.revalidate(&root, &source_directory, &placement.files, &source)?;
        drop(source_lease);
        // No blocking upgrade: a remaining reader causes refusal, not deadlock.
        let _exclusive = exclusive(&sibling(&root, ".container-placement.lock")?)?;
        if read(&root)?.is_some() {
            return Err("source placement changed during preparation".into());
        }
        still_retained(&root, &source_directory, &placement.files, &source)?;
        still_retained(
            &cold,
            &destination_directory,
            &placement.files,
            &destination,
        )?;
        producer.revalidate(&root, &source_directory, &placement.files, &source)?;
        custody.revalidate(&root, MigrationStage::Publish)?;
        placement.state = State::Frozen;
        publish(&root, &placement)?;
        custody.revalidate(&root, MigrationStage::Retire)?;
        drop(source);
        drop(source_directory);
        retire_recorded(&root, &placement.files, false)
    }
}
