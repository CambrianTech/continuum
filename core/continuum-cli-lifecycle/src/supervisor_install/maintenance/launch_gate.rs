use super::*;

#[cfg(windows)]
pub(super) mod windows {
    use super::super::*;
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        os::windows::fs::OpenOptionsExt,
        path::{Path, PathBuf},
    };

    const LAUNCH_GATE: &str = "supervisor-maintenance-launch.lock";

    fn launch_gate(root: &Path, exclusive: bool) -> Result<File, String> {
        crate::installed_release::unredirected(root)?;
        let path = root.join(LAUNCH_GATE);
        if path.symlink_metadata().is_ok() {
            crate::installed_release::unredirected(&path)?;
        }
        // All cooperating openers deny deletion. Windows symmetric sharing
        // admits concurrent startup readers or one maintenance owner, never both.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(if exclusive { 0 } else { 3 })
            .open(&path)
            .map_err(|e| format!("cannot acquire maintenance launch custody: {e}"))?;
        crate::installed_release::unredirected(&path)?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("maintenance launch gate is not a regular file".into());
        }
        Ok(file)
    }

    /// Held only from startup admission through child creation, not child exit.
    /// This is distinct from install.lock, which the installing parent retains
    /// while waiting for the newly started core to become ready.
    pub struct StartupPermit {
        _gate: File,
        _evidence: Option<File>,
    }

    impl StartupPermit {
        pub async fn acquire(root: &Path) -> Result<Self, String> {
            let gate = launch_gate(root, false)?;
            match root.join(JOURNAL).symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                    _gate: gate,
                    _evidence: None,
                }),
                Ok(_) => {
                    let (file, snapshot) = read_snapshot(root)?;
                    if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
                        return Err(
                            "scheduler maintenance recovery is pending; startup refused".into()
                        );
                    }
                    check_restoring_startup(&snapshot, &read_tasks().await?)?;
                    let purpose = snapshot
                        .legacy
                        .as_ref()
                        .ok_or("legacy restoration binding absent")?;
                    // Launch cannot redirect to a different selection while verification is pending.
                    if crate::installed_release::read_receipt(&root.join("install-active.json"))?
                        != purpose.active
                        || previous_receipt(root)? != purpose.previous
                    {
                        return Err("restoration launch selection changed".into());
                    }
                    Ok(Self {
                        _gate: gate,
                        _evidence: Some(file),
                    })
                }
                Err(e) => Err(format!(
                    "cannot establish maintenance state before startup: {e}"
                )),
            }
        }
    }

    // Reuse the existing scheduler probe deadline; this is not a drain timeout.
    const PROBE: std::time::Duration = std::time::Duration::from_secs(30);
    const JOURNAL: &str = "supervisor-maintenance.json";
    const READ: &str = r#"
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$scheduler=New-Object -ComObject 'Schedule.Service'; $scheduler.Connect()
$folder=$scheduler.GetFolder('\')
function Read-TaskSnapshot([string]$name) {
    $task=$folder.GetTask($name)
    $raw=[string]$task.Xml
    $doc=New-Object System.Xml.XmlDocument; $doc.PreserveWhitespace=$true; $doc.LoadXml($raw)
    $ns=New-Object System.Xml.XmlNamespaceManager($doc.NameTable)
    $ns.AddNamespace('t','http://schemas.microsoft.com/windows/2004/02/mit/task')
    $enabled=$doc.SelectSingleNode('/t:Task/t:Settings/t:Enabled',$ns)
    if ($null -eq $enabled) { throw 'Task enabled setting is absent; refusing maintenance' }
    $enabled.InnerText='true'
    [pscustomobject]@{name=$name;xml=$raw;identityXml=$doc.OuterXml;sddl=[string]$task.GetSecurityDescriptor(7);enabled=[bool]$task.Enabled;state=[int]$task.State}
}
"#;

    async fn read_tasks() -> Result<Vec<TaskSnapshot>, String> {
        let script=format!("{READ}\n@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress");
        let raw = super::super::super::powershell(&script, PROBE).await?;
        serde_json::from_str(&raw).map_err(|e| format!("scheduler maintenance snapshot: {e}"))
    }

    fn mutation_script(
        snapshot: &RecoverySnapshot,
        restore: bool,
        core_only: bool,
        journal: &Path,
    ) -> Result<String, String> {
        snapshot.validate()?;
        let json = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?; // Task Scheduler subprocess input boundary.
        let input = {
            use sha2::{Digest, Sha256};
            let digest = format!("{:x}", Sha256::digest(&json));
            let path = journal
                .to_str()
                .ok_or("maintenance journal path is not Unicode")?
                .replace('\'', "''");
            format!(
                r#"$bytes=[IO.File]::ReadAllBytes('{path}')
$sha=[Security.Cryptography.SHA256]::Create()
try {{ $digest=[BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-','').ToLowerInvariant() }} finally {{ $sha.Dispose() }}
if ($digest -cne '{digest}') {{ throw 'Maintenance evidence changed before mutation' }}
$saved=[Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json"#
            )
        };
        let script = format!(
            r#"{READ}
{input}
# Check both registrations before changing either, then each again immediately
# before mutation. The install lease excludes cooperating registration writers.
foreach ($entry in $saved.tasks) {{
    $now=Read-TaskSnapshot $entry.name
    if ($now.identityXml -cne $entry.identityXml -or $now.sddl -cne $entry.sddl) {{ throw 'Task registration/security drift; recovery evidence retained' }}
}}
foreach ($entry in $saved.tasks) {{
    $now=Read-TaskSnapshot $entry.name
    if ($now.identityXml -cne $entry.identityXml -or $now.sddl -cne $entry.sddl) {{ throw 'Task registration/security drift before mutation' }}
    $task=$folder.GetTask($entry.name)
    $task.Enabled={desired}
}}
"#,
            desired = if restore && core_only {
                "([bool]$entry.enabled -and $entry.name -ceq 'ContinuumCore')"
            } else if restore {
                "[bool]$entry.enabled"
            } else {
                "$false"
            }
        );
        Ok(script)
    }

    // Reuse the installer's exact protected-loader policy, compiled into this
    // reader. Never dot-source a mutable checkout or probe an unchecked command.
    fn bootstrap_policy_script() -> Result<String, String> {
        let source = include_str!("../../../../../tools/scripts/lib/windows-prepared.ps1");
        let mut script = String::from("$ErrorActionPreference='Stop';\n");
        for name in [
            "ConvertTo-CoreImagePath",
            "Assert-CorePreparedPath",
            "Get-CoreSupervisorBootstrap",
            "Assert-CoreSupervisorLocation",
            "Assert-CoreSupervisorBootstrap",
            "Assert-CoreBootstrapAccess",
        ] {
            let marker = format!("function {name} {{");
            let start = source
                .find(&marker)
                .ok_or_else(|| format!("missing bootstrap policy {name}"))?;
            let rest = &source[start..];
            let end = rest.find("\nfunction ").unwrap_or(rest.len());
            script.push_str(&rest[..end]);
            script.push('\n');
        }
        Ok(script)
    }

    fn registered_action_matches(
        task: &super::super::super::TaskReport,
        envelope: &crate::installed_release::Envelope,
        mode: &str,
        sid: &str,
    ) -> bool {
        task.present
            && task.actions == 1
            && task.user_sid == sid
            && task.command.eq_ignore_ascii_case(&envelope.bootstrap)
            && task.arguments == envelope.arguments(mode)
            && Path::new(&task.working_directory)
                == Path::new(&envelope.bootstrap)
                    .parent()
                    .unwrap_or(Path::new(""))
    }

    async fn pin_verified_bootstrap(root: &Path, cooperative: bool) -> Result<Vec<File>, String> {
        use crate::installed_release;
        let core = super::super::super::task_report(super::super::super::CORE_TASK).await?;
        let deploy = super::super::super::task_report(super::super::super::DEPLOY_TASK).await?;
        let sid = super::super::super::caller_sid().await?;
        let envelope = installed_release::envelope(&core.description)?
            .ok_or("maintenance requires a protected supervisor; finish the supported installer transition first")?;
        let program_files =
            PathBuf::from(std::env::var_os("ProgramFiles").ok_or("ProgramFiles is unset")?);
        envelope.validate(root, &program_files, &sid)?;
        if !registered_action_matches(&core, &envelope, "core", &sid)
            || !registered_action_matches(&deploy, &envelope, "deploy", &sid)
        {
            return Err("registered supervisor actions differ from protected authority; no task was disabled".into());
        }
        let directory = Path::new(&envelope.bootstrap)
            .parent()
            .ok_or("bootstrap directory absent")?;
        let pin = |path: &Path| -> Result<File, String> {
            installed_release::unredirected(path)?;
            let file = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(path)
                .map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("bootstrap input is not regular".into());
            }
            Ok(file)
        };
        let mut manifest = pin(&directory.join("bootstrap-hashes.json"))?;
        let mut bytes = Vec::new();
        (&mut manifest)
            .take(installed_release::MAX_RECEIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > installed_release::MAX_RECEIPT_BYTES {
            return Err("bootstrap manifest exceeds installed receipt bound".into());
        }
        let hashes: std::collections::BTreeMap<String, String> =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if !hashes.contains_key("continuum.exe") {
            return Err("bootstrap manifest lacks executable".into());
        }
        let mut pins = vec![manifest];
        for name in hashes.keys() {
            if name != "continuum.exe"
                && name != "runtime-libs.txt"
                && !(name.ends_with(".dll")
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))
            {
                return Err("unsafe bootstrap manifest member".into());
            }
            pins.push(pin(&directory.join(name))?);
        }
        let mut script = bootstrap_policy_script()?;
        let executable = envelope.bootstrap.replace('\'', "''");
        let accepted = if cooperative {
            "$protocol -cne '4'"
        } else {
            "$protocol -notin @('3','4')"
        };
        script.push_str(&format!("\nAssert-CoreSupervisorBootstrap -Path '{executable}' -UserSid '{sid}'\n$protocol = (& '{executable}' installed-service --protocol | Out-String).Trim()\nif ($LASTEXITCODE -ne 0 -or {accepted}) {{ throw 'Maintenance requires a verified supported supervisor protocol; finish the supported installer transition before disabling tasks.' }}\n'cooperating'"));
        if super::super::super::powershell(&script, PROBE).await? != "cooperating" {
            return Err("protected supervisor capability was not established".into());
        }
        // Recheck actual registrations after the capability probe. The caller's
        // snapshot identity check also binds these observations to mutation.
        if super::super::super::task_report(super::super::super::CORE_TASK).await? != core
            || super::super::super::task_report(super::super::super::DEPLOY_TASK).await? != deploy
        {
            return Err("supervisor registration changed during capability verification".into());
        }
        Ok(pins)
    }

    async fn set_enabled(
        snapshot: &RecoverySnapshot,
        restore: bool,
        core_only: bool,
        lease: &File,
        journal: &Path,
        journal_pin: &File,
        bootstrap_pins: &[File],
    ) -> Result<(), String> {
        let script = mutation_script(snapshot, restore, core_only, journal)?;
        // A probe timeout/kill-on-drop is not mutation custody. The existing
        // foreground installer runner waits for child exit; its worker retains
        // the actual lease even if this async caller is cancelled.
        let pins = mutation_pins(lease, journal_pin)?;
        let images = bootstrap_pins
            .iter()
            .map(File::try_clone)
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        let shell = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        tokio::task::spawn_blocking(move || {
            let _pins = pins;
            let _images = images;
            super::super::super::run_installer_script(&shell, &script)
        })
        .await
        .map_err(|e| format!("maintenance mutation worker failed: {e}"))??;
        let now = read_tasks().await?;
        snapshot.check_identity(&now)?;
        if snapshot.tasks.iter().zip(&now).any(|(saved, now)| {
            let expected =
                restore && saved.enabled && (!core_only || saved.name == super::super::super::CORE_TASK);
            now.enabled != expected || (expected == saved.enabled && now.xml != saved.xml)
        }) {
            return Err(
                "scheduler enabled/XML verification failed; recovery evidence retained".into(),
            );
        }
        Ok(())
    }

    fn mutation_pins(lease: &File, journal: &File) -> Result<(File, File), String> {
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
            set_enabled(
                &snapshot,
                false,
                false,
                &lease,
                &journal,
                &file,
                &bootstrap_pins,
            )
            .await?;
            Ok(Self {
                snapshot,
                journal,
                journal_pin: file,
                _lease: lease,
                _launch_gate: Some(launch_gate),
                _bootstrap_pins: bootstrap_pins,
            })
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

    fn check_restoring_startup(
        snapshot: &RecoverySnapshot,
        tasks: &[TaskSnapshot],
    ) -> Result<(), String> {
        if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
            return Err("scheduler exclusion remains active".into());
        }
        snapshot.check_identity(tasks)?;
        if snapshot.tasks.iter().zip(tasks).any(|(saved, now)| {
            now.enabled != (saved.enabled && saved.name == super::super::super::CORE_TASK)
        }) {
            return Err(
                "restoration startup requires original core state and disabled deploy task".into(),
            );
        }
        Ok(())
    }

    fn publish_restoring(
        journal: &Path,
        old_pin: File,
        snapshot: &RecoverySnapshot,
    ) -> Result<File, String> {
        snapshot.validate()?;
        if snapshot.legacy.as_ref().map(|p| p.phase) != Some(LegacyPhase::Restoring) {
            return Err("only restoration phase can replace maintenance evidence".into());
        }
        let bytes = serde_json::to_vec(snapshot).map_err(|e| e.to_string())?; // Durable maintenance phase wire.
        let temporary = journal.with_extension("json.restoring");
        crate::installed_release::unredirected(&temporary)?;
        let mut staged = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&temporary)
            .map_err(|e| format!("cannot stage restoration phase: {e}"))?;
        if !staged.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("restoration stage is not a file".into());
        }
        let mut prior = Vec::new();
        (&mut staged)
            .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
            .read_to_end(&mut prior)
            .map_err(|e| e.to_string())?;
        // The exact named stage may contain a crash-short prefix of this same
        // snapshot. Unknown/different bytes are retained, never overwritten.
        if !bytes.starts_with(&prior) {
            return Err("restoration staging identity differs; evidence retained".into());
        }
        staged
            .seek(SeekFrom::Start(0))
            .and_then(|_| staged.write_all(&bytes))
            .and_then(|_| staged.sync_all())
            .map_err(|e| e.to_string())?;
        drop(staged);
        drop(old_pin);
        // Lifecycle cannot depend on core's fs_portable owner. Use its same
        // Windows write-through replacement contract at this journal boundary.
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>()
        };
        let from = wide(&temporary);
        let to = wide(journal);
        // SAFETY: NUL-terminated UTF-16 buffers outlive the synchronous OS call.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "cannot publish restoration phase: {}",
                std::io::Error::last_os_error()
            ));
        }
        let (pin, actual) = read_snapshot(journal.parent().ok_or("journal root absent")?)?;
        let actual_bytes = serde_json::to_vec(&actual).map_err(|e| e.to_string())?; // Compare exact committed phase payload.
        if actual_bytes != bytes {
            return Err("published restoration evidence changed".into());
        }
        Ok(pin)
    }

    fn read_snapshot(root: &Path) -> Result<(File, RecoverySnapshot), String> {
        let journal = root.join(JOURNAL);
        crate::installed_release::unredirected(&journal)?;
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&journal)
            .map_err(|e| format!("cannot pin maintenance evidence: {e}"))?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("maintenance recovery evidence is not a regular file".into());
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > crate::installed_release::MAX_RECEIPT_BYTES {
            return Err("oversized maintenance recovery evidence retained".into());
        }
        let snapshot: RecoverySnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| format!("corrupt maintenance evidence retained: {e}"))?;
        snapshot.validate()?;
        Ok((file, snapshot))
    }

    fn require_idle_claim(root: &Path) -> Result<(), String> {
        let path = crate::deploy_claim::claim_path(root);
        match path.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("cannot establish deploy ownership: {e}")),
            Ok(_) => {}
        }
        let text = crate::installed_release::read_receipt(&path)?;
        let claim: crate::deploy_claim::DeployClaim = serde_json::from_str(&text)
            .map_err(|e| format!("unreadable deploy claim blocks maintenance: {e}"))?;
        if claim.pid <= 0 || crate::deploy_claim::owner_alive(claim.pid) {
            return Err("existing deploy owner must finish before legacy maintenance".into());
        }
        Ok(())
    }

    fn previous_receipt(root: &Path) -> Result<Option<String>, String> {
        let path = root.join("install-previous.json");
        match path.symlink_metadata() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
            Ok(_) => crate::installed_release::read_receipt(&path).map(Some),
        }
    }

    async fn check_legacy_sources(
        root: &Path,
        purpose: &LegacyPurpose,
        manifest: bool,
    ) -> Result<(), String> {
        require_idle_claim(root)?;
        if crate::installed_release::read_receipt(&root.join("install-active.json"))?
            != purpose.active
            || previous_receipt(root)? != purpose.previous
        {
            return Err("installed selections changed during legacy maintenance".into());
        }
        let core = super::super::super::task_report(super::super::super::CORE_TASK).await?;
        let envelope = crate::installed_release::envelope(&core.description)?
            .ok_or("protected bootstrap absent")?;
        if envelope.bootstrap != purpose.bootstrap {
            return Err("protected bootstrap changed during maintenance".into());
        }
        if manifest
            && crate::installed_release::read_receipt(
                &Path::new(&purpose.bootstrap)
                    .parent()
                    .ok_or("bootstrap parent absent")?
                    .join("bootstrap-hashes.json"),
            )? != purpose.bootstrap_manifest
        {
            return Err("protected bootstrap closure changed during maintenance".into());
        }
        Ok(())
    }

    fn canonical_source(source: &Path) -> Result<String, String> {
        if !source.is_absolute()
            || source
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("legacy source must be absolute and normalized".into());
        }
        crate::installed_release::unredirected(source)?;
        let physical = match source.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Successful retirement removes only the source leaf. Its existing
                // physical parent still binds recovery to the exact saved location.
                let parent = source.parent().ok_or("legacy source parent absent")?;
                let name = source.file_name().ok_or("legacy source filename absent")?;
                parent
                    .canonicalize()
                    .map_err(|e| format!("cannot resolve retired source parent: {e}"))?
                    .join(name)
            }
            Err(error) => return Err(error.to_string()),
        };
        physical
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "legacy source is not UTF-8".into())
    }

    async fn refuse_own_image(root: &Path, purpose: &LegacyPurpose) -> Result<(), String> {
        let own = std::env::current_exe()
            .and_then(|p| p.canonicalize())
            .map_err(|e| e.to_string())?;
        let bootstrap = Path::new(&purpose.bootstrap)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if own == bootstrap {
            return Err("maintenance must run outside the protected bootstrap".into());
        }
        let sid = super::super::super::caller_sid().await?;
        for text in std::iter::once(&purpose.active).chain(purpose.previous.iter()) {
            let validated = crate::installed_release::validate_receipt(
                text,
                root,
                Path::new(&purpose.payload),
                &sid,
            )?;
            let release = &validated.release;
            for image in [
                &release.artifact,
                &release.cli,
                &release.launcher,
                &release.engine,
            ] {
                if own == Path::new(image).canonicalize().map_err(|e| e.to_string())? {
                    return Err("maintenance must run outside the managed release images".into());
                }
            }
        }
        Ok(())
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
            let core = super::super::super::task_report(super::super::super::CORE_TASK).await?;
            let envelope = crate::installed_release::envelope(&core.description)?
                .ok_or("protected bootstrap absent")?;
            let active = crate::installed_release::read_receipt(&root.join("install-active.json"))?;
            let previous = previous_receipt(root)?;
            let sid = super::super::super::caller_sid().await?;
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
                return Err(
                    "restoration already pending; resume verification without draining".into(),
                );
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

        pub fn recovery_expectation(
            root: &Path,
            source: &Path,
        ) -> Result<serde_json::Value, String> {
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
            let sid = super::super::super::caller_sid().await?;
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
            if purpose.source != canonical_source(source)? || Path::new(&purpose.payload) != payload
            {
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

    #[cfg(test)]
    mod tests {
        use super::super::*;

        #[test]
        fn legacy_claim_and_operation_refuse_ambiguous_recovery() {
            let temp = tempfile::tempdir().unwrap();
            require_idle_claim(temp.path()).unwrap();
            // The real deploy-claim writer creates this parent before publication.
            std::fs::create_dir_all(temp.path().join("run")).unwrap();
            std::fs::write(crate::deploy_claim::claim_path(temp.path()), b"corrupt").unwrap();
            assert!(require_idle_claim(temp.path()).is_err());
            std::fs::write(
                crate::deploy_claim::claim_path(temp.path()),
                format!(
                    r#"{{"pid":{},"started_ms":0,"target_sha":"fixture"}}"#,
                    std::process::id()
                ),
            )
            .unwrap();
            assert!(require_idle_claim(temp.path())
                .unwrap_err()
                .contains("existing deploy owner"));
            let mut snapshot = RecoverySnapshot {
                version: 1,
                legacy: None,
                tasks: [
                    super::super::super::super::CORE_TASK,
                    super::super::super::super::DEPLOY_TASK,
                ]
                .into_iter()
                .map(|name| TaskSnapshot {
                    name: name.into(),
                    xml: "xml".into(),
                    identity_xml: "identity".into(),
                    sddl: "security".into(),
                    enabled: true,
                    state: 0,
                })
                .collect(),
            };
            snapshot.check_operation(None).unwrap();
            let digest = "a".repeat(64);
            snapshot.legacy = Some(LegacyPurpose {
                operation_digest: digest.clone(),
                source: canonical_source(temp.path()).unwrap(),
                serving_expectation: serde_json::json!({"schema": 1, "build": "original"}),
                phase: LegacyPhase::Excluded,
                payload: "payload".into(),
                bootstrap: "bootstrap".into(),
                bootstrap_manifest: "{}".into(),
                active: "active".into(),
                previous: None,
            });
            assert!(snapshot.check_operation(None).is_err());
            assert!(snapshot.check_operation(Some(&"b".repeat(64))).is_err());
            snapshot.check_operation(Some(&digest)).unwrap();
            let restored: RecoverySnapshot =
                serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
            restored.check_operation(Some(&digest)).unwrap();
            std::fs::write(
                temp.path().join(JOURNAL),
                serde_json::to_vec(&snapshot).unwrap(),
            )
            .unwrap();
            assert_eq!(
                LegacySchedulerWindow::recovery_binding(temp.path(), temp.path()).unwrap(),
                digest
            );
            assert_eq!(
                LegacySchedulerWindow::recovery_expectation(temp.path(), temp.path()).unwrap()
                    ["build"],
                "original"
            );
            let mut oversized = snapshot.clone();
            oversized.legacy.as_mut().unwrap().serving_expectation = serde_json::json!({"schema": 1, "data": "x".repeat(crate::installed_release::MAX_RECEIPT_BYTES as usize)});
            assert!(oversized.validate().is_err());
            let mut unknown_schema = snapshot.clone();
            unknown_schema.legacy.as_mut().unwrap().serving_expectation =
                serde_json::json!({"schema": 2});
            assert!(unknown_schema.validate().is_err());
            let other = tempfile::tempdir().unwrap();
            assert!(LegacySchedulerWindow::recovery_binding(temp.path(), other.path()).is_err());
            let mut invalid = snapshot.clone();
            invalid.legacy.as_mut().unwrap().source = "relative".into();
            assert!(invalid.validate().is_err());
            let (_, excluded) = read_snapshot(temp.path()).unwrap();
            assert!(check_restoring_startup(&excluded, &excluded.tasks).is_err());
            let mut restoring = snapshot.clone();
            restoring.legacy.as_mut().unwrap().phase = LegacyPhase::Restoring;
            let journal = temp.path().join(JOURNAL);
            let stage = journal.with_extension("json.restoring");
            let bytes = serde_json::to_vec(&restoring).unwrap();
            std::fs::write(&stage, &bytes[..bytes.len() / 2]).unwrap();
            let (old_pin, _) = read_snapshot(temp.path()).unwrap();
            let pin = publish_restoring(&journal, old_pin, &restoring).unwrap();
            assert!(LegacySchedulerWindow::restoration_pending(temp.path(), temp.path()).unwrap());
            assert_eq!(
                LegacySchedulerWindow::recovery_expectation(temp.path(), temp.path()).unwrap()
                    ["build"],
                "original"
            );
            let mut staged_tasks = restoring.tasks.clone();
            staged_tasks[1].enabled = false;
            check_restoring_startup(&restoring, &staged_tasks).unwrap();
            assert!(check_restoring_startup(&restoring, &restoring.tasks).is_err());
            let mut partial = staged_tasks.clone();
            partial[0].enabled = false;
            assert!(check_restoring_startup(&restoring, &partial).is_err());
            partial = staged_tasks.clone();
            partial[1].sddl = "changed".into();
            assert!(check_restoring_startup(&restoring, &partial).is_err());
            drop(pin);
            std::fs::write(&stage, b"unknown").unwrap();
            let (old_pin, _) = read_snapshot(temp.path()).unwrap();
            assert!(publish_restoring(&journal, old_pin, &restoring).is_err());
            assert_eq!(std::fs::read(&journal).unwrap(), bytes);
            let retired_source = temp.path().join("retired-source");
            std::fs::create_dir(&retired_source).unwrap();
            let mut retired = snapshot.clone();
            retired.legacy.as_mut().unwrap().source = canonical_source(&retired_source).unwrap();
            retired.legacy.as_mut().unwrap().phase = LegacyPhase::Restoring;
            std::fs::write(&journal, serde_json::to_vec(&retired).unwrap()).unwrap();
            std::fs::remove_dir(&retired_source).unwrap();
            assert_eq!(
                LegacySchedulerWindow::recovery_binding(temp.path(), &retired_source).unwrap(),
                digest
            );
            assert!(
                LegacySchedulerWindow::restoration_pending(temp.path(), &retired_source).unwrap()
            );
            assert!(LegacySchedulerWindow::recovery_binding(
                temp.path(),
                &temp.path().join("other-missing")
            )
            .is_err());
            assert!(canonical_source(&temp.path().join("missing-parent").join("source")).is_err());

            std::fs::write(temp.path().join(JOURNAL), b"corrupt").unwrap();
            assert!(LegacySchedulerWindow::recovery_binding(temp.path(), temp.path()).is_err());
        }

        #[tokio::test]
        async fn legacy_own_bootstrap_refuses_before_scheduler_or_release_access() {
            let temp = tempfile::tempdir().unwrap();
            let purpose = LegacyPurpose {
                operation_digest: "a".repeat(64),
                source: canonical_source(temp.path()).unwrap(),
                serving_expectation: serde_json::json!({"schema": 1}),
                phase: LegacyPhase::Excluded,
                payload: temp.path().to_str().unwrap().into(),
                bootstrap: std::env::current_exe().unwrap().to_str().unwrap().into(),
                bootstrap_manifest: "{}".into(),
                active: "invalid unopened receipt".into(),
                previous: None,
            };
            assert!(refuse_own_image(temp.path(), &purpose)
                .await
                .unwrap_err()
                .contains("outside the protected bootstrap"));
            assert!(!temp.path().join(JOURNAL).exists());
        }

        #[test]
        fn bootstrap_policy_and_registered_actions_remain_bound() {
            let script = bootstrap_policy_script().unwrap();
            assert!(!script.contains("$PSScriptRoot"));
            assert!(script.contains("function Assert-CoreSupervisorBootstrap {"));
            assert!(script.contains("function Assert-CoreBootstrapAccess {"));
            assert!(!script.contains("function Install-CoreSupervisorBootstrap {"));
            let envelope = crate::installed_release::Envelope {
                schema: 2,
                active_release: r"C:\home\install-active.json".into(),
                bootstrap: r"C:\protected\continuum.exe".into(),
            };
            let mut task = super::super::super::super::TaskReport {
                present: true,
                actions: 1,
                user_sid: "S-1-5-21-1".into(),
                command: envelope.bootstrap.clone(),
                working_directory: r"C:\protected".into(),
                arguments: envelope.arguments("deploy"),
                ..Default::default()
            };
            assert!(registered_action_matches(
                &task,
                &envelope,
                "deploy",
                "S-1-5-21-1"
            ));
            task.command = r"C:\untrusted\continuum.exe".into();
            assert!(!registered_action_matches(
                &task,
                &envelope,
                "deploy",
                "S-1-5-21-1"
            ));
            task.command = envelope.bootstrap.clone();
            task.arguments = envelope.arguments("core");
            assert!(!registered_action_matches(
                &task,
                &envelope,
                "deploy",
                "S-1-5-21-1"
            ));
        }

        #[tokio::test]
        async fn cancelled_waiter_retains_install_and_evidence_custody_until_worker_exit() {
            let temp = tempfile::tempdir().unwrap();
            let startup = StartupPermit::acquire(temp.path()).await.unwrap();
            let second_startup = StartupPermit::acquire(temp.path()).await.unwrap();
            assert!(launch_gate(temp.path(), true).is_err());
            drop(startup);
            drop(second_startup);
            let maintenance = launch_gate(temp.path(), true).unwrap();
            assert!(StartupPermit::acquire(temp.path()).await.is_err());
            drop(maintenance);
            let lease = take_install_lease(temp.path()).unwrap();
            let journal = temp.path().join(JOURNAL);
            std::fs::write(&journal, b"preserved evidence").unwrap();
            let evidence = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&journal)
                .unwrap();
            let pins = mutation_pins(&lease, &evidence).unwrap();
            let (release, wait) = std::sync::mpsc::channel::<()>();
            let (exited, exit) = std::sync::mpsc::channel();
            let worker = tokio::task::spawn_blocking(move || {
                let _ = wait.recv();
                drop(pins);
                let _ = exited.send(());
            });
            drop(worker); // Cancelling the wait must not cancel foreground custody.
            drop(lease);
            drop(evidence);
            assert!(take_install_lease(temp.path()).is_err());
            assert!(OpenOptions::new().write(true).open(&journal).is_err());
            drop(release);
            exit.recv().unwrap();
            assert!(take_install_lease(temp.path()).is_ok());
            assert!(OpenOptions::new().write(true).open(&journal).is_ok());
        }

        // Only the external scheduler object is replaced. The production XML
        // reader and mutation script execute unchanged against this object.
        const SCHEDULER: &str = r#"
$script:failDeploy=$false
$script:tasks=@{}
foreach ($name in @('ContinuumCore','ContinuumDeploy')) {
    $task=[pscustomobject]@{Name=$name;Value=$true;State=4;Security='O:SYG:SYD:(A;;FA;;;SY)';Extra=''}
    $task | Add-Member ScriptProperty Enabled { $this.Value } {
        param($value)
        if ($script:failDeploy -and $this.Name -eq 'ContinuumDeploy') { throw 'injected interrupted mutation' }
        $this.Value=[bool]$value
    }
    $task | Add-Member ScriptProperty Xml {
        '<Task xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><Settings><Enabled>' + $this.Value.ToString().ToLowerInvariant() + '</Enabled></Settings>' + $this.Extra + '</Task>'
    }
    $task | Add-Member ScriptMethod GetSecurityDescriptor { param($flags) if ($flags -ne 7) { throw 'owner/group/DACL required' }; $this.Security }
    $script:tasks[$name]=$task
}
$folder=New-Object PSObject
$folder | Add-Member ScriptMethod GetTask { param($name) $script:tasks[$name] }
"#;

        fn isolated(script: &str) -> String {
            let connect = "$scheduler=New-Object -ComObject 'Schedule.Service'; $scheduler.Connect()\n$folder=$scheduler.GetFolder('\\')";
            assert_eq!(
                script.matches(connect).count(),
                1,
                "fixture replaces only the scheduler boundary"
            );
            script.replace(connect, "# disposable scheduler object supplied by fixture")
        }

        #[tokio::test]
        async fn interrupted_scheduler_changes_recover_without_replacing_registration() {
            let reader = isolated(READ);
            let original=super::super::super::super::powershell(
                &format!("{SCHEDULER}\n{reader}\n@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress"), PROBE).await.unwrap();
            let snapshot = RecoverySnapshot {
                version: 1,
                legacy: None,
                tasks: serde_json::from_str(&original).unwrap(),
            };
            let temp = tempfile::tempdir().unwrap();
            let lease = take_install_lease(temp.path()).unwrap();
            assert!(
                take_install_lease(temp.path()).is_err(),
                "real install lease excludes another owner"
            );
            let journal = temp.path().join(JOURNAL);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&journal)
                .unwrap();
            serde_json::to_writer(&mut file, &snapshot).unwrap();
            file.sync_all().unwrap();
            drop(file);
            assert!(
                StartupPermit::acquire(temp.path()).await.is_err(),
                "durable journal blocks startup even without a live maintenance handle"
            );
            let journal_pin = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&journal)
                .unwrap();
            assert!(OpenOptions::new().write(true).open(&journal).is_err());
            let mut saved: RecoverySnapshot =
                serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
            saved.legacy = Some(LegacyPurpose {
                operation_digest: "a".repeat(64),
                source: canonical_source(temp.path()).unwrap(),
                serving_expectation: serde_json::json!({"schema": 1, "build": "original"}),
                phase: LegacyPhase::Restoring,
                payload: temp.path().to_str().unwrap().into(),
                bootstrap: "fixture".into(),
                bootstrap_manifest: "{}".into(),
                active: "fixture".into(),
                previous: None,
            });
            let journal_pin = publish_restoring(&journal, journal_pin, &saved).unwrap();
            let core_only = isolated(&mutation_script(&saved, true, true, &journal).unwrap());
            let disable = isolated(&mutation_script(&saved, false, false, &journal).unwrap());
            let restore = isolated(&mutation_script(&saved, true, false, &journal).unwrap());
            let output_path = temp.path().join("observed.json");
            let output_literal = output_path.to_str().unwrap().replace('\'', "''");
            let script = format!(
                r#"{SCHEDULER}
$script:failDeploy=$true
try {{ {disable} }} catch {{ $firstError=$_.Exception.Message }}
if (!$firstError -or $tasks['ContinuumCore'].Enabled -or !$tasks['ContinuumDeploy'].Enabled) {{ throw 'first disable interruption was not observed' }}
$script:failDeploy=$false
{restore}
{disable}
$script:failDeploy=$true
try {{ {restore} }} catch {{ $secondError=$_.Exception.Message }}
if (!$secondError -or !$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'partial restore was not observed' }}
$script:failDeploy=$false
{restore}
$tasks['ContinuumDeploy'].Extra='<Actions />'
try {{ {disable} }} catch {{ $xmlError=$_.Exception.Message }}
if (!$xmlError -or !$tasks['ContinuumCore'].Enabled) {{ throw 'XML drift changed another task' }}
$tasks['ContinuumDeploy'].Extra=''
$tasks['ContinuumDeploy'].Security='changed'
try {{ {disable} }} catch {{ $securityError=$_.Exception.Message }}
if (!$securityError -or !$tasks['ContinuumCore'].Enabled) {{ throw 'security drift changed another task' }}
$tasks['ContinuumDeploy'].Security='O:SYG:SYD:(A;;FA;;;SY)'
# A failed start/acceptance leaves core enabled and deploy disabled.
{core_only}
if (!$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'restoration did not isolate deployment' }}
# Simulate finish enabling deploy followed by a crash before evidence removal.
{restore}
if (!$tasks['ContinuumDeploy'].Enabled) {{ throw 'finish did not restore original deployment state' }}
# The same saved phase resumes verification without migration or new identity.
{core_only}
if (!$tasks['ContinuumCore'].Enabled -or $tasks['ContinuumDeploy'].Enabled) {{ throw 'interrupted finish could not resume verification' }}
{restore}
$finalJson=@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress
[IO.File]::WriteAllText('{output_literal}', $finalJson, [Text.UTF8Encoding]::new($false))
"#
            );
            let script_path = temp.path().join("scheduler-fixture.ps1");
            std::fs::write(&script_path, script).unwrap();
            let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            super::super::super::super::run_installer_script(
                &shell,
                &format!("& '{}'", script_path.to_str().unwrap().replace('\'', "''")),
            )
            .unwrap();
            let final_state = std::fs::read_to_string(output_path).unwrap();
            let final_state: Vec<TaskSnapshot> = serde_json::from_str(&final_state).unwrap();
            assert_eq!(
                final_state, snapshot.tasks,
                "exact original XML/security/enabled state recovered"
            );
            assert_eq!(
                std::fs::read(&journal).unwrap(),
                serde_json::to_vec(&saved).unwrap(),
                "interruption keeps original evidence"
            );
            drop(journal_pin);
            drop(lease);
            assert!(take_install_lease(temp.path()).is_ok());
        }
    }
}
