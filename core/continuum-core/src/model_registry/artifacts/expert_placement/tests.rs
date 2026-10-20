    use super::*;
    use crate::capacity::expert_container::fixtures::write_container;
    struct Custody {
        stop: Option<MigrationStage>,
    }
    impl ContainerMigrationCustody for Custody {
        fn revalidate(&self, _: &Path, stage: MigrationStage) -> Result<(), String> {
            if self.stop == Some(stage) {
                Err("owner generation changed".into())
            } else {
                Ok(())
            }
        }
    }

    impl ProducerCustody for Custody {
        fn revalidate(&self, _: &Path) -> Result<(), String> {
            if self.stop == Some(MigrationStage::Copy) {
                Err("producer custody lost".into())
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn online_preparation_preserves_readers_until_explicit_final_exclusion() {
        // Regression: the long copy must neither publish resolver refusal nor
        // obtain an exclusive lease that rejects new readers.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        let custody = Custody { stop: None };
        let reader = ExpertContainer::open(&source).unwrap();
        #[cfg(windows)]
        let prepared = {
            // Kernel custody must reject an already-open writer before any cold
            // publication, then permit readers after the writer releases.
            let writer = OpenOptions::new()
                .write(true)
                .open(source.join("manifest.json"))
                .unwrap();
            assert!(preparation::prepare_windows_in(&source, &cold, &|_| Ok(())).is_err());
            assert!(!cold.exists());
            drop(writer);
            preparation::prepare_windows_in(&source, &cold, &|_| Ok(())).unwrap()
        };
        #[cfg(not(windows))]
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        assert!(read(&source).unwrap().is_none());
        assert!(ExpertContainer::open(&source).is_ok());
        #[cfg(windows)]
        assert!(OpenOptions::new()
            .write(true)
            .open(source.join("manifest.json"))
            .is_err());
        // The maintained legacy-reader contract is not replaced by the trait
        // assertion: an actual cooperating reader still prevents the upgrade.
        assert!(prepared.freeze(&custody).is_err());
        assert!(read(&source).unwrap().is_none());
        assert!(source.join("manifest.json").exists());
        drop(reader);
        // Restart reacquires and re-hashes; a prior cold copy is not a saved
        // live-handle authorization. Failed final custody preserves serving.
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        assert!(prepared
            .freeze(&Custody {
                stop: Some(MigrationStage::Publish)
            })
            .is_err());
        assert!(read(&source).unwrap().is_none());
        let prepared = preparation::prepare_in(&source, &cold, &custody, &|_| Ok(())).unwrap();
        let operation = prepared.operation_digest().unwrap();
        assert_eq!(prepared.source_root().unwrap(), std::fs::canonicalize(&source).unwrap());
        assert!(prepared.freeze(&custody).unwrap() > 0);
        assert_eq!(saved_operation_digest(&source).unwrap().as_deref(), Some(operation.as_str()));
        assert!(!source.exists());
        assert!(resolve_for_serving(&source).is_err());
    }

    #[test]
    fn actual_lazy_reader_excludes_freeze_until_its_lifetime_ends() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        let reader = ExpertContainer::open(&source).unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(read(&source).unwrap().is_none());
        assert!(!cold.exists());
        drop(reader);
        let abort = hold_unstarted(&source).unwrap();
        assert!(ExpertContainer::open(&source).is_ok(), "abort custody does not block unchanged readers");
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err(), "abort restoration excludes placement publication");
        drop(abort);
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap() > 0);
        assert!(hold_unstarted(&source).is_err(), "Frozen placement requires reconciliation, not abort");
        let saved = read(&source).unwrap().unwrap();
        let digest = operation_digest(&saved).unwrap();
        let restored = hold_restoration(&source, &digest).unwrap();
        assert!(!source.exists(), "restart verification never recreates the source");
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err(), "restart verification retains transaction custody");
        drop(restored);
        assert!(hold_restoration(&source, &"f".repeat(64)).is_err());
        let mut copying = saved.clone();
        copying.state = State::Copying;
        publish(&source, &copying).unwrap();
        assert!(hold_restoration(&source, &digest).is_err(), "an unfinished placement cannot restart legacy readers");
        publish(&source, &saved).unwrap();
        let destination = saved.destination.decode().unwrap();
        let member = destination.join(entry_name(&saved.files[0]).unwrap());
        std::fs::write(&member, b"changed").unwrap();
        assert!(hold_restoration(&source, &digest).is_err(), "lost in-memory hashes must not authorize changed cold bytes");
        std::fs::write(receipt_path(&source).unwrap(), b"corrupt").unwrap();
        assert!(hold_unstarted(&source).is_err(), "unknown placement is never an unstarted operation");
    }

    #[tokio::test]
    #[ignore = "existing Unix/Windows CI runs this with the compiled native custody probe"]
    async fn fs2_and_native_container_locks_interoperate_across_processes() {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let native = std::env::var_os("CONTINUUM_NATIVE_CONTAINER_LEASE_TEST")
            .expect("native custody fixture must be compiled by the owning CI step");
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("container");
        std::fs::create_dir(&root).unwrap();
        let probe = |mode: &'static str| {
            let native = native.clone();
            let root = root.clone();
            async move {
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    tokio::process::Command::new(native)
                        .args(["--container-lease"])
                        .arg(root)
                        .arg(mode)
                        .kill_on_drop(true)
                        .status(),
                )
                .await
                .unwrap()
                .unwrap()
                .code()
                .unwrap()
            }
        };
        let (_, shared) = reader_lease(&root).unwrap();
        assert_eq!(probe("reader").await, 0);
        assert_eq!(probe("writer").await, 3);
        drop(shared);
        let writer = exclusive(&sibling(&root, ".container-placement.lock").unwrap()).unwrap();
        assert_eq!(probe("reader").await, 3);
        assert_eq!(probe("writer").await, 3);
        drop(writer);
        assert_eq!(probe("writer").await, 0);
        for mode in ["reader", "writer"] {
            let mut child = tokio::process::Command::new(&native)
                .arg("--container-lease")
                .arg(&root)
                .arg(mode)
                .arg("hold")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                output.read_line(&mut ready),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(ready.trim(), "held");
            assert!(exclusive(&sibling(&root, ".container-placement.lock").unwrap()).is_err());
            assert_eq!(reader_lease(&root).is_ok(), mode == "reader");
            child.stdin.take().unwrap().write_all(b"\n").await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .success()
            );
            assert!(exclusive(&sibling(&root, ".container-placement.lock").unwrap()).is_ok());
        }
        #[cfg(windows)]
        {
            let member = root.join("mapped-member");
            std::fs::write(&member, b"fixture bytes").unwrap();
            let mut child = tokio::process::Command::new(&native)
                .arg("--container-write-map")
                .arg(&member)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut output = BufReader::new(child.stdout.take().unwrap());
            let mut ready = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                output.read_line(&mut ready),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                ready.trim(),
                "held",
                "native mapping must exist after its file handle closes"
            );
            assert!(
                open_source(&member).is_err(),
                "pre-existing writable mapping must exclude immutable preparation"
            );
            child.stdin.take().unwrap().write_all(b"\n").await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
                    .await
                    .unwrap()
                    .unwrap()
                    .success()
            );
            assert!(
                open_source(&member).is_ok(),
                "mapping release restores preparation admission"
            );
        }
    }

    #[test]
    fn interrupted_unpublished_member_is_replaced_only_after_readmission() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        assert_eq!(saved.state, State::Copying);
        let destination = saved.destination.decode().unwrap();
        let staging = sibling(&destination, ".copying").unwrap();
        std::fs::rename(&destination, &staging).unwrap();
        let member = staging.join("experts-L0.bin");
        std::fs::write(&member, b"partial").unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Err(
            "capacity".into()
        ))
        .is_err());
        assert_eq!(std::fs::read(&member).unwrap(), b"partial");
        assert!(source.exists());
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        assert!(!staging.exists());
        assert_eq!(
            std::fs::read(destination.join("experts-L0.bin")).unwrap(),
            std::fs::read(source.join("experts-L0.bin")).unwrap()
        );
        assert_eq!(read(&source).unwrap().unwrap().state, State::Frozen);
    }

    #[test]
    fn missing_published_member_refuses_without_repairing_shared_destination() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        let destination = saved.destination.decode().unwrap();
        let missing = destination.join("experts-L0.bin");
        std::fs::remove_file(&missing).unwrap();
        let manifest = std::fs::read(destination.join("manifest.json")).unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(!missing.exists());
        assert!(source.join("experts-L0.bin").exists());
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            manifest
        );
        assert_eq!(read(&source).unwrap().unwrap().state, State::Copying);
    }

    #[test]
    fn freeze_preserves_original_bytes_and_resumes_only_after_verified_publication() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 2, 2);
        let original = std::fs::read(source.join("manifest.json")).unwrap();
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        assert!(source.join("experts-L0.bin").exists());
        assert_eq!(read(&source).unwrap().unwrap().state, State::Frozen);
        assert!(resolve_for_serving(&source).unwrap_err().contains("frozen"));
        let destination = read(&source)
            .unwrap()
            .unwrap()
            .destination
            .decode()
            .unwrap();
        assert_eq!(
            std::fs::read(destination.join("manifest.json")).unwrap(),
            original
        );
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap() > 0);
        assert!(!source.exists());
        assert!(resolve_for_serving(&source).is_err());
        assert_eq!(
            freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).unwrap(),
            0
        );
    }

    #[test]
    fn capacity_or_custody_refusal_keeps_source_and_corrupt_copy_never_retires_it() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Err(
            "capacity".into()
        ))
        .is_err());
        assert!(!cold.exists());
        assert!(read(&source).unwrap().is_none());
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Publish)
            },
            &|_| Ok(())
        )
        .is_err());
        let saved = read(&source).unwrap().unwrap();
        assert_eq!(saved.state, State::Copying);
        std::fs::write(
            saved.destination.decode().unwrap().join("experts-L0.bin"),
            b"changed",
        )
        .unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert!(source.join("experts-L0.bin").exists());
    }

    #[test]
    fn custody_changes_and_foreign_source_entries_never_authorize_retirement() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("hot");
        let cold = temp.path().join("cold");
        std::fs::create_dir(&source).unwrap();
        write_container(&source, 1, 2);
        for stage in [MigrationStage::Observe, MigrationStage::Copy] {
            assert!(
                freeze_in(&source, &cold, &Custody { stop: Some(stage) }, &|_| Ok(())).is_err()
            );
            assert!(read(&source).unwrap().is_none());
            assert!(source.join("experts-L0.bin").exists());
        }
        assert!(freeze_in(
            &source,
            &cold,
            &Custody {
                stop: Some(MigrationStage::Retire)
            },
            &|_| Ok(())
        )
        .is_err());
        std::fs::write(source.join("later-owner-data"), b"preserve").unwrap();
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
        assert_eq!(
            std::fs::read(source.join("later-owner-data")).unwrap(),
            b"preserve"
        );
        assert!(source.join("experts-L0.bin").exists());
        std::fs::write(receipt_path(&source).unwrap(), b"broken receipt").unwrap();
        assert!(resolve_for_serving(&source).is_err());
        assert!(freeze_in(&source, &cold, &Custody { stop: None }, &|_| Ok(())).is_err());
    }
