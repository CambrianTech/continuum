pub(super) fn below_the_foreground(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(windows_sys::Win32::System::Threading::IDLE_PRIORITY_CLASS);
}
