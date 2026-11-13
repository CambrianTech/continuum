use super::*;

pub(super) fn mutation_pins(lease: &File, journal: &File) -> Result<(File, File), String> {
    Ok((
        lease.try_clone().map_err(|e| e.to_string())?,
        journal.try_clone().map_err(|e| e.to_string())?,
    ))
}

/// Private acquisition fields prevent a caller substituting an arbitrary File.
/// Drop releases only the install lease, NEVER silently restores async tasks.
/// On every failure/crash the persisted snapshot is the recovery authority.
pub struct SchedulerCustody {
    snapshot: RecoverySnapshot,
    journal: PathBuf,
    journal_pin: File,
    _lease: File,
    _launch_gate: Option<File>,
    _bootstrap_pins: Vec<File>,
}

impl SchedulerCustody {
    pub async fn disable(root: &Path) -> Result<Self, String> {
        Self::disable_for(root, None).await
    }

    async fn disable_for(root: &Path, legacy: Option<LegacyPurpose>) -> Result<Self, String> {
        let lease = take_install_lease(root)?;
        require_idle_claim(root)?;
        let launch_gate = launch_gate(root, true)?;
        let snapshot = RecoverySnapshot {
            version: 1,
            tasks: read_tasks().await?,
            legacy,
        };
        snapshot.validate()?;
        let bootstrap_pins = pin_verified_bootstrap(root, snapshot.legacy.is_none()).await?;
        snapshot.check_identity(&read_tasks().await?)?;
        if let Some(purpose) = &snapshot.legacy {
            check_legacy_sources(root, purpose, true).await?;
        }
        let journal = root.join(JOURNAL);
        crate::installed_release::unredirected(root)?;
        let bytes = serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?; // Durable maintenance recovery file boundary.
        if bytes.len() as u64 > crate::installed_release::MAX_RECEIPT_BYTES {
            return Err("maintenance snapshot exceeds existing installed receipt bound".into());
        }
        // Never replace earlier evidence. A partial write also blocks mutation
        // and must be diagnosed, rather than interpreted as no maintenance.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&journal)
            .map_err(|e| format!("cannot create maintenance recovery evidence: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("cannot persist maintenance intent: {e}"))?;
        drop(file);
        crate::installed_release::unredirected(&journal)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&journal)
            .map_err(|e| format!("cannot pin durable maintenance evidence: {e}"))?;
        // The mutation child hashes these pinned bytes against the original
        // snapshot before its first task mutation.
        let custody = Self {
            snapshot,
            journal,
            journal_pin: file,
            _lease: lease,
            _launch_gate: Some(launch_gate),
            _bootstrap_pins: bootstrap_pins,
        };
        let disabled = set_enabled(
            &custody.snapshot,
            false,
            false,
            &custody._lease,
            &custody.journal,
            &custody.journal_pin,
            &custody._bootstrap_pins,
        )
        .await;
        let legacy = custody.snapshot.legacy.is_some();
        settle_disable(custody, legacy, disabled, |custody| custody.restore()).await
    }

    /// Recheck actual scheduler exclusion before the caller's separate drain.
    /// A successful result is NOT proof that any running reader has stopped.
    pub async fn revalidate(&self) -> Result<(), String> {
        let observed = read_tasks().await?;
        self.snapshot.check_disabled(&observed)
    }

    /// Caller must reconcile any placement/retirement before re-enabling
    /// launchers. Scheduler custody alone cannot judge artifact readiness.
    pub async fn restore(self) -> Result<(), String> {
        if self.snapshot.legacy.is_some() {
            return Err("legacy recovery requires the matching placement operation".into());
        }
        self.restore_inner().await
    }

    async fn restore_inner(self) -> Result<(), String> {
        set_enabled(
            &self.snapshot,
            true,
            false,
            &self._lease,
            &self.journal,
            &self.journal_pin,
            &self._bootstrap_pins,
        )
        .await?;
        // Exact original XML/security/enabled state has been observed. Failure
        // to remove evidence leaves an idempotent explicit recovery operation.
        drop(self.journal_pin);
        std::fs::remove_file(&self.journal)
            .map_err(|e| format!("tasks restored; cannot retire recovery evidence: {e}"))
    }

    async fn begin_restoration(mut self) -> Result<LegacyRestoration, String> {
        self.snapshot
            .legacy
            .as_mut()
            .ok_or("legacy restoration binding absent")?
            .phase = LegacyPhase::Restoring;
        self.snapshot.validate()?;
        // Publish the same bounded journal atomically before enabling. A crash
        // before enabled-state restoration remains restartable, never absent evidence.
        self.journal_pin = publish_restoring(&self.journal, self.journal_pin, &self.snapshot)?;
        self.enable_for_verification().await
    }

    async fn enable_for_verification(mut self) -> Result<LegacyRestoration, String> {
        if self.snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
            return Err("restoration phase not committed".into());
        }
        set_enabled(
            &self.snapshot,
            true,
            true,
            &self._lease,
            &self.journal,
            &self.journal_pin,
            &self._bootstrap_pins,
        )
        .await?;
        check_restoring_startup(&self.snapshot, &read_tasks().await?)?;
        // Keep install/evidence custody through acceptance, but permit the
        // intended registered startup after exact original states are restored.
        drop(self._launch_gate.take());
        Ok(LegacyRestoration { custody: self })
    }

