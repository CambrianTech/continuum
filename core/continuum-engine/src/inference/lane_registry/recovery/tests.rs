use super::*;

fn record() -> LaneRecord {
    LaneRecord {
        pid: 42,
        port: 58057,
        role: LaneRole::Live,
        model: "retained-launch".into(),
        context_window: 32768,
        lanes: 3,
        page_dir: None,
        engine_bin: None,
        started_s: 100,
        placement: None,
        weights: None,
    }
}

fn alive(pid: u32) -> ProcessProbe {
    if pid == 42 {
        ProcessProbe::Present(100)
    } else {
        ProcessProbe::Absent
    }
}

#[test]
fn missing_and_dead_registration_recover_without_erasing_other_records() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let dir = temp.path().join("lanes");
    let pid = temp.path().join("lane.pid");
    std::fs::write(&pid, "396")?;
    let other = LaneRecord {
        pid: 396,
        ..record()
    };
    record_in(&dir, &other)?;
    let before = std::fs::read(record_path(&dir, 396))?;
    restore_in(&dir, &pid, &record(), &alive)?;
    assert_eq!(std::fs::read_to_string(&pid)?, "42");
    assert_eq!(std::fs::read(record_path(&dir, 396))?, before);
    assert_eq!(
        serde_json::from_slice::<LaneRecord>(&std::fs::read(record_path(&dir, 42))?)?,
        record()
    );
    restore_in(&dir, &pid, &record(), &alive)?;
    Ok(())
}

#[test]
fn a_live_or_unknown_competing_pid_is_not_overwritten() -> io::Result<()> {
    for other in [ProcessProbe::Present(900), ProcessProbe::Unreadable] {
        let temp = tempfile::tempdir()?;
        let pid = temp.path().join("lane.pid");
        let dir = temp.path().join("lanes");
        std::fs::write(&pid, "99")?;
        assert!(restore_in(&dir, &pid, &record(), &|p| if p == 42 {
            alive(p)
        } else {
            other
        })
        .is_err());
        assert_eq!(std::fs::read_to_string(&pid)?, "99");
        assert!(!dir.exists());
    }
    Ok(())
}

#[test]
fn a_missing_pointer_does_not_erase_a_competing_live_claim() -> io::Result<()> {
    for other in [ProcessProbe::Present(100), ProcessProbe::Unreadable] {
        let temp = tempfile::tempdir()?;
        let pid = temp.path().join("lane.pid");
        let dir = temp.path().join("lanes");
        record_in(
            &dir,
            &LaneRecord {
                pid: 99,
                ..record()
            },
        )?;
        let before = std::fs::read(record_path(&dir, 99))?;
        assert!(restore_in(&dir, &pid, &record(), &|p| if p == 42 {
            alive(p)
        } else {
            other
        })
        .is_err());
        assert!(!pid.exists());
        assert!(!record_path(&dir, 42).exists());
        assert_eq!(std::fs::read(record_path(&dir, 99))?, before);
    }
    Ok(())
}

#[test]
fn a_reused_pid_record_and_unknown_retained_start_refuse() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let dir = temp.path().join("lanes");
    let pid = temp.path().join("lane.pid");
    let replacement = LaneRecord {
        started_s: 101,
        ..record()
    };
    record_in(&dir, &replacement)?;
    let before = std::fs::read(record_path(&dir, 42))?;
    assert!(restore_in(&dir, &pid, &record(), &alive).is_err());
    assert_eq!(std::fs::read(record_path(&dir, 42))?, before);
    assert!(!pid.exists());
    assert!(restore_in(
        &dir,
        &pid,
        &LaneRecord {
            started_s: 0,
            ..record()
        },
        &alive
    )
    .is_err());
    Ok(())
}

#[test]
fn death_between_record_and_pid_publication_never_publishes_the_pid() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let dir = temp.path().join("lanes");
    let pid = temp.path().join("lane.pid");
    let calls = std::cell::Cell::new(0);
    assert!(restore_in(&dir, &pid, &record(), &|p| {
        calls.set(calls.get() + 1);
        if calls.get() >= 3 {
            ProcessProbe::Absent
        } else {
            alive(p)
        }
    })
    .is_err());
    assert!(!pid.exists());
    assert!(record_path(&dir, 42).exists());
    // A later attempt needs fresh positive identity evidence; it can complete a partial write.
    restore_in(&dir, &pid, &record(), &alive)?;
    assert_eq!(std::fs::read_to_string(&pid)?, "42");
    Ok(())
}
