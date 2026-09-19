//! `continuum install --supervisor`: converge this machine's OS supervisor to the
//! contract, from the binary, the same way every time.
//!
//! Joel, 2026-09-18: *"What's the repeatable process or inside an install? … Are all
//! concerns wired into the same location? Same strategy. Do NOT hand jack."* and
//! *"it needs to be in our own binary here."* The contract (card 7b56a84b, audit
//! §5 D): **the core is a service, not a session child** — supervised by the OS,
//! run whether or not anyone is logged on, unprivileged. On Windows that is a
//! Scheduled Task with an S4U principal (no stored password, no interactive
//! session to die with) and a boot trigger, plus its sibling `ContinuumDeploy`
//! (the deploy consumer, every ten minutes, same principal). Until 2026-09-19 the
//! core ran `Interactive` + AtLogOn and the 5090 went dark for 2 h 42 m when the
//! logon session tore down.
//!
//! What this verb does NOT do: build, stage, or pick the release. The installed
//! release is the one the `ContinuumCore` task already names — its description IS
//! the descriptor (`CoreServiceDescription`), the single source of the slot. This
//! verb re-registers that same release under the contract's principal and
//! trigger, registers the consumer beside it, grants the caller read/execute on
//! both, and verifies by re-reading the scheduler — never by its own exit code.
//!
//! Elevation: registering an S4U task needs an administrator token. The
//! unelevated verb writes a typed plan, asks Windows for ONE consent
//! (`Start-Process -Verb RunAs` on this same binary with `--elevated --plan`),
//! and the elevated child does only the registration. The core and every build
//! stay unelevated.
//!
//! The pure halves — the task XML the scheduler is handed, the drift verdict, the
//! caller-access read of an SDDL — are platform-independent and pinned by tests.
//! macOS (a LaunchDaemon, Fable's arm) and Linux (`systemd --user` + linger,
//! Cormac's arm) plug into the same verb; until they land the verb says which
//! owner, never "unsupported".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub(super) const CORE_TASK: &str = "ContinuumCore";
pub(super) const DEPLOY_TASK: &str = "ContinuumDeploy";
/// How often the supervisor's sibling asks whether a deploy is owed.
pub(super) const DEPLOY_EVERY_MIN: u32 = 10;
/// A deploy tick may build: 50 min on the 5090, 70 on IntelMac. Four hours is the
/// wall past which the tick is a hang, not a build.
const DEPLOY_TIME_LIMIT: &str = "PT4H";

/// The registration plan: what the elevated child registers, and nothing else.
/// Same shape the installer's PowerShell registrar takes (`register-core-service.ps1`),
/// so a plan is a plan whichever registrar reads it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct SupervisorPlan {
    /// The core task's command (the hidden-launcher shell).
    pub shell: String,
    /// The core task's arguments (the launcher's `-File … -CorePath …` line).
    pub arguments: String,
    /// The release descriptor JSON — the task's description, the slot's single source.
    pub description: String,
    /// The caller's SID: the principal both tasks run as, and the grantee of read/execute.
    pub user_sid: String,
    /// The installed CLI the deploy consumer runs.
    pub cli: String,
}

/// When a task fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Trigger {
    /// At system startup — the supervisor's trigger: no logon needed.
    Boot,
    /// Every `minutes`, from `start` (ISO 8601 local, no zone), indefinitely.
    Every { minutes: u32, start: String },
}

/// One scheduled task as the contract states it. Rendered to Task Scheduler XML
/// (`schtasks /Create /XML`), which is the one representation that carries an S4U
/// principal, a boot trigger and restart-on-failure declaratively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskSpec {
    pub name: &'static str,
    pub description: String,
    pub user_sid: String,
    pub trigger: Trigger,
    pub command: String,
    pub arguments: String,
    /// ISO 8601 duration; `PT0S` = unlimited (the supervisor never times out).
    pub execution_time_limit: &'static str,
    /// `(interval, count)`: relaunch the action when it exits non-zero.
    pub restart_on_failure: Option<(&'static str, u32)>,
}

impl TaskSpec {
    /// The supervisor: the installer's launcher line, S4U at boot, relaunched a
    /// minute after any exit, forever.
    pub fn core(plan: &SupervisorPlan) -> Self {
        Self {
            name: CORE_TASK,
            description: plan.description.clone(),
            user_sid: plan.user_sid.clone(),
            trigger: Trigger::Boot,
            command: plan.shell.clone(),
            arguments: plan.arguments.clone(),
            execution_time_limit: "PT0S",
            restart_on_failure: Some(("PT1M", 999)),
        }
    }

    /// The deploy consumer: the installed CLI, S4U, every ten minutes from `start`.
    pub fn deploy(plan: &SupervisorPlan, start: String) -> Self {
        Self {
            name: DEPLOY_TASK,
            description: "Continuum deploy consumer: turns a DeployRequest from the Rust tracker into reboot --service. Registered by `continuum install`; session-independent.".to_string(),
            user_sid: plan.user_sid.clone(),
            trigger: Trigger::Every { minutes: DEPLOY_EVERY_MIN, start },
            command: plan.cli.clone(),
            arguments: "deploy-consume".to_string(),
            execution_time_limit: DEPLOY_TIME_LIMIT,
            restart_on_failure: None,
        }
    }

