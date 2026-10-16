//! Recoverable scheduler exclusion under the existing installation lease.
//! This does not stop running processes or exclude manual/legacy launchers.
//! It is deliberately not an expert-container migration authorization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskSnapshot {
    pub name: String,
    /// Actual scheduler XML, retained for recovery/audit, never regenerated.
    pub xml: String,
    /// The same XML with Settings/Enabled normalized by the scheduler reader.
    pub identity_xml: String,
    pub sddl: String,
    pub enabled: bool,
    /// Observation only: disabling a task does not terminate this state.
    pub state: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoverySnapshot {
    pub version: u32,
    pub tasks: Vec<TaskSnapshot>,
}

impl RecoverySnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.tasks.len() != 2
            || self.tasks[0].name != super::CORE_TASK
            || self.tasks[1].name != super::DEPLOY_TASK
            || self
                .tasks
                .iter()
                .any(|t| t.xml.is_empty() || t.identity_xml.is_empty() || t.sddl.is_empty())
        {
            return Err(
                "invalid scheduler maintenance recovery snapshot; evidence retained".into(),
            );
        }
        Ok(())
    }

    /// Enabled may differ after an interrupted disable/restore; no other
    /// registration or security change is ours to overwrite.
    pub fn check_identity(&self, observed: &[TaskSnapshot]) -> Result<(), String> {
        self.validate()?;
        if observed.len() != self.tasks.len()
            || self.tasks.iter().zip(observed).any(|(saved, now)| {
                saved.name != now.name
                    || saved.identity_xml != now.identity_xml
                    || saved.sddl != now.sddl
            })
        {
            return Err(
                "scheduled task identity/security changed; recovery refuses to overwrite it".into(),
            );
        }
        Ok(())
    }

    pub fn check_disabled(&self, observed: &[TaskSnapshot]) -> Result<(), String> {
        self.check_identity(observed)?;
        if observed.iter().any(|t| t.enabled) {
            return Err("maintenance task was re-enabled".into());
        }
        Ok(())
    }
}

