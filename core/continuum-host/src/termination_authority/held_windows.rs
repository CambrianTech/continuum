use std::os::windows::ffi::OsStringExt;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

pub(super) struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: this handle came from OpenProcess and is closed exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

impl Handle {
    pub(super) fn take(pid: crate::process_id::ProcessId) -> std::io::Result<Self> {
        // SAFETY: acquiring a handle does not signal the process. Ownership passes to Drop.
        let handle = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE, 0, pid.unsigned())
        };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self(handle))
    }

    pub(super) fn image(&self) -> std::io::Result<std::path::PathBuf> {
        let mut buffer = [0u16; 32768];
        let mut length = 32768u32;
        // SAFETY: the held query handle is live and buffer has length UTF-16 units.
        if unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let length = usize::try_from(length)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        let image = buffer.get(..length).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "process image exceeds query buffer")
        })?;
        Ok(std::ffi::OsString::from_wide(image).into())
    }

    pub(super) fn terminate(self, exit_code: u32) -> std::io::Result<()> {
        // SAFETY: the held handle was opened for termination, and still names the inspected process.
        if unsafe { TerminateProcess(self.0, exit_code) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_held_process_reports_its_native_image_without_terminating_it() -> std::io::Result<()> {
        let pid = crate::process_id::ProcessId::try_from(std::process::id())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
        let process = super::super::HeldProcess::take(pid)?;
        assert_eq!(process.image()?, std::env::current_exe()?);
        drop(process);
        assert!(matches!(super::super::may_terminate(pid),
            super::super::TerminationAuthority::Granted));
        Ok(())
    }
}
