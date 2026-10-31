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

// what this catches: a durable directory creation creates the entry and reports an existing one
// as AlreadyExists (a racing creator relies on that kind), on this platform's sync arm.
#[test]
fn create_dir_creates_once_and_an_existing_one_is_already_exists() {
    let dir = tempfile::tempdir().unwrap();
    let made = dir.path().join("objects");
    create_dir(&made).unwrap();
    assert!(made.is_dir());
    assert_eq!(create_dir(&made).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
}