/// The same FileShare::None lease held by the installer/reboot handoff.
#[cfg(windows)]
pub fn take_install_lease(root: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::create_dir_all(root)
        .map_err(|e| format!("cannot create install lease directory: {e}"))?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(root.join("install.lock"))
        .map_err(|e| {
            format!(
                "another installer/reboot holds the install lease, or it cannot be acquired: {e}"
            )
        })
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Write},
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
    }

    impl StartupPermit {
        pub fn acquire(root: &Path) -> Result<Self, String> {
            let gate = launch_gate(root, false)?;
            match root.join(JOURNAL).symlink_metadata() {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self { _gate: gate }),
                Ok(_) => Err("scheduler maintenance recovery is pending; startup refused".into()),
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
        let raw = super::super::powershell(&script, PROBE).await?;
        serde_json::from_str(&raw).map_err(|e| format!("scheduler maintenance snapshot: {e}"))
    }

    fn mutation_script(
        snapshot: &RecoverySnapshot,
        restore: bool,
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
            desired = if restore {
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
        let source = include_str!("../../../../tools/scripts/lib/windows-prepared.ps1");
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
        task: &super::super::TaskReport,
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

    async fn pin_cooperating_bootstrap(root: &Path) -> Result<Vec<File>, String> {
        use crate::installed_release;
        let core = super::super::task_report(super::super::CORE_TASK).await?;
        let deploy = super::super::task_report(super::super::DEPLOY_TASK).await?;
        let sid = super::super::caller_sid().await?;
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
        script.push_str(&format!("\nAssert-CoreSupervisorBootstrap -Path '{executable}' -UserSid '{sid}'\n$protocol = (& '{executable}' installed-service --protocol | Out-String).Trim()\nif ($LASTEXITCODE -ne 0 -or $protocol -cne '4') {{ throw 'Maintenance requires supervisor protocol 4; finish the supported installer transition before disabling tasks.' }}\n'cooperating'"));
        if super::super::powershell(&script, PROBE).await? != "cooperating" {
            return Err("protected supervisor capability was not established".into());
        }
        // Recheck actual registrations after the capability probe. The caller's
        // snapshot identity check also binds these observations to mutation.
        if super::super::task_report(super::super::CORE_TASK).await? != core
            || super::super::task_report(super::super::DEPLOY_TASK).await? != deploy
        {
            return Err("supervisor registration changed during capability verification".into());
        }
        Ok(pins)
    }

    async fn set_enabled(
        snapshot: &RecoverySnapshot,
        restore: bool,
        lease: &File,
        journal: &Path,
        journal_pin: &File,
        bootstrap_pins: &[File],
    ) -> Result<(), String> {
        let script = mutation_script(snapshot, restore, journal)?;
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
            super::super::run_installer_script(&shell, &script)
        })
        .await
        .map_err(|e| format!("maintenance mutation worker failed: {e}"))??;
        let now = read_tasks().await?;
        snapshot.check_identity(&now)?;
        if snapshot.tasks.iter().zip(&now).any(|(saved, now)| {
            now.enabled != (restore && saved.enabled) || (restore && now.xml != saved.xml)
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
        _launch_gate: File,
        _bootstrap_pins: Vec<File>,
    }

    impl SchedulerCustody {
        pub async fn disable(root: &Path) -> Result<Self, String> {
            let lease = take_install_lease(root)?;
            let launch_gate = launch_gate(root, true)?;
            let snapshot = RecoverySnapshot {
                version: 1,
                tasks: read_tasks().await?,
            };
            snapshot.validate()?;
            let bootstrap_pins = pin_cooperating_bootstrap(root).await?;
            snapshot.check_identity(&read_tasks().await?)?;
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
            set_enabled(&snapshot, false, &lease, &journal, &file, &bootstrap_pins).await?;
            Ok(Self {
                snapshot,
                journal,
                journal_pin: file,
                _lease: lease,
                _launch_gate: launch_gate,
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
            set_enabled(
                &self.snapshot,
                true,
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

        /// The integration owner must first reconcile any interrupted placement
        /// and establish its separate legacy/manual launcher exclusion. This
        /// method restores scheduler registration only; it cannot certify that
        /// re-enabling old launchers is safe for a partially retired artifact.
        pub async fn recover(root: &Path) -> Result<(), String> {
            let lease = take_install_lease(root)?;
            let launch_gate = launch_gate(root, true)?;
            let journal = root.join(JOURNAL);
            crate::installed_release::unredirected(&journal)?;
            let mut file = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&journal)
                .map_err(|e| format!("cannot pin maintenance evidence: {e}"))?;
            let mut bytes = Vec::new();
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("maintenance recovery evidence is not a regular file".into());
            }
            (&mut file)
                .take(crate::installed_release::MAX_RECEIPT_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| format!("cannot read maintenance evidence: {e}"))?;
            if bytes.len() as u64 > crate::installed_release::MAX_RECEIPT_BYTES {
                return Err("oversized maintenance recovery evidence retained".into());
            }
            let snapshot: RecoverySnapshot = serde_json::from_slice(&bytes)
                .map_err(|e| format!("corrupt maintenance evidence retained: {e}"))?;
            snapshot.validate()?;
            let bootstrap_pins = pin_cooperating_bootstrap(root).await?;
            snapshot.check_identity(&read_tasks().await?)?;
            Self {
                snapshot,
                journal,
                journal_pin: file,
                _lease: lease,
                _launch_gate: launch_gate,
                _bootstrap_pins: bootstrap_pins,
            }
            .restore()
            .await
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

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
            let mut task = super::super::super::TaskReport {
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
            let startup = StartupPermit::acquire(temp.path()).unwrap();
            let second_startup = StartupPermit::acquire(temp.path()).unwrap();
            assert!(launch_gate(temp.path(), true).is_err());
            drop(startup);
            drop(second_startup);
            let maintenance = launch_gate(temp.path(), true).unwrap();
            assert!(StartupPermit::acquire(temp.path()).is_err());
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
            let original=super::super::super::powershell(
                &format!("{SCHEDULER}\n{reader}\n@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress"), PROBE).await.unwrap();
            let snapshot = RecoverySnapshot {
                version: 1,
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
                StartupPermit::acquire(temp.path()).is_err(),
                "durable journal blocks startup even without a live maintenance handle"
            );
            let journal_pin = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&journal)
                .unwrap();
            assert!(OpenOptions::new().write(true).open(&journal).is_err());
            let saved: RecoverySnapshot =
                serde_json::from_slice(&std::fs::read(&journal).unwrap()).unwrap();
            let disable = isolated(&mutation_script(&saved, false, &journal).unwrap());
            let restore = isolated(&mutation_script(&saved, true, &journal).unwrap());
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
$finalJson=@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress
[IO.File]::WriteAllText('{output_literal}', $finalJson, [Text.UTF8Encoding]::new($false))
"#
            );
            let script_path = temp.path().join("scheduler-fixture.ps1");
            std::fs::write(&script_path, script).unwrap();
            let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
            super::super::super::run_installer_script(
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

#[cfg(windows)]
pub use windows::{SchedulerCustody, StartupPermit};