    /// The Task Scheduler 1.4 document for this spec. Values are XML-escaped; the
    /// principal is the SID (the scheduler resolves and displays the name).
    pub fn to_xml(&self) -> String {
        let trigger = match &self.trigger {
            Trigger::Boot => "<BootTrigger><Enabled>true</Enabled></BootTrigger>".to_string(),
            Trigger::Every { minutes, start } => format!(
                "<TimeTrigger><StartBoundary>{}</StartBoundary><Enabled>true</Enabled>\
                 <Repetition><Interval>PT{minutes}M</Interval><StopAtDurationEnd>false</StopAtDurationEnd></Repetition></TimeTrigger>",
                xml_escape(start)
            ),
        };
        let restart = match self.restart_on_failure {
            Some((interval, count)) => {
                format!("<RestartOnFailure><Interval>{interval}</Interval><Count>{count}</Count></RestartOnFailure>")
            }
            None => String::new(),
        };
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\
             <Task version=\"1.4\" xmlns=\"http://schemas.microsoft.com/windows/2004/02/mit/task\">\
             <RegistrationInfo><Description>{desc}</Description></RegistrationInfo>\
             <Triggers>{trigger}</Triggers>\
             <Principals><Principal id=\"Author\"><UserId>{sid}</UserId><LogonType>S4U</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>\
             <Settings>\
             <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>\
             <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>\
             <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>\
             <AllowHardTerminate>true</AllowHardTerminate>\
             <StartWhenAvailable>true</StartWhenAvailable>\
             <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>\
             <AllowStartOnDemand>true</AllowStartOnDemand>\
             <Enabled>true</Enabled>\
             <Hidden>false</Hidden>\
             <RunOnlyIfIdle>false</RunOnlyIfIdle>\
             <WakeToRun>false</WakeToRun>\
             <ExecutionTimeLimit>{limit}</ExecutionTimeLimit>\
             {restart}\
             <Priority>7</Priority>\
             </Settings>\
             <Actions Context=\"Author\"><Exec><Command>{cmd}</Command><Arguments>{args}</Arguments></Exec></Actions>\
             </Task>",
            desc = xml_escape(&self.description),
            sid = xml_escape(&self.user_sid),
            limit = self.execution_time_limit,
            cmd = xml_escape(&self.command),
            args = xml_escape(&self.arguments),
        )
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A task as the scheduler reports it, through one query — enough to judge drift
/// from the contract. `present: false` is the whole report for an absent task.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct TaskReport {
    pub present: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub arguments: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub state: String,
    /// `S4U`, `Interactive`, `Password`, `ServiceAccount`, …
    #[serde(default)]
    pub logon_type: String,
    /// `Limited` (= LeastPrivilege) or `Highest`.
    #[serde(default)]
    pub run_level: String,
    /// The principal resolved to a SID (empty when it could not be).
    #[serde(default)]
    pub user_sid: String,
    /// CIM class names: `MSFT_TaskBootTrigger`, `MSFT_TaskLogonTrigger`, `MSFT_TaskTimeTrigger`, …
    #[serde(default, deserialize_with = "string_or_seq")]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub actions: u32,
    /// The task's security descriptor, SDDL.
    #[serde(default)]
    pub sddl: String,
}

/// PowerShell 5.1 serializes a one-element array property as a bare string; the
/// report must read either.
fn string_or_seq<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(s) => vec![s],
        OneOrMany::Many(v) => v,
    })
}

/// One way a registered task differs from the contract. Each names WHAT, so the
/// receipt can say what the elevation changed — or what it failed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Drift {
    Absent,
    /// The principal outlives no session: anything but S4U dies with a logon.
    LogonType(String),
    /// Unprivileged, always.
    RunLevel(String),
    /// The wrong trigger class (a logon trigger needs a logon).
    Triggers(Vec<String>),
    /// Not the release/arguments the plan names, or not one action.
    Action,
    Disabled,
    /// Another principal's task.
    Principal(String),
    /// The caller cannot `schtasks /Run` it (`continuum start`'s handoff needs RX).
    CallerAccess,
}

/// The drift of both tasks from what `plan` would register. Pure; the elevated
/// step runs only when this is non-empty, and the verify after it demands empty.
pub(super) fn drift(core: &TaskReport, deploy: &TaskReport, plan: &SupervisorPlan) -> Vec<(&'static str, Drift)> {
    let mut out = Vec::new();
    let core_spec = TaskSpec::core(plan);
    for d in drift_of(core, &core_spec, "MSFT_TaskBootTrigger") {
        out.push((CORE_TASK, d));
    }
    let deploy_spec = TaskSpec::deploy(plan, String::new());
    for d in drift_of(deploy, &deploy_spec, "MSFT_TaskTimeTrigger") {
        out.push((DEPLOY_TASK, d));
    }
    out
}

fn drift_of(report: &TaskReport, spec: &TaskSpec, trigger_class: &str) -> Vec<Drift> {
    if !report.present {
        return vec![Drift::Absent];
    }
    let mut out = Vec::new();
    if report.logon_type != "S4U" {
        out.push(Drift::LogonType(report.logon_type.clone()));
    }
    if report.run_level != "Limited" {
        out.push(Drift::RunLevel(report.run_level.clone()));
    }
    if report.triggers.len() != 1 || report.triggers[0] != trigger_class {
        out.push(Drift::Triggers(report.triggers.clone()));
    }
    if report.actions != 1
        || !same_path(&report.command, &spec.command)
        || report.arguments != spec.arguments
    {
        out.push(Drift::Action);
    }
    if !report.enabled {
        out.push(Drift::Disabled);
    }
    if !report.user_sid.eq_ignore_ascii_case(&spec.user_sid) {
        out.push(Drift::Principal(report.user_sid.clone()));
    }
    if !caller_has_read_execute(&report.sddl, &spec.user_sid) {
        out.push(Drift::CallerAccess);
    }
    out
}