    /// The integration owner must first reconcile any interrupted placement
    /// and establish its separate legacy/manual launcher exclusion. This
    /// method restores scheduler registration only; it cannot certify that
    /// re-enabling old launchers is safe for a partially retired artifact.
    pub async fn recover(root: &Path) -> Result<(), String> {
        Self::load_recovery(root, None).await?.restore().await
    }

    async fn load_recovery(root: &Path, operation: Option<&str>) -> Result<Self, String> {
        let lease = take_install_lease(root)?;
        let launch_gate = launch_gate(root, true)?;
        let journal = root.join(JOURNAL);
        let (file, snapshot) = read_snapshot(root)?;
        snapshot.check_operation(operation)?;
        let bootstrap_pins = pin_verified_bootstrap(root, snapshot.legacy.is_none()).await?;
        snapshot.check_identity(&read_tasks().await?)?;
        Ok(Self {
            snapshot,
            journal,
            journal_pin: file,
            _lease: lease,
            _launch_gate: Some(launch_gate),
            _bootstrap_pins: bootstrap_pins,
        })
    }
}

/// Scheduler exclusion only. The caller must perform its existing durable
/// full drain and confirm owned generations exited before excluding images.
/// This cannot exclude arbitrary copied binaries or unregistered legacy jobs.
pub struct LegacySchedulerWindow {
    custody: SchedulerCustody,
    root: PathBuf,
}

pub struct LegacyImageCustody {
    window: LegacySchedulerWindow,
    _releases: Vec<crate::installed_release::ValidatedRelease>,
    _bootstrap: Vec<File>,
    files: crate::installed_release::ExclusiveFiles,
}

impl LegacySchedulerWindow {
    pub async fn begin(
        root: &Path,
        payload: &Path,
        operation_digest: &str,
        source: &Path,
        serving_expectation: serde_json::Value,
    ) -> Result<Self, String> {
        require_idle_claim(root)?;
        let core =
            super::super::super::super::task_report(super::super::super::super::CORE_TASK).await?;
        let envelope = crate::installed_release::envelope(&core.description)?
            .ok_or("protected bootstrap absent")?;
        let active = crate::installed_release::read_receipt(&root.join("install-active.json"))?;
        let previous = previous_receipt(root)?;
        let sid = super::super::super::super::caller_sid().await?;
        // Validate the normal sealed authority before capturing it, without
        // trying to exclude still-running images before the owner's drain.
        for text in std::iter::once(&active).chain(previous.iter()) {
            crate::installed_release::validate_receipt(text, root, payload, &sid)?;
        }
        let purpose = LegacyPurpose {
            operation_digest: operation_digest.to_owned(),
            source: canonical_source(source)?,
            serving_expectation,
            phase: LegacyPhase::Excluded,
            payload: payload
                .to_str()
                .ok_or("payload path is not UTF-8")?
                .to_owned(),
            bootstrap_manifest: crate::installed_release::read_receipt(
                &Path::new(&envelope.bootstrap)
                    .parent()
                    .ok_or("bootstrap parent absent")?
                    .join("bootstrap-hashes.json"),
            )?,
            bootstrap: envelope.bootstrap,
            active,
            previous,
        };
        refuse_own_image(root, &purpose).await?;
        let custody = SchedulerCustody::disable_for(root, Some(purpose)).await?;
        Ok(Self {
            custody,
            root: root.to_owned(),
        })
    }

    pub async fn begin_recovery(
        root: &Path,
        payload: &Path,
        operation_digest: &str,
        source: &Path,
    ) -> Result<Self, String> {
        require_idle_claim(root)?;
        let custody = SchedulerCustody::load_recovery(root, Some(operation_digest)).await?;
        let purpose = custody
            .snapshot
            .legacy
            .as_ref()
            .ok_or("not a legacy recovery journal")?;
        if purpose.source != canonical_source(source)? {
            return Err("legacy recovery source differs".into());
        }
        if purpose.phase != LegacyPhase::Excluded {
            return Err("restoration already pending; resume verification without draining".into());
        }
        refuse_own_image(root, purpose).await?;
        if Path::new(&purpose.payload) != payload {
            return Err("legacy recovery payload differs".into());
        }
        check_legacy_sources(root, purpose, true).await?;
        // An interrupted disable may have stopped after its first task.
        set_enabled(
            &custody.snapshot,
            false,
            false,
            &custody._lease,
            &custody.journal,
            &custody.journal_pin,
            &custody._bootstrap_pins,
        )
        .await?;
        Ok(Self {
            custody,
            root: root.to_owned(),
        })
    }

