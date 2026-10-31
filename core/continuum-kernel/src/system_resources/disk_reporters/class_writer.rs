//! The one way the substrate puts a file on disk: through the class that owns its directory
//! (ONE-BUDGET row 5, "class handles"). A writer names its class and a path RELATIVE to it; the
//! class owns the root (resolved from the continuum home at boot), measures it, and decides its
//! eviction. A writer that picks its own absolute path is `writes_outside_a_class`, the shape that
//! filled the M5 (460 GB, 2026-07-13) while every component was green.
//!
//! Durability, staging and locking are `continuum_host::durable`'s (the one owner of placing a
//! file); this module only confines the path to the class.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use continuum_host::durable;

use super::TrackedDir;

/// A writer's hold on one tracked class. Obtained from the class ([`TrackedDir::writer`]), never
/// built from a path, so every file it makes is under a measured root with an eviction decision.
#[derive(Clone)]
pub struct ClassWriter {
    dir: Arc<TrackedDir>,
    /// The owner's directory inside the class (`owned_by`); empty for the class root.
    scope: PathBuf,
}

impl TrackedDir {
    pub fn writer(self: &Arc<Self>) -> ClassWriter {
        ClassWriter { dir: Arc::clone(self), scope: PathBuf::new() }
    }
}

impl ClassWriter {
    /// The writer for a registered class, from the boot registry, resolved when the caller needs
    /// it (a lazily built global resolves at its first write, not at construction). Refused before
    /// boot installs the registry or for a class with no row: there is no default directory to
    /// guess. A test injects `TrackedDir::new(name, temp).writer()` instead.
    pub fn of(class: &str) -> io::Result<Self> {
        super::tracked_dir(class).map(|dir| dir.writer()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("disk class {class} is not registered: boot has not installed the registry, or the class has no row in standard_tracked_dirs"),
            )
        })
    }

    pub fn class(&self) -> &'static str {
        self.dir.class()
    }

    /// The same class, scoped to one owner's directory (`<class root>/<owner>/`): a persona's,
    /// a citizen's. The one place an owner's directory name is formed; a caller names only its
    /// file.
    pub fn owned_by(&self, owner: uuid::Uuid) -> Self {
        ClassWriter { dir: Arc::clone(&self.dir), scope: self.scope.join(owner.to_string()) }
    }

    /// The owners with a directory in this scope (the mirror of `owned_by`), for a reader that
    /// rehydrates every owner at boot. An entry whose name is not an owner id is not an owner; a
    /// class nothing has written yet has none.
    pub fn owners(&self) -> io::Result<Vec<uuid::Uuid>> {
        let entries = match std::fs::read_dir(self.dir.path().join(&self.scope)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut owners = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(owner) = entry.file_name().to_str().and_then(|name| uuid::Uuid::parse_str(name).ok()) {
                    owners.push(owner);
                }
            }
        }
        owners.sort();
        Ok(owners)
    }

    /// Where `relative` lives in this class, by name: refused unless it is plain names (no absolute
    /// path, `..`, `.`, empty). This is the address only; what is created or opened there goes
    /// through `continuum_host::confine`, which also refuses a redirect on the way.
    pub fn path(&self, relative: &Path) -> io::Result<PathBuf> {
        let mut parts = relative.components().peekable();
        if parts.peek().is_none() || !parts.all(|part| matches!(part, Component::Normal(_))) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{} is not a path inside disk class {}", relative.display(), self.class()),
            ));
        }
        Ok(self.dir.path().join(&self.scope).join(relative))
    }

    /// The file's path with its directories created under the class root, every one a real
    /// directory (no symlink or junction redirects the write out of the measured root).
    fn prepared(&self, relative: &Path) -> io::Result<PathBuf> {
        let path = self.path(relative)?;
        let scoped = self.scope.join(relative);
        let dirs = scoped.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{} names no file", relative.display())))?;
        continuum_host::confine::dirs_within(self.dir.path(), dirs)?;
        Ok(path)
    }

    /// The content staged beside its destination, ready to place.
    fn staged(path: &Path, bytes: &[u8]) -> io::Result<durable::Staging> {
        let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, format!("{} has no directory", path.display())))?;
        let mut staging = durable::Staging::new_in(dir)?;
        staging.file().write_all(bytes)?;
        Ok(staging)
    }

    /// Replace a file whole: a reader sees the old bytes or the new ones, never half, and the
    /// replacement survives power loss.
    pub fn replace(&self, relative: &Path, bytes: &[u8]) -> io::Result<()> {
        let path = self.prepared(relative)?;
        Self::staged(&path, bytes)?.replace(&path)
    }

    /// Publish an immutable file once: `AlreadyExists` if the name is taken, never an overwrite.
    /// A caller that may be replaying compares what is there.
    pub fn publish_new(&self, relative: &Path, bytes: &[u8]) -> io::Result<()> {
        let path = self.prepared(relative)?;
        Self::staged(&path, bytes)?.publish_new(&path)
    }

    /// Open a file to append to (a log, a capture stream), creating it on first use.
    pub fn append(&self, relative: &Path) -> io::Result<File> {
        self.path(relative)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        continuum_host::confine::open_within(self.dir.path(), &self.scope.join(relative), &options)
    }

    /// Lock `relative` exclusively (a lock file in the class), waiting for a holder when `wait`,
    /// else refusing with `WouldBlock`. Every party to one file's read/write takes the same lock.
    pub fn lock(&self, relative: &Path, wait: bool) -> io::Result<durable::Lock> {
        durable::lock(&self.prepared(relative)?, wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class() -> (tempfile::TempDir, ClassWriter) {
        let root = tempfile::tempdir().expect("tempdir");
        let writer = TrackedDir::new("test-class", root.path().join("class")).writer();
        (root, writer)
    }

    // what this catches: a writer escaping its class root (an absolute path, `..`), which is how a
    // write lands in a directory nobody measures or evicts.
    #[test]
    fn a_path_outside_the_class_is_refused() {
        let (_root, writer) = class();
        for escape in ["", "/etc/passwd", "../sibling", "a/../../b", "./a"] {
            assert!(writer.path(Path::new(escape)).is_err(), "{escape:?} must be refused");
        }
        assert!(writer.path(Path::new("peers/a/emission.json")).is_ok());
    }

    // what this catches: a replace that leaves its staging file or half a file, and a publish that
    // overwrites what is already published.
    #[test]
    fn replace_is_whole_and_publish_never_overwrites() {
        let (_root, writer) = class();
        let name = Path::new("p/state.json");
        writer.replace(name, b"one").expect("first");
        writer.replace(name, b"two").expect("second");
        let path = writer.path(name).expect("path");
        assert_eq!(std::fs::read(&path).expect("read"), b"two");
        let left: Vec<_> = std::fs::read_dir(path.parent().expect("parent")).expect("dir").collect();
        assert_eq!(left.len(), 1, "no staging file left beside it");
        assert_eq!(writer.publish_new(name, b"three").expect_err("taken").kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).expect("read"), b"two", "a refused publish changed nothing");
    }

    // what this catches: an owner's file landing outside its owner's directory, or a caller
    // forming the directory name by hand differently from the next one.
    #[test]
    fn an_owner_writes_inside_its_own_directory() {
        let (_root, writer) = class();
        let owner = uuid::Uuid::from_u128(7);
        let mine = writer.owned_by(owner);
        mine.replace(Path::new("emission.json"), b"{}").expect("write");
        assert_eq!(
            mine.path(Path::new("emission.json")).expect("path"),
            writer.path(Path::new(&owner.to_string())).expect("owner dir").join("emission.json")
        );
        assert!(mine.path(Path::new("../other")).is_err(), "an owner cannot reach a sibling");
        // The mirror: the owners read back are exactly those written, and a stray file is not one.
        writer.owned_by(uuid::Uuid::from_u128(3)).replace(Path::new("demand.json"), b"{}").expect("second owner");
        writer.replace(Path::new("README"), b"not an owner").expect("stray");
        assert_eq!(writer.owners().expect("owners"), vec![uuid::Uuid::from_u128(3), owner]);
        assert_eq!(class().1.owners().expect("empty class"), Vec::<uuid::Uuid>::new());
    }

    // what this catches: a symlink inside the class sending a write outside the measured root
    // (Codex on #5651): through a linked directory and through a linked final file, for replace,
    // append and lock alike; nothing lands outside.
    #[cfg(unix)]
    #[test]
    fn a_redirect_inside_the_class_writes_nothing_outside_it() {
        let (root, writer) = class();
        let outside = tempfile::tempdir().expect("outside");
        writer.replace(Path::new("seed"), b"x").expect("the class root exists");
        std::os::unix::fs::symlink(outside.path(), root.path().join("class/away")).expect("dir link");
        assert!(writer.replace(Path::new("away/state.json"), b"leak").is_err());
        assert!(writer.append(Path::new("away/log")).is_err());
        assert!(writer.lock(Path::new("away/.lock"), false).is_err());
        std::fs::write(outside.path().join("target"), b"").expect("target");
        std::os::unix::fs::symlink(outside.path().join("target"), root.path().join("class/file-link")).expect("file link");
        assert!(writer.append(Path::new("file-link")).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).expect("outside").count(), 1, "nothing written outside the class");
    }

    // what this catches: a writer that guesses a directory when its class is not registered.
    #[test]
    fn an_unregistered_class_has_no_writer() {
        assert_eq!(ClassWriter::of("no-such-class").err().map(|e| e.kind()), Some(io::ErrorKind::NotFound));
    }

    // what this catches: two holders of one lock at once.
    #[test]
    fn a_held_lock_refuses_a_second_holder() {
        let (_root, writer) = class();
        let held = writer.lock(Path::new("p/.volatile.lock"), false).expect("first");
        assert!(writer.lock(Path::new("p/.volatile.lock"), false).is_err());
        drop(held);
        writer.lock(Path::new("p/.volatile.lock"), false).expect("free again");
    }
}
