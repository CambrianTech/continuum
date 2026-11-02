//! Checkpoint paths, locks and writes share the personas disk-class owner.
use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use std::{
    io,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(super) fn writer(owner: Uuid) -> io::Result<ClassWriter> {
    #[cfg(not(any(test, feature = "test-fixtures")))]
    let class = ClassWriter::of("personas")?;
    // Explicit test layout, from the same class catalog as boot. Resolve on this
    // thread so NativeHomeOverride and blocking-worker fixture custody agree.
    #[cfg(any(test, feature = "test-fixtures"))]
    let class = {
        let native = crate::paths::home_dir()
            .ok_or_else(|| io::Error::other("fixture has no native home"))?;
        let home = crate::paths::continuum_home().map_err(io::Error::other)?;
        continuum_kernel::system_resources::disk_reporters::standard_tracked_dirs(&native, &home)
            .into_iter()
            .find(|class| class.class() == "personas")
            .ok_or_else(|| io::Error::other("fixture catalog has no personas class"))?
            .writer()
    };
    Ok(class.owned_by(owner))
}

pub(super) fn path(owner: Uuid) -> io::Result<PathBuf> {
    writer(owner)?.path(Path::new("volatile.json"))
}

pub(super) fn for_path(path: &Path) -> io::Result<ClassWriter> {
    let owner = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "checkpoint has no typed owner")
        })?;
    let writer = writer(owner)?;
    if resolved_path(&std::path::absolute(path)?)?
        != resolved_path(&std::path::absolute(
            writer.path(Path::new("volatile.json"))?,
        )?)?
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "checkpoint is outside its owner class",
        ));
    }
    Ok(writer)
}

/// Resolve existing ancestors before comparing class custody. Temporary homes on
/// macOS may use /var while canonical paths use /private/var. The ClassWriter
/// still owns confinement and the actual held-directory write.
pub(super) fn resolved_path(path: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "symbolic checkpoint/evidence paths are not accepted",
                ));
            }
            std::fs::canonicalize(path)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| io::Error::other("path has no parent"))?;
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("path has no filename"))?;
            Ok(resolved_path(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}
