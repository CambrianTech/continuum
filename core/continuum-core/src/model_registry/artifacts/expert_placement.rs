//! Whole expert-container placement, owned by the model artifact resolver.
//!
//! The adjacent receipt survives retirement of the original directory. It is
//! shared filesystem protocol state, not another catalog or background manager.
//! Only a custody owner excluding producers, legacy readers AND all spawn/
//! rollback authorities may execute a freeze. No production legacy-custody
//! implementation is supplied here: a receipt is not evidence of quiescence.
use crate::capacity::expert_container::ExpertContainer;
use crate::paths::native::NativePath;
use continuum_cli_lifecycle::installed_release::MAX_RECEIPT_BYTES;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

mod preparation;
pub use preparation::{prepare_online, OnlinePreparation, ProducerCustody};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationStage {
    Observe,
    Copy,
    Publish,
    Retire,
}

/// A retained exclusion lease, not a boolean operator assertion. Implementors
/// must own producer/reader lifetime and prevent new legacy raw-path readers,
/// including rollback launches, until this object is dropped. Revalidation must
/// refuse changed ownership/generation. Ordinary active-path snapshots alone do
/// not satisfy this contract. A failed check never permits source retirement.
pub trait ContainerMigrationCustody {
    fn revalidate(&self, root: &Path, stage: MigrationStage) -> Result<(), String>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Entry {
    name: NativePath,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum State {
    Copying,
    Frozen,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    schema: u32,
    source: NativePath,
    destination: NativePath,
    /// Exact producer bytes, including fields this reader does not interpret.
    manifest: Vec<u8>,
    files: Vec<Entry>,
    state: State,
}

fn sibling(root: &Path, suffix: &str) -> Result<PathBuf, String> {
    let mut name = root
        .file_name()
        .ok_or("container has no filename")?
        .to_os_string();
    name.push(suffix);
    Ok(root.with_file_name(name))
}

fn receipt_path(root: &Path) -> Result<PathBuf, String> {
    sibling(root, ".container-placement.json")
}

fn plain(path: &Path) -> Result<std::fs::Metadata, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let redirected = meta.file_type().is_symlink();
    if redirected {
        return Err(format!("redirected container entry: {}", path.display()));
    }
    Ok(meta)
}

fn check_existing_plain_file(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
        Ok(_) if plain(path)?.is_file() => Ok(()),
        Ok(_) => Err(format!("not a plain file: {}", path.display())),
    }
}

