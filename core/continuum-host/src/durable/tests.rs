use super::*;

// what this catches: replace has rename(2) semantics on this platform through the arm that also
// makes it durable (target replaced, source consumed, a first write the same call), and an error
// that is not "held by another process" returns at once instead of being slept on. On Windows
// this is the call persona seeds failed on when a directory was opened as a file.
#[test]
fn replace_replaces_on_this_platform_and_a_real_error_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("seed.json.tmp");
    let to = dir.path().join("seed.json");
    std::fs::write(&to, b"old").unwrap();
    std::fs::write(&from, b"new").unwrap();
    replace(&from, &to).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), b"new");
    assert!(!from.exists(), "the source is consumed by the move");

    let fresh = dir.path().join("fresh.json");
    std::fs::write(&from, b"first").unwrap();
    replace(&from, &fresh).unwrap();
    assert_eq!(std::fs::read(&fresh).unwrap(), b"first");

    let started = std::time::Instant::now();
    assert!(replace(&from, &fresh).is_err(), "a missing source is an error");
    assert!(started.elapsed() < IN_USE_FIRST_WAIT, "and it is not retried");
}

// what this catches: an immutable publication refuses an occupied address, even while a reader
// holds it open, and leaves the original bytes; a free address publishes; an injected fence
// failure is reported, never a silent success.
#[test]
fn publish_new_refuses_an_occupied_address_and_a_failed_fence_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let from = dir.path().join("staging");
    let taken = dir.path().join("taken.log");
    std::fs::write(&taken, b"first").unwrap();
    let reader = File::open(&taken).unwrap();
    std::fs::write(&from, b"different").unwrap();
    assert_eq!(publish_new(&from, &taken).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    let mut original = [0; 5];
    crate::positioned::read_exact_at(&reader, &mut original, 0).unwrap();
    assert_eq!(&original, b"first");

    let free = dir.path().join("free.log");
    publish_new(&from, &free).unwrap();
    assert_eq!(std::fs::read(&free).unwrap(), b"different");

    let file = OpenOptions::new().read(true).write(true).open(&free).unwrap();
    fault_injection::with_failed_fence(|| assert!(fence(&file, &free).is_err()));
    fence(&file, &free).unwrap();
}

// what this catches: a staging file outliving its write (dropped unplaced, placed by rename, or
// placed by hard link), a publish over a taken name changing it, and a leftover staging name from
// a crashed writer with this PID refusing the next life's first write (a container's core is PID 1
// every boot; Cormac on #5651).
#[test]
fn staging_never_outlives_its_write_and_never_refuses_a_leftover_name() {
    let base = tempfile::tempdir().expect("tempdir");
    let dir = cap_std::fs::Dir::open_ambient_dir(base.path(), cap_std::ambient_authority()).expect("dir");
    let next = staging::peek_seq();
    for seq in next..next + 64 {
        std::fs::write(base.path().join(format!(".{}-{seq}.staging", std::process::id())), b"crash").expect("plant");
    }
    let planted = std::fs::read_dir(base.path()).expect("dir").count();
    drop(Staging::new(&dir).expect("a leftover name is skipped"));
    assert_eq!(std::fs::read_dir(base.path()).expect("dir").count(), planted, "dropped unplaced: removed");
    let dest = std::path::Path::new("blob");
    let mut first = Staging::new(&dir).expect("staging");
    first.write_all(b"one").expect("write");
    first.publish_new(dest).expect("publish");
    let mut second = Staging::new(&dir).expect("staging");
    second.write_all(b"two").expect("write");
    assert_eq!(second.publish_new(dest).expect_err("taken").kind(), std::io::ErrorKind::AlreadyExists);
    fence_existing(&dir, dest).expect("fence the existing name");
    let mut third = Staging::new(&dir).expect("staging");
    third.write_all(b"three").expect("write");
    third.replace(dest).expect("replace");
    assert_eq!(std::fs::read(base.path().join("blob")).expect("read"), b"three");
    assert_eq!(std::fs::read_dir(base.path()).expect("dir").count(), planted + 1, "only the placed file was added");
}

// what this catches: two holders of one lock at once.
#[test]
fn a_held_lock_refuses_a_second_holder() {
    let base = tempfile::tempdir().expect("tempdir");
    let path = base.path().join(".x.lock");
    let held = lock_path(&path, false).expect("first");
    assert_eq!(lock_path(&path, false).err().map(|e| e.kind()), Some(std::io::ErrorKind::WouldBlock));
    drop(held);
    // Waiting, not try: a child another test is spawning shares the lock's open description
    // between fork and exec (O_CLOEXEC releases it at exec), so "free" is "free once that ends".
    lock_path(&path, true).expect("free again");
}
