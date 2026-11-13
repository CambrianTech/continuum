use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;

use super::FileIdentity;

pub(super) fn identity(file: &File) -> io::Result<FileIdentity> {
    let meta = file.metadata()?;
    Ok(FileIdentity { volume: meta.dev(), index: meta.ino() })
}