    fn recovery_purpose(root: &Path, source: &Path) -> Result<LegacyPurpose, String> {
        let text = crate::installed_release::read_receipt(&root.join(JOURNAL))?;
        let snapshot: RecoverySnapshot = serde_json::from_str(&text)
            .map_err(|e| format!("invalid maintenance evidence: {e}"))?;
        snapshot.validate()?;
        let purpose = snapshot.legacy.ok_or("not a legacy recovery journal")?;
        if purpose.source != canonical_source(source)? {
            return Err("legacy recovery source differs".into());
        }
        Ok(purpose)
    }

    /// Read-only discovery; begin_recovery checks the same binding again under custody.
    pub fn recovery_binding(root: &Path, source: &Path) -> Result<String, String> {
        Ok(Self::recovery_purpose(root, source)?.operation_digest)
    }

    /// Caller validates its typed projection; compare against the pinned window
    /// value before acting, since this discovery read does not retain custody.
    pub fn restoration_pending(root: &Path, source: &Path) -> Result<bool, String> {
        Ok(Self::recovery_purpose(root, source)?.phase == LegacyPhase::Restoring)
    }

    pub fn recovery_expectation(root: &Path, source: &Path) -> Result<serde_json::Value, String> {
        Ok(Self::recovery_purpose(root, source)?.serving_expectation)
    }

    pub fn serving_expectation(&self) -> &serde_json::Value {
        &self
            .custody
            .snapshot
            .legacy
            .as_ref()
            .expect("legacy window binding")
            .serving_expectation // Private constructors require a validated legacy-purpose journal.
    }

    pub fn operation_digest(&self) -> &str {
        &self
            .custody
            .snapshot
            .legacy
            .as_ref()
            .expect("legacy window binding")
            .operation_digest // Private constructors require a validated legacy-purpose journal.
    }

    pub async fn revalidate(&self) -> Result<(), String> {
        self.custody.revalidate().await?;
        check_legacy_sources(
            &self.root,
            self.custody
                .snapshot
                .legacy
                .as_ref()
                .ok_or("legacy binding absent")?,
            false,
        )
        .await
    }

    /// Caller prerequisite: full MayDrain Durable/NothingRunning AND actual
    /// owned process exit evidence. File sharing alone does not prove that an
    /// already mapped reader or a copied legacy executable has stopped.
    pub async fn exclude_managed_images(mut self) -> Result<LegacyImageCustody, String> {
        self.revalidate().await?;
        let purpose = self
            .custody
            .snapshot
            .legacy
            .as_ref()
            .ok_or("legacy binding absent")?
            .clone();
        check_legacy_sources(&self.root, &purpose, true).await?;
        let sid = super::super::super::super::caller_sid().await?;
        self.custody._bootstrap_pins.clear();
        let texts: Vec<String> = std::iter::once(purpose.active.clone())
            .chain(purpose.previous.clone())
            .collect();
        let mut files = crate::installed_release::ExclusiveFiles::default();
        let releases = crate::installed_release::exclude_receipts(
            &texts,
            &self.root,
            Path::new(&purpose.payload),
            &sid,
            &mut files,
        )?;
        let directory = Path::new(&purpose.bootstrap)
            .parent()
            .ok_or("bootstrap directory absent")?;
        let hashes: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&purpose.bootstrap_manifest).map_err(|e| e.to_string())?;
        let mut manifest_pin = files.pin(&directory.join("bootstrap-hashes.json"))?;
        let mut manifest_text = String::new();
        (&mut manifest_pin)
            .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
            .read_to_string(&mut manifest_text)
            .map_err(|e| e.to_string())?;
        if manifest_text != purpose.bootstrap_manifest {
            return Err("bootstrap manifest changed before exclusive custody".into());
        }
        let mut bootstrap = vec![manifest_pin];
        use sha2::{Digest, Sha256};
        for (name, expected) in hashes {
            if name != "continuum.exe"
                && name != "runtime-libs.txt"
                && !(name.ends_with(".dll")
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))
            {
                return Err("unsafe legacy bootstrap closure member".into());
            }
            let path = directory.join(name);
            crate::installed_release::unredirected(&path)?;
            let mut file = files.pin(&path)?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
            if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(&expected) {
                return Err("bootstrap changed before exclusive custody".into());
            }
            bootstrap.push(file);
        }
        self.revalidate().await?;
        Ok(LegacyImageCustody {
            window: self,
            _releases: releases,
            _bootstrap: bootstrap,
            files,
        })
    }

    /// Only after the artifact owner has reconciled this exact operation.
    pub async fn restore(mut self) -> Result<LegacyRestoration, String> {
        self.revalidate().await?;
        check_legacy_sources(
            &self.root,
            self.custody
                .snapshot
                .legacy
                .as_ref()
                .ok_or("legacy binding absent")?,
            true,
        )
        .await?;
        if self.custody._bootstrap_pins.is_empty() {
            self.custody._bootstrap_pins = pin_verified_bootstrap(&self.root, false).await?;
        }
        self.custody.begin_restoration().await
    }
}

