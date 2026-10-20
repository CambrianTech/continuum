use super::*;

// Reuse the existing scheduler probe deadline; this is not a drain timeout.
pub(super) const PROBE: std::time::Duration = std::time::Duration::from_secs(30);
pub(super) const JOURNAL: &str = "supervisor-maintenance.json";
pub(super) const READ: &str = r#"
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

pub(super) async fn read_tasks() -> Result<Vec<TaskSnapshot>, String> {
    let script=format!("{READ}\n@((Read-TaskSnapshot 'ContinuumCore'),(Read-TaskSnapshot 'ContinuumDeploy')) | ConvertTo-Json -Depth 4 -Compress");
    let raw = super::super::super::super::powershell(&script, PROBE).await?;
    serde_json::from_str(&raw).map_err(|e| format!("scheduler maintenance snapshot: {e}"))
}

pub(super) fn mutation_script(
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
pub(super) fn bootstrap_policy_script() -> Result<String, String> {
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

pub(super) fn registered_action_matches(
    task: &super::super::super::super::TaskReport,
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

pub(super) async fn pin_verified_bootstrap(root: &Path, cooperative: bool) -> Result<Vec<File>, String> {
    use crate::installed_release;
    let core = super::super::super::super::task_report(super::super::super::super::CORE_TASK).await?;
    let deploy = super::super::super::super::task_report(super::super::super::super::DEPLOY_TASK).await?;
    let sid = super::super::super::super::caller_sid().await?;
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
    if super::super::super::super::powershell(&script, PROBE).await? != "cooperating" {
        return Err("protected supervisor capability was not established".into());
    }
    // Recheck actual registrations after the capability probe. The caller's
    // snapshot identity check also binds these observations to mutation.
    if super::super::super::super::task_report(super::super::super::super::CORE_TASK).await? != core
        || super::super::super::super::task_report(super::super::super::super::DEPLOY_TASK).await? != deploy
    {
        return Err("supervisor registration changed during capability verification".into());
    }
    Ok(pins)
}

pub(super) async fn set_enabled(
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
        super::super::super::super::run_installer_script(&shell, &script)
    })
    .await
    .map_err(|e| format!("maintenance mutation worker failed: {e}"))??;
    let now = read_tasks().await?;
    snapshot.check_identity(&now)?;
    if snapshot.tasks.iter().zip(&now).any(|(saved, now)| {
        let expected =
            restore && saved.enabled && (!core_only || saved.name == super::super::super::super::CORE_TASK);
        now.enabled != expected || (expected == saved.enabled && now.xml != saved.xml)
    }) {
        return Err(
            "scheduler enabled/XML verification failed; recovery evidence retained".into(),
        );
    }
    Ok(())
}
