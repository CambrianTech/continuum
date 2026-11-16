use super::PowerShellError;
use base64::Engine;
use std::os::windows::process::CommandExt;

pub async fn run(
    script: &str,
    deadline: std::time::Duration,
) -> Result<std::process::Output, PowerShellError> {
    let root = std::env::var_os("SystemRoot").ok_or(PowerShellError::MissingSystemRoot)?;
    let shell =
        std::path::PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut command = std::process::Command::new(shell);
    command
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        // Let Windows PowerShell discover its own modules, not inherited pwsh 7 modules.
        .env_remove("PSModulePath")
        .stdin(std::process::Stdio::null())
        .creation_flags(0x0800_0000);
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(deadline, command.output())
        .await
        .map_err(|_| PowerShellError::TimedOut)?
        .map_err(PowerShellError::Spawn)?;
    if !output.status.success() {
        return Err(PowerShellError::Exit {
            status: output.status,
            stderr: output.stderr,
        });
    }
    Ok(output)
}
