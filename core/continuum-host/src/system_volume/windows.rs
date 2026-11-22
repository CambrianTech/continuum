use std::path::{Component, Path, PathBuf, Prefix};

pub(super) fn system_root() -> Option<PathBuf> {
    std::env::var_os("SystemRoot").map(PathBuf::from)
}

// canonicalize returns verbatim prefixes on Windows, while mount points use ordinary prefixes
pub(super) fn mount_comparison_path(path: &Path) -> PathBuf {
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    let mut normalized = match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => PathBuf::from(format!("{}:", char::from(letter.to_ascii_uppercase()))),
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let mut root = PathBuf::from(r"\\");
            root.push(server);
            root.push(share);
            root
        }
        _ => return path.to_path_buf(),
    };
    normalized.extend(components);
    normalized
}