fn exclusive(path: &Path) -> Result<File, String> {
    check_existing_plain_file(path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    FileExt::try_lock_exclusive(&file)
        .map_err(|e| format!("container placement already owned: {e}"))?;
    Ok(file)
}

/// Retained by a reader before interpreting the manifest or lazily opening
/// banks. The native reader uses the same whole-file OS lock protocol.
pub(crate) fn reader_lease(root: &Path) -> Result<(PathBuf, File), String> {
    let root = crate::paths::cargo::physical_root(root)?;
    let path = sibling(&root, ".container-placement.lock")?;
    check_existing_plain_file(&path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    FileExt::try_lock_shared(&file).map_err(|e| format!("container writer owns placement: {e}"))?;
    // Check after acquiring custody: a writer cannot publish between this
    // refusal boundary and the caller's first manifest read.
    resolve_for_serving(&root)?;
    Ok((root, file))
}

fn entry_name(entry: &Entry) -> Result<PathBuf, String> {
    let name = entry.name.decode()?;
    let mut components = name.components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err("placement entry is not one native filename".into());
    }
    Ok(name)
}

fn read(root: &Path) -> Result<Option<Placement>, String> {
    let physical = crate::paths::cargo::physical_root(root)?;
    let root = physical.as_path();
    let path = receipt_path(root)?;
    read_at(root, &path)
}

fn read_at(root: &Path, path: &Path) -> Result<Option<Placement>, String> {
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    if !plain(&path)?.is_file() {
        return Err("placement receipt is not a file".into());
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|file| file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("placement receipt exceeds lifecycle bound".into());
    }
    let placement: Placement =
        serde_json::from_slice(&bytes).map_err(|e| format!("placement receipt corrupt: {e}"))?;
    if placement.schema != 1 || placement.source.decode()? != root || placement.files.is_empty() {
        return Err("placement receipt identity/schema mismatch".into());
    }
    let mut names = std::collections::HashSet::new();
    for entry in &placement.files {
        if !names.insert(entry_name(entry)?) || entry.sha256.len() != 64 {
            return Err("placement closure contains duplicate or invalid entries".into());
        }
    }
    validate_manifest_provenance(&placement)?;
    let destination = placement.destination.decode()?;
    if !destination.is_absolute()
        || destination.file_name().and_then(|name| name.to_str())
            != Some(closure_identity(&placement.files)?.as_str())
    {
        return Err("placement content address mismatch".into());
    }
    Ok(Some(placement))
}

fn closure_identity(files: &[Entry]) -> Result<String, String> {
    let closure = serde_json::to_vec(files).map_err(|e| e.to_string())?; // Identity protocol: exact ordered native filename/size/content closure.
    Ok(format!("{:x}", Sha256::digest(&closure)))
}

fn validate_manifest_provenance(placement: &Placement) -> Result<(), String> {
    let manifest = placement
        .files
        .iter()
        .find(|entry| entry_name(entry).ok().as_deref() == Some(Path::new("manifest.json")))
        .ok_or("placement has no original manifest")?;
    if manifest.bytes != placement.manifest.len() as u64
        || manifest.sha256 != format!("{:x}", Sha256::digest(&placement.manifest))
    {
        return Err("placement manifest provenance mismatch".into());
    }
    Ok(())
}

fn publish(root: &Path, placement: &Placement) -> Result<(), String> {
    publish_to(
        &receipt_path(root)?,
        &sibling(root, ".container-placement.tmp")?,
        placement,
    )
}

fn publish_at(path: &Path, placement: &Placement) -> Result<(), String> {
    let temporary = sibling(path, ".tmp")?;
    publish_to(path, &temporary, placement)
}

fn publish_to(path: &Path, temporary: &Path, placement: &Placement) -> Result<(), String> {
    validate_manifest_provenance(placement)?;
    let bytes = serde_json::to_vec(placement).map_err(|e| e.to_string())?; // Disk boundary: placement protocol consumed by later resolver/migration processes.
    if bytes.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("placement closure exceeds lifecycle receipt bound".into());
    }
    // The migration lease serializes this exact staging filename. A previous
    // interrupted writer's regular staging file is replaced, never a redirect.
    check_existing_plain_file(&temporary)?;
    let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    crate::fs_portable::durable_rename(&temporary, path).map_err(|e| e.to_string())
}

/// Blocking artifact-resolution boundary. Frozen and interrupted placement
/// never silently resolve to cold storage; promotion is a distinct operation.
pub fn resolve_for_serving(root: &Path) -> Result<PathBuf, String> {
    let root = crate::paths::cargo::physical_root(root)?;
    match read(&root)? {
        None => Ok(root),
        Some(placement) => Err(match placement.state {
            State::Copying => "expert container placement is incomplete; serving refused".into(),
            State::Frozen => {
                "expert container is frozen; supported hot-tier promotion is required".into()
            }
        }),
    }
}

fn digest(file: &mut File) -> Result<(u64, String), String> {
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .ok_or("container file size overflow")?;
        hash.update(&buffer[..count]);
    }
    Ok((size, format!("{:x}", hash.finalize())))
}

