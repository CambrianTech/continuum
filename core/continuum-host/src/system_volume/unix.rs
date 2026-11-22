use std::path::{Path, PathBuf};

pub(super) fn system_root() -> Option<PathBuf> {
    Some(PathBuf::from("/"))
}

pub(super) fn mount_comparison_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}
