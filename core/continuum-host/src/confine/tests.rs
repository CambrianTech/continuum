use super::*;

// what this catches: a write through a redirect inside the root landing outside it: a linked
// directory on the way, and a linked final file (Codex on #5651), while plain paths still work.
#[test]
fn a_redirect_below_the_root_is_refused_and_plain_paths_work() {
    let base = tempfile::tempdir().expect("base");
    let root = base.path().join("not-yet-created");
    let root = root.as_path();
    let outside = tempfile::tempdir().expect("outside");
    let mut append = OpenOptions::new();
    append.create(true).append(true);
    open_within(root, Path::new("a/b/log"), &append).expect("plain path");
    assert!(root.join("a/b/log").is_file());

    link(outside.path(), &root.join("away"));
    assert!(dirs_within(root, Path::new("away/x")).is_err(), "a linked directory is refused");
    assert!(open_within(root, Path::new("away/result"), &append).is_err(), "a write through it is refused");
    std::fs::write(outside.path().join("target"), b"").expect("target");
    link(&outside.path().join("target"), &root.join("a/file-link"));
    assert!(open_within(root, Path::new("a/file-link"), &append).is_err(), "a linked final file is refused");
    assert_eq!(std::fs::read_dir(outside.path()).expect("outside").count(), 1, "nothing was written outside the root");
    assert!(names(Path::new("../x")).is_err() && names(Path::new("/x")).is_err());
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
