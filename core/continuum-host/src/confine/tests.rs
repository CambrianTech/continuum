use super::*;

// what this catches: a write through a link below the root landing outside it (a linked
// directory, a linked final file), and a held directory swapped for a link mid-write sending the
// bytes out (Codex on #5651); plain paths and a not-yet-created root still work.
#[test]
fn nothing_below_a_held_root_reaches_outside_it() {
    use std::io::Write;
    let base = tempfile::tempdir().expect("base");
    let outside = tempfile::tempdir().expect("outside");
    let root_path = base.path().join("not-yet-created");
    let root = open_root(&root_path).expect("root created and held");
    let a = subdir(&root, Path::new("a/b")).expect("plain path");
    a.create("log").expect("create").write_all(b"x").expect("write");
    assert!(root_path.join("a/b/log").is_file());

    link(outside.path(), &root_path.join("away"));
    assert!(subdir(&root, Path::new("away/x")).is_err(), "a link out of the root is refused");
    std::fs::write(outside.path().join("target"), b"").expect("target");
    link(&outside.path().join("target"), &root_path.join("a/file-link"));
    assert!(root.open_with("a/file-link", cap_std::fs::OpenOptions::new().append(true)).is_err(), "a linked final file is refused");

    // The swap: hold a/b, move it away and put a link to outside at its old name, then write
    // through the held handle. POSIX lets the move happen and the bytes follow the held
    // directory wherever it now is; Windows refuses the move outright, because cap-std holds
    // every directory without FILE_SHARE_DELETE (cap-primitives 4.0.3 windows/fs/dir_utils.rs).
    // Either way nothing lands outside.
    match std::fs::rename(root_path.join("a/b"), root_path.join("a/moved")) {
        Ok(()) => {
            link(outside.path(), &root_path.join("a/b"));
            a.create("after-swap").expect("create through the held handle");
            assert!(root_path.join("a/moved/after-swap").is_file(), "the write followed the held directory");
        }
        Err(refused) => {
            assert!(cfg!(windows), "only Windows refuses moving a held directory: {refused}");
            a.create("after-swap").expect("create through the held handle");
            assert!(root_path.join("a/b/after-swap").is_file());
        }
    }
    assert_eq!(std::fs::read_dir(outside.path()).expect("outside").count(), 1, "nothing written outside the root");
    assert!(plain(Path::new("../x")).is_err() && plain(Path::new("/x")).is_err() && plain(Path::new("")).is_err());
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).expect("symlink");
}

#[cfg(windows)]
fn link(target: &Path, at: &Path) {
    // A junction needs no privilege, so the Windows redirect case runs on every runner.
    if target.is_dir() {
        let status = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(at).arg(target).status().expect("mklink");
        assert!(status.success(), "junction");
    } else {
        std::os::windows::fs::symlink_file(target, at).expect("file symlink (developer mode or privilege)");
    }
}