/// Two Windows paths name the same file when they agree ignoring case and
/// separator spelling (the scheduler echoes `Execute` as registered).
fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_matches('"').to_ascii_lowercase();
    norm(a) == norm(b)
}

/// The digest that binds a plan file to the elevated child. Computed from the bytes
/// the parent WROTE (never a re-read), carried on the RunAs argv, re-derived by the
/// child from what it reads: a same-user process can rewrite a file in `%TEMP%`
/// between the write and the consent, but not the argv of a process already spawned
/// (Fable, review of #4232 — the elevated child would otherwise register a boot task
/// as ANY account, from a file anyone running as this user can edit).
pub(super) fn plan_digest(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Does `sddl` grant `sid` read + execute on the task, with no explicit deny for it?
///
/// The scheduler stores a task's security as the task file's DACL; a grant of RX
/// reads as `(A;;0x1200a9;;;SID)` (FILE_GENERIC_READ|FILE_GENERIC_EXECUTE) or its
/// alias `FRFX`, and full access `FA` covers it. Any `(D;…;SID)` deny outranks a
/// grant, so it fails closed. Never reads inherited ACEs as the caller's own grant.
pub(super) fn caller_has_read_execute(sddl: &str, sid: &str) -> bool {
    let sid_l = sid.to_ascii_lowercase();
    let mut granted = false;
    for ace in sddl.split('(').skip(1) {
        let ace = ace.trim_end_matches(')');
        let fields: Vec<&str> = ace.split(';').collect();
        if fields.len() < 6 || !fields[5].eq_ignore_ascii_case(&sid_l) {
            continue;
        }
        match fields[0] {
            "D" => return false,
            "A" => {
                let rights = fields[2].to_ascii_uppercase();
                // A hex mask is read as BITS: RX = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
                // (0x1200a9); any mask containing them (full control 0x1f01ff included)
                // grants it — never a string match on one spelling (Cormac, #4232).
                const RX: u32 = 0x1200a9;
                let mask_grants = rights
                    .strip_prefix("0X")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .is_some_and(|mask| mask & RX == RX);
                granted |= mask_grants
                    || rights == "FA"
                    || rights == "GA"
                    || (rights.contains("FR") && rights.contains("FX"))
                    || (rights.contains("GR") && rights.contains("GX"));
            }
            _ => {}
        }
    }
    granted
}

/// What one arm of `install` found and did. The orchestrator sums these: a bare
/// `install` exits non-zero only when drift remains after every arm has run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ArmReport {
    /// Ways the arm's subject differed from the contract when read.
    pub drift_before: usize,
    /// Ways still differing after the arm ran (equal to `drift_before` under --check).
    pub drift_after: usize,
}

impl ArmReport {
    pub fn converged() -> Self {
        Self::default()
    }
    pub fn read_only(drift: usize) -> Self {
        Self { drift_before: drift, drift_after: drift }
    }
}

/// The install verb's options. Bare `install` runs EVERY arm (Joel: "you want users
/// to remember almost nothing"); naming arms restricts it to those.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct InstallOptions {
    pub supervisor: bool,
    /// The CLI on PATH (`continuum`, `uu`) follows the installed release's CLI.
    pub cli: bool,
    /// The running core is the checkout's HEAD: build, stage, hand off when not.
    pub core: bool,
    /// macOS only: the per-user LaunchAgent instead of the system LaunchDaemon (no sudo;
    /// cannot heal on a gui domain in on-demand-only mode — measured, IntelMac). The
    /// daemon is the default nobody has to remember; this is the explicit other choice.
    pub user: bool,
    /// Report drift and change nothing (exit non-zero when drifted).
    pub check: bool,
    /// The elevated child: register from `plan`, exit. Never spawns another elevation.
    pub elevated: bool,
    pub plan: Option<PathBuf>,
    /// SHA-256 (hex) of the plan bytes the parent wrote, carried on the RunAs argv —
    /// the one thing another same-user process cannot rewrite after the spawn.
    pub plan_sha: Option<String>,
}