fn open_source(path: &Path) -> Result<File, String> {
    if !plain(path)?.is_file() {
        return Err("source member is not a plain file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Retain exact source handles through hash/copy/publication. Existing
        // write/delete handles cause refusal rather than a racy snapshot.
        options.share_mode(1);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    FileExt::try_lock_shared(&file)
        .map_err(|e| format!("container source writer owns {}: {e}", path.display()))?;
    Ok(file)
}

fn verify_file_shape(path: &Path, entry: &Entry) -> Result<(), String> {
    let metadata = plain(path)?;
    if !metadata.is_file() || metadata.len() != entry.bytes {
        return Err("container member is not a regular file".into());
    }
    Ok(())
}

fn verify_file(path: &Path, entry: &Entry) -> Result<(), String> {
    verify_file_shape(path, entry)?;
    let measured = digest(&mut File::open(path).map_err(|e| e.to_string())?)?;
    if measured != (entry.bytes, entry.sha256.clone()) {
        return Err(format!("container closure changed: {}", path.display()));
    }
    Ok(())
}

fn verify_tree(
    root: &Path,
    files: &[Entry],
    allow_missing: bool,
    contents: bool,
) -> Result<(), String> {
    if !plain(root)?.is_dir() {
        return Err("container root is not a plain directory".into());
    }
    let expected: std::collections::HashMap<_, _> = files
        .iter()
        .map(|entry| Ok((entry_name(entry)?, entry)))
        .collect::<Result<_, String>>()?;
    let mut seen = 0usize;
    for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let expected = expected
            .get(Path::new(&entry.file_name()))
            .ok_or("unexpected container entry; retirement refused")?;
        if contents {
            verify_file(&entry.path(), expected)?;
        } else {
            verify_file_shape(&entry.path(), expected)?;
        }
        seen += 1;
    }
    if !allow_missing && seen != files.len() {
        return Err("container closure is incomplete".into());
    }
    Ok(())
}

// Only an unpublished tree named by an existing Copying receipt can contain
// interrupted writes. Unknown members and oversized files are never repaired.
fn interrupted_members(root: &Path, files: &[Entry]) -> Result<Vec<PathBuf>, String> {
    if !plain(root)?.is_dir() {
        return Err("staging root is not a plain directory".into());
    }
    let expected: std::collections::HashMap<_, _> = files
        .iter()
        .map(|entry| Ok((entry_name(entry)?, entry)))
        .collect::<Result<_, String>>()?;
    let mut partial = Vec::new();
    for member in std::fs::read_dir(root).map_err(|e| e.to_string())? {
        let member = member.map_err(|e| e.to_string())?;
        let name = PathBuf::from(member.file_name());
        let entry = expected.get(&name).ok_or("unknown staging member")?;
        let metadata = plain(&member.path())?;
        if !metadata.is_file() || metadata.len() > entry.bytes {
            return Err("invalid staging member; recovery refused".into());
        }
        if metadata.len() < entry.bytes {
            partial.push(name);
        }
    }
    Ok(partial)
}

/// Invoked by the existing serving-tier owner in its blocking eviction context.
/// The configured storage root must already exist; no home-volume fallback.
pub fn freeze(root: &Path, custody: &dyn ContainerMigrationCustody) -> Result<u64, String> {
    let storage =
        crate::paths::configured_storage_root().ok_or("cold storage is not configured")?;
    if !storage.is_absolute() || !storage.is_dir() {
        return Err("configured cold storage is unavailable".into());
    }
    let cold = std::fs::canonicalize(storage).map_err(|e| e.to_string())?;
    let root = crate::paths::cargo::physical_root(root)?;
    let drives = crate::capacity::system_profile::detect_drives();
    let source_drive = crate::capacity::system_profile::drive_holding(&drives, &root)
        .ok_or("source volume unavailable")?;
    let cold_drive = crate::capacity::system_profile::drive_holding(&drives, &cold)
        .ok_or("cold volume unavailable")?;
    if source_drive.mount == cold_drive.mount {
        return Err("cold placement would not relieve the source volume".into());
    }
    freeze_in(
        &root,
        &cold.join("models/expert-containers"),
        custody,
        &|bytes| {
            // Closure hashing can be lengthy. Admission must sample the actual
            // destination at the write boundary, not reuse its pre-hash free
            // bytes. This remains admission rather than a reservation.
            let current = crate::capacity::system_profile::detect_drives();
            let observed = crate::capacity::system_profile::drive_holding(&current, &cold)
                .ok_or("cold volume disappeared before copy")?;
            if observed.mount != cold_drive.mount {
                return Err("cold volume changed before copy".into());
            }
            crate::paging::pool::ensure_projected_disk_capacity(
                "expert-containers",
                observed
                    .total_bytes
                    .saturating_sub(observed.available_bytes),
                observed.total_bytes,
                bytes,
            )
            .map_err(|e| e.to_string())
        },
    )
}

fn copy_members(
    root: &Path,
    files: &[Entry],
    sources: &mut std::collections::HashMap<PathBuf, File>,
    allow_create: bool,
) -> Result<(), String> {
    for entry in files {
        let name = entry_name(entry)?;
        let path = root.join(&name);
        if !path.try_exists().map_err(|e| e.to_string())? {
            if !allow_create {
                return Err("published container changed after verification".into());
            }
            let source = sources.get_mut(&name).ok_or("source handle missing")?;
            source.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            let mut target = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            let copied = std::io::copy(&mut *source, &mut target).map_err(|e| e.to_string())?;
            if copied != entry.bytes {
                return Err("source length changed during copy".into());
            }
            target
                .set_permissions(source.metadata().map_err(|e| e.to_string())?.permissions())
                .map_err(|e| e.to_string())?;
            crate::fs_portable::sync_publication(&target, &path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn freeze_in(
    root: &Path,
    cold: &Path,
    custody: &dyn ContainerMigrationCustody,
    admit: &dyn Fn(u64) -> Result<(), String>,
) -> Result<u64, String> {
    let physical = crate::paths::cargo::physical_root(root)?;
    let root = physical.as_path();
    let cold_physical = crate::paths::cargo::physical_root(cold)?;
    let cold = cold_physical.as_path();
    custody.revalidate(root, MigrationStage::Observe)?;
    let lock_path = sibling(root, ".container-placement.lock")?;
    let _source_lease = exclusive(&lock_path)?;
    let mut source_handles = std::collections::HashMap::new();
    let saved = read(root)?;
    let resuming = saved.is_some();
    let mut placement = if let Some(saved) = saved {
        saved
    } else {
        let observed = ExpertContainer::observe_complete(root).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        for member in observed.files {
            let path = root.join(&member.name);
            let mut file = open_source(&path)?;
            let (bytes, sha256) = digest(&mut file)?;
            if bytes != member.bytes {
                return Err("container changed during closure observation".into());
            }
            files.push(Entry {
                name: NativePath::encode(Path::new(&member.name)),
                bytes,
                sha256,
            });
            source_handles.insert(PathBuf::from(member.name), file);
        }
        let identity = closure_identity(&files)?;
        Placement {
            schema: 1,
            source: NativePath::encode(root),
            destination: NativePath::encode(&cold.join(identity)),
            manifest: observed.manifest_bytes,
            files,
            state: State::Copying,
        }
    };
    let destination = placement.destination.decode()?;
    if destination.parent() != Some(cold)
        || root.starts_with(&destination)
        || destination.starts_with(root)
    {
        return Err("saved destination is outside the admitted cold root".into());
    }
    // Identical containers can come from different hot roots. Their source
    // leases differ, so serialize publication at the shared content address too.
    if !cold.try_exists().map_err(|e| e.to_string())? {
        admit(0)?;
        std::fs::create_dir_all(cold).map_err(|e| e.to_string())?;
    }
    if !plain(cold)?.is_dir() || std::fs::canonicalize(cold).map_err(|e| e.to_string())? != cold {
        return Err("cold root changed during placement".into());
    }
    let _destination_lease = exclusive(&sibling(&destination, ".placement.lock")?)?;
    let newly_frozen = placement.state == State::Copying;
    if newly_frozen {
        verify_tree(root, &placement.files, false, false)?;
        for entry in &placement.files {
            let name = entry_name(entry)?;
            if !source_handles.contains_key(&name) {
                let mut file = open_source(&root.join(&name))?;
                if digest(&mut file)? != (entry.bytes, entry.sha256.clone()) {
                    return Err("source handle no longer matches immutable closure".into());
                }
                source_handles.insert(name.clone(), file);
            }
        }
        let staging = sibling(&destination, ".copying")?;
        let copy_root = if destination.try_exists().map_err(|e| e.to_string())? {
            &destination
        } else {
            &staging
        };
        let partial = if copy_root.try_exists().map_err(|e| e.to_string())? {
            if copy_root == &staging {
                if !resuming {
                    return Err("staging tree lacks this source's durable receipt".into());
                }
                interrupted_members(copy_root, &placement.files)?
            } else {
                // A published content address may be shared by another source.
                // It is immutable: missing or corrupt members refuse, never
                // turn this placement into an in-place repair writer.
                verify_tree(copy_root, &placement.files, false, true)?;
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let mut missing = 0u64;
        for entry in &placement.files {
            let path = copy_root.join(entry_name(entry)?);
            if path.try_exists().map_err(|e| e.to_string())?
                && !partial.contains(&entry_name(entry)?)
            {
                verify_file_shape(&path, entry)?;
            } else {
                missing = missing
                    .checked_add(entry.bytes)
                    .ok_or("container size overflow")?;
            }
        }
        admit(missing)?;
        custody.revalidate(root, MigrationStage::Copy)?;
        publish(root, &placement)?;
        std::fs::create_dir_all(copy_root).map_err(|e| e.to_string())?;
        if std::fs::canonicalize(copy_root).map_err(|e| e.to_string())? != *copy_root {
            return Err("cold placement path changed during admission".into());
        }
        // Admission counts a complete replacement conservatively. Both leases
        // and the external custody contract remain held; published payloads
        // never enter this reconciliation path.
        for name in partial {
            let path = copy_root.join(name);
            if !plain(&path)?.is_file() {
                return Err("staging member changed during admission".into());
            }
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        copy_members(
            copy_root,
            &placement.files,
            &mut source_handles,
            copy_root != &destination,
        )?;
        verify_tree(copy_root, &placement.files, false, true)?;
        if copy_root != &destination {
            // Directory publication fences the destination name before a
            // receipt can point to it. The source still exists on every error.
            crate::fs_portable::durable_rename(copy_root, &destination)
                .map_err(|e| e.to_string())?;
        }
        verify_tree(root, &placement.files, false, false)?;
        custody.revalidate(root, MigrationStage::Publish)?;
        placement.state = State::Frozen;
        publish(root, &placement)?;
    }
    if !newly_frozen {
        verify_tree(&destination, &placement.files, false, true)?;
    }
    custody.revalidate(root, MigrationStage::Retire)?;
    // Windows denies source deletion while these handles are open. The
    // external lifetime/spawn lease remains held across this release and the
    // exact-entry retirement below; an advisory placement lock alone is not it.
    drop(source_handles);
    retire_recorded(root, &placement.files, !newly_frozen)
}

fn retire_recorded(root: &Path, files: &[Entry], verify_contents: bool) -> Result<u64, String> {
    if !root.try_exists().map_err(|e| e.to_string())? {
        return Ok(0);
    }
    // A Frozen receipt makes interrupted retirement resumable. Only its exact
    // remaining entries may be removed, never a recursively discovered tree.
    verify_tree(root, files, true, verify_contents)?;
    let mut freed = 0u64;
    for entry in files {
        let path = root.join(entry_name(entry)?);
        if path.try_exists().map_err(|e| e.to_string())? {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
            freed += entry.bytes;
        }
    }
    std::fs::remove_dir(root).map_err(|e| e.to_string())?;
    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::expert_container::fixtures::write_container;
    struct Custody {
        stop: Option<MigrationStage>,
    }
    impl ContainerMigrationCustody for Custody {
        fn revalidate(&self, _: &Path, stage: MigrationStage) -> Result<(), String> {
            if self.stop == Some(stage) {
                Err("owner generation changed".into())
            } else {
                Ok(())
            }
        }
    }

    impl ProducerCustody for Custody {
        fn revalidate(&self, _: &Path) -> Result<(), String> {
            if self.stop == Some(MigrationStage::Copy) {
                Err("producer custody lost".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn online_preparation_preserves_readers_until_explicit_final_exclusion() {
        // Regression: the long copy must neither publish resolver refusal nor
        // obtain an exclusive lease that rejects new readers.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        let custody = Custody { stop: None };
        let reader = ExpertContainer::open(&source).unwrap();
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        assert!(read(&source).unwrap().is_none());
        assert!(ExpertContainer::open(&source).is_ok());
        #[cfg(windows)]
        assert!(OpenOptions::new()
            .write(true)
            .open(source.join("manifest.json"))
            .is_err());
        // The maintained legacy-reader contract is not replaced by the trait
        // assertion: an actual cooperating reader still prevents the upgrade.
        assert!(prepared.freeze(&custody).is_err());
        assert!(read(&source).unwrap().is_none());
        assert!(source.join("manifest.json").exists());
        drop(reader);
        // Restart reacquires and re-hashes; a prior cold copy is not a saved
        // live-handle authorization. Failed final custody preserves serving.
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        assert!(prepared
            .freeze(&Custody {
                stop: Some(MigrationStage::Publish)
            })
            .is_err());
        assert!(read(&source).unwrap().is_none());
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        assert!(prepared.freeze(&custody).unwrap() > 0);
        assert!(!source.exists());
        assert!(resolve_for_serving(&source).is_err());
    }

    #[test]
    fn actual_lazy_reader_excludes_freeze_until_its_lifetime_ends() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        let reader = ExpertContainer::open(&source).unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(read(&source).unwrap().is_none());
        assert!(!cold.exists());
        drop(reader);
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap() > 0);
    }

    #[tokio::test]
    #[ignore = "existing Unix/Windows CI runs this with the compiled native custody probe"]
    async fn fs2_and_native_container_locks_interoperate_across_processes() {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let native = std::env::var_os("CONTINUUM_NATIVE_CONTAINER_LEASE_TEST")
            .expect("native custody fixture must be compiled by the owning CI step");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("container");
        std::fs::create_dir(&root).unwrap();
        let probe = |mode: &'static str| {
            let native = native.clone();
            let root = root.clone();
            async move {
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    tokio::process::Command::new(native)
                        .args(["--container-lease"])
                        .arg(root)
                        .arg(mode)
                        .kill_on_drop(true)
                        .status(),
                )
                .await
                .unwrap()
                .unwrap()
                .code()
                .unwrap()
            }
        };
        let (_, shared) = reader_lease(&root).unwrap();
        assert_eq!(probe("reader").await, 0);
        assert_eq!(probe("writer").await, 3);
        drop(shared);
        let writer = exclusive(&sibling(&root, ".container-placement.lock").unwrap()).unwrap();
        assert_eq!(probe("reader").await, 3);
        assert_eq!(probe("writer").await, 3);
        drop(writer);
        assert_eq!(probe("writer").await, 0);
        for mode in ["reader", "writer"] {
            let mut child = tokio::process::Command::new(&native)
                .arg("--container-lease")
                .arg(&root)
                .arg(mode)
                .arg("hold")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                output.read_line(&mut ready),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(ready.trim(), "held");
            assert!(exclusive(&sibling(&root, ".container-placement.lock").unwrap()).is_err());
            assert_eq!(reader_lease(&root).is_ok(), mode == "reader");
            child.stdin.take().unwrap().write_all(b"\n").await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .success()
            );
            assert!(exclusive(&sibling(&root, ".container-placement.lock").unwrap()).is_ok());
        }
        #[cfg(windows)]
        {
            let member = root.join("mapped-member");
            std::fs::write(&member, b"fixture bytes").unwrap();
            let mut child = tokio::process::Command::new(&native)
                .arg("--container-write-map")
                .arg(&member)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                output.read_line(&mut ready),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                ready.trim(),
                "held",
                "native mapping must exist after its file handle closes"
            );
            assert!(
                open_source(&member).is_err(),
                "pre-existing writable mapping must exclude immutable preparation"
            );
            child.stdin.take().unwrap().write_all(b"\n").await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .success()
            );
            assert!(
                open_source(&member).is_ok(),
                "mapping release restores preparation admission"
            );
        }
    }

    #[test]
    fn interrupted_unpublished_member_is_replaced_only_after_readmission() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        assert_eq!(saved.state, State::Copying);
        let destination = saved.destination.decode().unwrap();
        let staging = sibling(&destination, ".copying").unwrap();
        std::fs::rename(&destination, &staging).unwrap();
        let member = staging.join("experts-L0.bin");
        std::fs::write(&member, b"partial").unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Err(
            "capacity".into()
        ))
        .is_err());
        assert_eq!(std::fs::read(&member).unwrap(), b"partial");
        assert!(source.exists());
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        assert!(!staging.exists());
        assert_eq!(
            std::fs::read(destination.join("experts-L0.bin")).unwrap(),
            std::fs::read(source.join("experts-L0.bin")).unwrap()
        );
        assert_eq!(read(&source).unwrap().unwrap().state, State::Frozen);
    }

    #[test]
    fn missing_published_member_refuses_without_repairing_shared_destination() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        let destination = saved.destination.decode().unwrap();
        let missing = destination.join("experts-L0.bin");
        std::fs::remove_file(&missing).unwrap();
        let manifest = std::fs::read(destination.join("manifest.json")).unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(!missing.exists());
        assert!(source.join("experts-L0.bin").exists());
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            manifest
        );
        assert_eq!(read(&source).unwrap().unwrap().state, State::Copying);
    }

    #[test]
    fn freeze_preserves_original_bytes_and_resumes_only_after_verified_publication() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 2, 2);
        let original = std::fs::read(source.join("manifest.json")).unwrap();
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        assert!(source.join("experts-L0.bin").exists());
        assert_eq!(read(&source).unwrap().unwrap().state, State::Frozen);
        assert!(resolve_for_serving(&source).unwrap_err().contains("frozen"));
        let destination = read(&source)
            .unwrap()
            .unwrap()
            .destination
            .decode()
            .unwrap();
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            original
        );
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap() > 0);
        assert!(!source.exists());
        assert!(resolve_for_serving(&source).is_err());
        assert_eq!(
            freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap(),
            0
        );
    }

    #[test]
    fn capacity_or_custody_refusal_keeps_source_and_corrupt_copy_never_retires_it() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Err(
            "capacity".into()
        ))
        .is_err());
        assert!(!cold.exists());
        assert!(read(&source).unwrap().is_none());
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        assert_eq!(saved.state, State::Copying);
        std::fs::write(
            saved.destination.decode().unwrap().join("experts-L0.bin"),
            b"changed",
        )
        .unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(source.join("experts-L0.bin").exists());
    }

    #[test]
    fn custody_changes_and_foreign_source_entries_never_authorize_retirement() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        for stage in [MigrationStage::Observe, MigrationStage::Copy] {
            assert!(
                freeze_in(&source, &cold, &Custody { stop: Some(stage) }, &|_| Ok(())).is_err()
            );
            assert!(read(&source).unwrap().is_none());
            assert!(source.join("experts-L0.bin").exists());
        }
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        std::fs::write(source.join("later-owner-data"), b"preserve").unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert_eq!(
            std::fs::read(source.join("later-owner-data")).unwrap(),
            b"preserve"
        );
        assert!(source.join("experts-L0.bin").exists());
        std::fs::write(receipt_path(&source).unwrap(), b"broken receipt").unwrap();
        assert!(resolve_for_serving(&source).is_err());
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
    }
}