/// Original serving expectation remains durable until the caller verifies it.
/// Dropping this value preserves Restoring evidence for explicit continuation.
pub struct LegacyRestoration {
    custody: SchedulerCustody,
}

impl LegacyRestoration {
    pub async fn resume(
        root: &Path,
        payload: &Path,
        operation_digest: &str,
        source: &Path,
    ) -> Result<Self, String> {
        require_idle_claim(root)?;
        let custody = SchedulerCustody::load_recovery(root, Some(operation_digest)).await?;
        let purpose = custody
            .snapshot
            .legacy
            .as_ref()
            .ok_or("legacy restoration binding absent")?;
        if purpose.source != canonical_source(source)? || Path::new(&purpose.payload) != payload {
            return Err("legacy restoration source/payload differs".into());
        }
        if purpose.phase != LegacyPhase::Restoring {
            return Err("placement reconciliation is still required".into());
        }
        refuse_own_image(root, purpose).await?;
        check_legacy_sources(root, purpose, true).await?;
        custody.enable_for_verification().await
    }

    pub fn serving_expectation(&self) -> &serde_json::Value {
        &self
            .custody
            .snapshot
            .legacy
            .as_ref()
            .expect("legacy restoring binding")
            .serving_expectation // Private construction requires Restoring journal.
    }

    /// Caller must first compare the live serving state to serving_expectation.
    pub async fn finish(mut self) -> Result<(), String> {
        let root = self.custody.journal.parent().ok_or("journal root absent")?;
        self.custody._launch_gate = Some(launch_gate(root, true)?);
        check_restoring_startup(&self.custody.snapshot, &read_tasks().await?)?;
        check_legacy_sources(
            root,
            self.custody
                .snapshot
                .legacy
                .as_ref()
                .ok_or("legacy binding absent")?,
            true,
        )
        .await?;
        set_enabled(
            &self.custody.snapshot,
            true,
            false,
            &self.custody._lease,
            &self.custody.journal,
            &self.custody.journal_pin,
            &self.custody._bootstrap_pins,
        )
        .await?;
        self.custody.snapshot.check_restored(&read_tasks().await?)?;
        drop(self.custody.journal_pin);
        std::fs::remove_file(&self.custody.journal)
            .map_err(|e| format!("verified restoration evidence retained: {e}"))
    }
}

impl LegacyImageCustody {
    pub fn held_image_paths(&self) -> impl Iterator<Item = &Path> {
        self.files.paths()
    }
    pub fn exclusive_files(&mut self) -> &mut crate::installed_release::ExclusiveFiles {
        &mut self.files
    }
    pub fn operation_digest(&self) -> &str {
        self.window.operation_digest()
    }
    pub async fn revalidate(&self) -> Result<(), String> {
        self.window.revalidate().await
    }
    pub async fn restore(self) -> Result<LegacyRestoration, String> {
        let Self {
            window,
            _releases,
            _bootstrap,
            files,
        } = self;
        drop(_releases);
        drop(_bootstrap);
        drop(files);
        window.restore().await
    }
}

/// A failed generic disable has not touched serving or artifacts; restore even
/// partial scheduler mutation. Legacy placement keeps its explicit recovery authority.
pub(super) async fn settle_disable<T, F, R>(
    custody: T,
    legacy: bool,
    disabled: Result<(), String>,
    restore: F,
) -> Result<T, String>
where
    F: FnOnce(T) -> R,
    R: std::future::Future<Output = Result<(), String>>,
{
    match disabled {
        Ok(()) => Ok(custody),
        Err(error) if legacy => Err(error),
        Err(error) => match restore(custody).await {
            Ok(()) => Err(format!("maintenance disable failed; original scheduler intent restored: {error}")),
            Err(recovery) => Err(format!("maintenance disable failed: {error}; restoration failed: {recovery}; run continuum maintenance recover")),
        },
    }
}