impl InstallOptions {
    pub fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--supervisor" if !options.supervisor => options.supervisor = true,
                "--cli" if !options.cli => options.cli = true,
                "--core" if !options.core => options.core = true,
                "--user" if !options.user => options.user = true,
                "--check" if !options.check => options.check = true,
                "--elevated" if !options.elevated => options.elevated = true,
                "--plan" if options.plan.is_none() => {
                    let path = args.next().ok_or("install: --plan needs a path")?;
                    options.plan = Some(PathBuf::from(path));
                }
                "--plan-sha" if options.plan_sha.is_none() => {
                    let sha = args.next().ok_or("install: --plan-sha needs a hex digest")?;
                    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err("install: --plan-sha is a 64-hex SHA-256".to_string());
                    }
                    options.plan_sha = Some(sha.to_ascii_lowercase());
                }
                "--supervisor" | "--cli" | "--core" | "--user" | "--check" | "--elevated" | "--plan" | "--plan-sha" => {
                    return Err(format!("duplicate option {arg}"))
                }
                _ => return Err(format!("unknown install option {arg}; use --check, or name arms: --supervisor --core --cli")),
            }
        }
        if options.check && options.elevated {
            return Err("install: --check reads; --elevated writes — not both".to_string());
        }
        if options.elevated != options.plan.is_some() || options.elevated != options.plan_sha.is_some() {
            return Err("install: --elevated, --plan and --plan-sha go together (the elevated child registers exactly one plan, bound by its digest)".to_string());
        }
        if options.elevated && (options.cli || options.core || options.user) {
            return Err("install: the elevated child registers the supervisor plan only".to_string());
        }
        Ok(options)
    }

    /// Bare `install` = every arm.
    pub fn runs(&self, arm: Arm) -> bool {
        let named = self.supervisor || self.cli || self.core;
        !named
            || match arm {
                Arm::Supervisor => self.supervisor,
                Arm::Core => self.core,
                Arm::Cli => self.cli,
            }
    }
}

/// The arms of `install`, in the order they run: the supervisor must be prepared
/// before a core can be handed to it; the slot's CLI is fresh only after a stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Arm {
    Supervisor,
    Core,
    Cli,
}

impl Arm {
    pub fn name(self) -> &'static str {
        match self {
            Arm::Supervisor => "supervisor",
            Arm::Core => "core",
            Arm::Cli => "cli",
        }
    }
}

/// Task XML as `schtasks /Create /XML` reads it: UTF-16LE with a BOM, matching the
/// document's own `encoding="UTF-16"` declaration.
pub(super) fn write_task_xml(path: &Path, xml: &str) -> Result<(), String> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(path, bytes).map_err(|e| format!("install: cannot write {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// The scheduler boundary (Windows). Everything above is pure and tested; this is
// the only part that talks to Task Scheduler, and it does so through the same
// PowerShell seam `PreparedCoreService` already uses to read the task.
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub(super) async fn powershell(script: &str, timeout: std::time::Duration) -> Result<String, String> {
    use base64::Engine;
    use std::os::windows::process::CommandExt;
    let root = std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?;
    let shell = PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes); // -EncodedCommand is UTF-16LE base64 at this process boundary
    let mut command = std::process::Command::new(shell);
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdin(std::process::Stdio::null())
        .creation_flags(0x0800_0000);
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| "Task Scheduler operation timed out".to_string())?
        .map_err(|e| format!("cannot invoke Task Scheduler: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "Task Scheduler operation failed: {}",
            plain_stderr(&String::from_utf8_lossy(&output.stderr))
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// What PowerShell said, as a human line. Under `-NonInteractive -EncodedCommand`
/// its stderr is CLIXML (`#< CLIXML <Objs …><S S="Error">…</S>`) with CR/LF
/// spelled `_x000D__x000A_`; a consent refusal reads as a 600-byte XML blob
/// unless it is unwrapped. Plain stderr passes through untouched.
pub(super) fn plain_stderr(raw: &str) -> String {
    if !raw.trim_start().starts_with("#< CLIXML") {
        return raw.trim().to_string();
    }
    let mut lines = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<S S=\"Error\">") {
        let body = &rest[start + "<S S=\"Error\">".len()..];
        let Some(end) = body.find("</S>") else { break };
        let text = body[..end]
            .replace("_x000D__x000A_", "")
            .replace("&gt;", ">")
            .replace("&lt;", "<")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&");
        let text = text.trim();
        // Keep the message lines; drop PowerShell's position/category trailer.
        if !text.is_empty() && !text.starts_with('+') && !text.starts_with("At line:") && !text.contains("CategoryInfo") && !text.contains("FullyQualifiedErrorId") {
            lines.push(text.to_string());
        }
        rest = &body[end..];
    }
    if lines.is_empty() {
        raw.trim().to_string()
    } else {
        lines.join(" ")
    }
}

/// One task, as the scheduler reports it. The name is one of this module's
/// constants, never operator input.
#[cfg(windows)]
pub(super) async fn task_report(name: &str) -> Result<TaskReport, String> {
    let script = format!(
        "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); \
         $n='{name}'; $t=Get-ScheduledTask | Where-Object {{ $_.TaskName -eq $n -and $_.TaskPath -eq '\\' }}; \
         if ($null -eq $t) {{ '{{\"present\":false}}'; exit 0 }}; \
         $s=New-Object -ComObject 'Schedule.Service'; $s.Connect(); $sd=[string]$s.GetFolder('\\').GetTask($n).GetSecurityDescriptor(4); \
         $u=[string]$t.Principal.UserId; $sid=''; \
         try {{ if ($u -match '^S-1-') {{ $sid=$u }} else {{ $sid=(New-Object System.Security.Principal.NTAccount($u)).Translate([System.Security.Principal.SecurityIdentifier]).Value }} }} catch {{ $sid='' }}; \
         $a=@($t.Actions); \
         [pscustomobject]@{{present=$true; description=[string]$t.Description; command=$(if ($a.Count -ge 1) {{ [string]$a[0].Execute }} else {{ '' }}); \
         arguments=$(if ($a.Count -ge 1) {{ [string]$a[0].Arguments }} else {{ '' }}); enabled=[bool]$t.Settings.Enabled; state=[string]$t.State; \
         logonType=[string]$t.Principal.LogonType; runLevel=[string]$t.Principal.RunLevel; userSid=$sid; \
         triggers=[string[]]@($t.Triggers | ForEach-Object {{ $_.CimClass.CimClassName }}); actions=$a.Count; sddl=$sd}} | ConvertTo-Json -Compress"
    );
    let json = powershell(&script, std::time::Duration::from_secs(30)).await?;
    serde_json::from_str(&json).map_err(|e| format!("cannot read the {name} task: {e} ({json})"))
}

#[cfg(windows)]
async fn caller_sid() -> Result<String, String> {
    powershell(
        "[Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        std::time::Duration::from_secs(30),
    )
    .await
}

/// The receipt the elevated child leaves beside the plan: the parent reads THIS,
/// never the child's console (an elevated child's stdout is a window that closes).
#[cfg(windows)]
fn receipt_path(plan: &Path) -> PathBuf {
    plan.with_extension("receipt.txt")
}

/// The unelevated verb: read both tasks, judge drift, elevate once if needed,
/// verify by re-reading. `descriptor_cli` reads the installed CLI out of the core
/// task's description (the release descriptor) — the caller owns that type.
#[cfg(windows)]
pub(super) async fn install_supervisor(
    check_only: bool,
    descriptor_cli: impl Fn(&str) -> Result<String, String>,
) -> Result<ArmReport, String> {
    let core = task_report(CORE_TASK).await?;
    if !core.present {
        return Err(format!(
            "continuum install --supervisor: no installed release on this machine (the {CORE_TASK} task is absent). \
             Run the installer once — `powershell -ExecutionPolicy Bypass -File .\\install.ps1` from the checkout — \
             which stages the release and registers it; `continuum install` converges an installed machine."
        ));
    }
    let cli = descriptor_cli(&core.description)?;
    let plan = SupervisorPlan {
        shell: core.command.clone(),
        arguments: core.arguments.clone(),
        description: core.description.clone(),
        user_sid: caller_sid().await?,
        cli,
    };
    let deploy = task_report(DEPLOY_TASK).await?;
    let before = drift(&core, &deploy, &plan);
    if before.is_empty() {
        println!(
            "✓ supervisor: converged — {CORE_TASK} S4U at boot (state {}), {DEPLOY_TASK} S4U every {DEPLOY_EVERY_MIN} min (state {})",
            core.state, deploy.state
        );
        return Ok(ArmReport::converged());
    }
    for (task, d) in &before {
        println!("  supervisor: {task}: {d:?}");
    }
    if check_only {
        println!(
            "✗ supervisor: drifted from the contract in {} way(s); `continuum install` converges it (one elevation)",
            before.len()
        );
        return Ok(ArmReport::read_only(before.len()));
    }
    println!(
        "→ one elevation to register both tasks under the contract (S4U, boot / every {DEPLOY_EVERY_MIN} min); the core and builds stay unelevated"
    );

    let plan_path = std::env::temp_dir().join(format!("continuum-supervisor-{}.json", std::process::id()));
    let receipt = receipt_path(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    let plan_bytes = serde_json::to_vec_pretty(&plan).map_err(|e| e.to_string())?;
    let plan_sha = plan_digest(&plan_bytes);
    std::fs::write(&plan_path, &plan_bytes).map_err(|e| format!("install: cannot write the plan: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("install: own path: {e}"))?;
    let quote = |s: String| s.replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; $p = Start-Process -FilePath '{}' -ArgumentList @('install','--supervisor','--elevated','--plan','{}','--plan-sha','{plan_sha}') -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
        quote(exe.display().to_string()),
        quote(plan_path.display().to_string()),
    );
    // The consent prompt waits for a human; ten minutes is the wall past which nobody is there.
    let elevated = powershell(&script, std::time::Duration::from_secs(600)).await;
    let receipt_text = std::fs::read_to_string(&receipt).unwrap_or_default(); // unwrap_or_default: an absent receipt is reported below as "none", never as success
    let _ = std::fs::remove_file(&plan_path);
    let _ = std::fs::remove_file(&receipt);
    if let Err(why) = elevated {
        if why.contains("canceled by the user") {
            return Err("install: the elevation consent was refused — nothing changed. Re-run `continuum install --supervisor` and approve the prompt to register the supervisor session-independent.".to_string());
        }
        return Err(format!(
            "install: the elevated registration did not complete: {why}\n  receipt: {}",
            if receipt_text.is_empty() {
                "(none — consent refused or the child never ran)"
            } else {
                receipt_text.trim()
            }
        ));
    }
    if !receipt_text.is_empty() {
        println!("{}", receipt_text.trim());
    }
    // Verify by reading the scheduler, not by trusting the exit code.
    let core = task_report(CORE_TASK).await?;
    let deploy = task_report(DEPLOY_TASK).await?;
    let after = drift(&core, &deploy, &plan);
    if !after.is_empty() {
        return Err(format!(
            "install: the registration ran but the scheduler still reports drift: {after:?}"
        ));
    }
    println!(
        "✓ supervisor: converged — {CORE_TASK} S4U at boot (state {}), {DEPLOY_TASK} S4U every {DEPLOY_EVERY_MIN} min (state {}); the caller has read/execute on both",
        core.state, deploy.state
    );
    Ok(ArmReport { drift_before: before.len(), drift_after: 0 })
}

/// The elevated child: register exactly the plan, grant the caller read/execute,
/// leave a receipt. No query, no decision, no second elevation.
#[cfg(windows)]
pub(super) fn install_supervisor_elevated(plan_path: &Path, plan_sha: &str) -> Result<(), String> {
    let receipt = receipt_path(plan_path);
    let result = read_bound_plan(plan_path, plan_sha).and_then(|plan| register_plan(plan_path, plan));
    let text = match &result {
        Ok(lines) => lines.join("\n"),
        Err(why) => format!("elevated registration failed: {why}"),
    };
    let _ = std::fs::write(&receipt, text);
    result.map(|_| ())
}

/// The plan the consent was given for, or a refusal: the bytes on disk must hash to
/// the digest on this process's argv. Pure and pinned — the one gate between "a file
/// in %TEMP%" and "a task that runs as some account at boot".
pub(super) fn read_bound_plan(plan_path: &Path, plan_sha: &str) -> Result<SupervisorPlan, String> {
    let bytes = std::fs::read(plan_path).map_err(|e| format!("cannot read the plan {}: {e}", plan_path.display()))?;
    let actual = plan_digest(&bytes);
    if !actual.eq_ignore_ascii_case(plan_sha) {
        return Err(format!(
            "the plan at {} is not the one the consent was given for (sha {} on argv, {actual} on disk) — refusing to register anything",
            plan_path.display(),
            &plan_sha[..12]
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("the plan is not a SupervisorPlan: {e}"))
}

#[cfg(windows)]
fn register_plan(plan_path: &Path, plan: SupervisorPlan) -> Result<Vec<String>, String> {
    let start = (chrono::Local::now() + chrono::Duration::minutes(1))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let tasks_dir = PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot is unset")?)
        .join("System32")
        .join("Tasks");
    let mut lines = Vec::new();
    for spec in [TaskSpec::core(&plan), TaskSpec::deploy(&plan, start)] {
        let xml_path = plan_path.with_extension(format!("{}.xml", spec.name));
        write_task_xml(&xml_path, &spec.to_xml())?;
        let out = std::process::Command::new("schtasks")
            .args(["/Create", "/TN", spec.name, "/XML"])
            .arg(&xml_path)
            .arg("/F")
            .output()
            .map_err(|e| format!("schtasks: {e}"))?;
        let _ = std::fs::remove_file(&xml_path);
        if !out.status.success() {
            return Err(format!(
                "schtasks refused {}: {} {}",
                spec.name,
                String::from_utf8_lossy(&out.stderr).trim(),
                String::from_utf8_lossy(&out.stdout).trim()
            ));
        }
        // The task's security IS its file's DACL under System32\Tasks. RX lets the
        // caller `schtasks /Run` it (`continuum start`'s handoff); nothing else changes.
        let grant = format!("*{}:RX", plan.user_sid);
        let out = std::process::Command::new("icacls")
            .arg(tasks_dir.join(spec.name))
            .args(["/grant", &grant])
            .output()
            .map_err(|e| format!("icacls: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "icacls could not grant the caller read/execute on {}: {}",
                spec.name,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        lines.push(format!(
            "  registered {} ({}) as S4U, RX granted to {}",
            spec.name,
            match &spec.trigger {
                Trigger::Boot => "at boot".to_string(),
                Trigger::Every { minutes, .. } => format!("every {minutes} min"),
            },
            plan.user_sid
        ));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> SupervisorPlan {
        SupervisorPlan {
            shell: r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string(),
            arguments: r#"-NoProfile -File "C:\slot\run-service-hidden.ps1" -CorePath "C:\slot\continuum-core-server.exe""#.to_string(),
            description: r#"{"artifact":"C:\\slot\\continuum-core-server.exe","cli":"C:\\slot\\continuum.exe"}"#.to_string(),
            user_sid: "S-1-5-21-1-2-3-1004".to_string(),
            cli: r"C:\slot\continuum.exe".to_string(),
        }
    }

    fn converged(spec: &TaskSpec, trigger: &str) -> TaskReport {
        TaskReport {
            present: true,
            description: spec.description.clone(),
            command: spec.command.clone(),
            arguments: spec.arguments.clone(),
            enabled: true,
            state: "Ready".to_string(),
            logon_type: "S4U".to_string(),
            run_level: "Limited".to_string(),
            user_sid: spec.user_sid.clone(),
            triggers: vec![trigger.to_string()],
            actions: 1,
            sddl: format!("D:(A;;0x1200a9;;;{})(A;ID;0x1f019f;;;BA)(A;ID;0x1f019f;;;SY)", spec.user_sid),
        }
    }

    // what this catches: the contract itself, as the scheduler will read it — S4U,
    // least privilege, a BOOT trigger for the core (a logon trigger is the 2 h 42 m
    // outage), restart forever for the core and a bounded repeating tick for the
    // consumer; and that a value with XML metacharacters (quotes in the launcher
    // line, `&` in a path) cannot break the document.
    #[test]
    fn task_xml_states_the_session_independent_contract() {
        let p = plan();
        let core = TaskSpec::core(&p).to_xml();
        assert!(core.contains("<LogonType>S4U</LogonType>"));
        assert!(core.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(core.contains("<BootTrigger>"), "the supervisor fires at boot, not at logon");
        assert!(!core.contains("LogonTrigger"));
        assert!(core.contains("<RestartOnFailure><Interval>PT1M</Interval><Count>999</Count></RestartOnFailure>"));
        assert!(core.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(core.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(core.contains("<Arguments>-NoProfile -File &quot;C:\\slot\\run-service-hidden.ps1&quot;"), "quotes escaped, not dropped");
        assert!(core.contains("<UserId>S-1-5-21-1-2-3-1004</UserId>"));

        let deploy = TaskSpec::deploy(&p, "2026-09-19T12:00:00".to_string()).to_xml();
        assert!(deploy.contains("<LogonType>S4U</LogonType>"), "the consumer rides the same principal");
        assert!(deploy.contains("<TimeTrigger><StartBoundary>2026-09-19T12:00:00</StartBoundary>"));
        assert!(deploy.contains("<Interval>PT10M</Interval><StopAtDurationEnd>false</StopAtDurationEnd>"));
        assert!(deploy.contains("<ExecutionTimeLimit>PT4H</ExecutionTimeLimit>"));
        assert!(!deploy.contains("RestartOnFailure"), "a failed deploy tick is the ledger's business, not a retry loop");
        assert!(deploy.contains("<Command>C:\\slot\\continuum.exe</Command><Arguments>deploy-consume</Arguments>"));

        let mut odd = p.clone();
        odd.cli = r"C:\a&b\continuum.exe".to_string();
        assert!(TaskSpec::deploy(&odd, String::new()).to_xml().contains("C:\\a&amp;b\\continuum.exe"));
    }

    // what this catches: the verdict the elevation is gated on and verified by. The
    // 5090's own task on 2026-09-19 (Interactive + logon trigger) must read as
    // drifted in exactly those two ways; a converged pair reads as nothing; an
    // absent consumer is one drift, not a crash; a task registered for another
    // user or with a deny ACE is never "converged".
    #[test]
    fn drift_names_every_way_a_task_leaves_the_contract() {
        let p = plan();
        let core_spec = TaskSpec::core(&p);
        let deploy_spec = TaskSpec::deploy(&p, String::new());
        let core = converged(&core_spec, "MSFT_TaskBootTrigger");
        let deploy = converged(&deploy_spec, "MSFT_TaskTimeTrigger");
        assert!(drift(&core, &deploy, &p).is_empty(), "converged is silent");

        let mut interactive = core.clone();
        interactive.logon_type = "Interactive".to_string();
        interactive.triggers = vec!["MSFT_TaskLogonTrigger".to_string()];
        assert_eq!(
            drift(&interactive, &deploy, &p),
            vec![
                (CORE_TASK, Drift::LogonType("Interactive".to_string())),
                (CORE_TASK, Drift::Triggers(vec!["MSFT_TaskLogonTrigger".to_string()])),
            ]
        );

        assert_eq!(drift(&core, &TaskReport::default(), &p), vec![(DEPLOY_TASK, Drift::Absent)]);

        let mut theirs = core.clone();
        theirs.user_sid = "S-1-5-21-9-9-9-1001".to_string();
        theirs.sddl = "D:(A;;0x1200a9;;;S-1-5-21-9-9-9-1001)".to_string();
        assert_eq!(
            drift(&theirs, &deploy, &p),
            vec![(CORE_TASK, Drift::Principal("S-1-5-21-9-9-9-1001".to_string())), (CORE_TASK, Drift::CallerAccess)]
        );

        let mut other_release = core.clone();
        other_release.arguments = "-File other.ps1".to_string();
        assert_eq!(drift(&other_release, &deploy, &p), vec![(CORE_TASK, Drift::Action)]);

        let mut spelled = core.clone();
        spelled.command = spelled.command.to_ascii_uppercase();
        assert!(drift(&spelled, &deploy, &p).is_empty(), "case and separators are not drift");
    }

    // what this catches: the ACL read the handoff depends on — RX in either SDDL
    // spelling counts, an inherited admin ACE never counts as the caller's, and a
    // deny for the caller fails closed even beside a grant.
    #[test]
    fn caller_access_reads_grants_and_fails_closed_on_deny() {
        let sid = "S-1-5-21-1-2-3-1004";
        assert!(caller_has_read_execute(&format!("D:(A;;0x1200a9;;;{sid})(A;ID;FA;;;BA)"), sid));
        assert!(caller_has_read_execute(&format!("D:(A;;FRFX;;;{sid})"), sid));
        assert!(caller_has_read_execute(&format!("D:(A;;FA;;;{sid})"), sid));
        assert!(!caller_has_read_execute("D:(A;ID;FA;;;BA)(A;ID;0x1f019f;;;SY)", sid), "admins' inherited ACEs are not the caller's grant");
        assert!(!caller_has_read_execute(&format!("D:(A;;FR;;;{sid})"), sid), "read without execute cannot run the task");
        assert!(caller_has_read_execute(&format!("D:(A;;0x1f01ff;;;{sid})"), sid), "full control as a mask contains RX");
        assert!(!caller_has_read_execute(&format!("D:(A;;0x120089;;;{sid})"), sid), "a read-only mask lacks execute");
        assert!(!caller_has_read_execute(&format!("D:(D;;GX;;;{sid})(A;;0x1200a9;;;{sid})"), sid), "a deny outranks the grant");
        assert!(!caller_has_read_execute("", sid));
    }

    // what this catches: PowerShell's CLIXML stderr (the consent refusal came back
    // as a 600-byte XML blob) reads as its message line; plain stderr is untouched.
    #[test]
    fn clixml_stderr_reads_as_its_message() {
        let raw = "#< CLIXML\n<Objs Version=\"1.1.0.1\"><Obj S=\"progress\"><MS><PR><AV>Preparing modules for first use.</AV></PR></MS></Obj>\
                   <S S=\"Error\">Start-Process : This command cannot be run due to the error: The operation was canceled by the user._x000D__x000A_</S>\
                   <S S=\"Error\">At line:1 char:37_x000D__x000A_</S><S S=\"Error\">+ ... Stop&apos;; $p = Start-Process_x000D__x000A_</S>\
                   <S S=\"Error\">    + CategoryInfo          : InvalidOperation_x000D__x000A_</S></Objs>";
        assert_eq!(
            plain_stderr(raw),
            "Start-Process : This command cannot be run due to the error: The operation was canceled by the user."
        );
        assert_eq!(plain_stderr("  Access is denied.\n"), "Access is denied.");
    }

    // what this catches (Fable, review of #4232): the elevated child registers only
    // the plan the consent was given for. A plan file rewritten under the child —
    // another principal, another command — hashes differently from the digest the
    // parent put on the argv, and the child refuses before touching the scheduler.
    #[test]
    fn a_plan_rewritten_under_the_elevated_child_is_refused() {
        let dir = std::env::temp_dir().join(format!("plan-bind-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap(); // unwrap: a temp dir the test owns
        let path = dir.join("plan.json");
        let bytes = serde_json::to_vec_pretty(&plan()).unwrap(); // unwrap: the fixture serializes
        std::fs::write(&path, &bytes).unwrap(); // unwrap: the test's own file
        let sha = plan_digest(&bytes);
        assert_eq!(read_bound_plan(&path, &sha).unwrap(), plan(), "the bytes the parent wrote"); // unwrap: the valid case — an Err here IS the failure
        assert!(read_bound_plan(&path, &sha.to_ascii_uppercase()).is_ok(), "digest case is not identity");
        let mut swapped = plan();
        swapped.user_sid = "S-1-5-21-9-9-9-500".to_string();
        std::fs::write(&path, serde_json::to_vec_pretty(&swapped).unwrap()).unwrap(); // unwrap: the test's own file
        let why = read_bound_plan(&path, &sha).unwrap_err();
        assert!(why.contains("not the one the consent was given for"), "{why}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a one-element trigger list arriving as a bare string
    // (PowerShell 5.1's ConvertTo-Json) reads the same as the array form.
    #[test]
    fn a_report_reads_one_trigger_as_string_or_array() {
        let one: TaskReport = serde_json::from_str(r#"{"present":true,"triggers":"MSFT_TaskBootTrigger"}"#).unwrap(); // unwrap: the fixture is the valid form under test
        let many: TaskReport = serde_json::from_str(r#"{"present":true,"triggers":["MSFT_TaskBootTrigger"]}"#).unwrap(); // unwrap: the fixture is the valid form under test
        assert_eq!(one.triggers, many.triggers);
        let absent: TaskReport = serde_json::from_str(r#"{"present":false}"#).unwrap(); // unwrap: the fixture is the valid form under test
        assert!(!absent.present);
    }

    // what this catches: the elevated child is a one-plan registrar and nothing
    // else; the flags that make it one travel together or not at all.
    #[test]
    fn install_options_pair_elevated_with_a_plan() {
        let parse = |a: &[&str]| InstallOptions::parse(a.iter().map(|s| s.to_string()));
        assert!(parse(&["--supervisor"]).is_ok());
        let bare = parse(&[]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!([Arm::Supervisor, Arm::Core, Arm::Cli].iter().all(|a| bare.runs(*a)), "bare install runs every arm");
        let one = parse(&["--cli"]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(one.runs(Arm::Cli) && !one.runs(Arm::Core) && !one.runs(Arm::Supervisor), "naming an arm restricts to it");
        assert!(parse(&["--cli", "--elevated", "--plan", "x", "--plan-sha", &"b".repeat(64)]).is_err(), "the elevated child is the supervisor's only");
        assert!(parse(&["--supervisor", "--elevated"]).is_err());
        assert!(parse(&["--supervisor", "--plan", "x.json"]).is_err());
        assert!(parse(&["--supervisor", "--elevated", "--plan", "x.json"]).is_err(), "a plan without its digest is unbound");
        let sha = "a".repeat(64);
        let child = parse(&["--supervisor", "--elevated", "--plan", "x.json", "--plan-sha", &sha]).unwrap(); // unwrap: the valid case — an Err here IS the failure
        assert!(child.elevated && child.plan.as_deref() == Some(Path::new("x.json")) && child.plan_sha.as_deref() == Some(sha.as_str()));
        assert!(parse(&["--supervisor", "--elevated", "--plan", "x.json", "--plan-sha", "not-hex"]).is_err());
        assert!(parse(&["--supervisor", "--supervisor"]).is_err());
        assert!(parse(&["--supervisor", "--check"]).unwrap().check); // unwrap: the valid case — an Err here IS the failure
        assert!(parse(&["--supervisor", "--check", "--elevated", "--plan", "x.json"]).is_err(), "check reads, elevated writes");
    }
}
