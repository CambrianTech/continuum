/// `pid`'s executable path from the kernel (`proc_pidpath`).
pub(super) fn name_of(pid: u32) -> Option<String> {
    let mut path = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: proc_pidpath writes at most `path.len()` bytes into the buffer it is given.
    let n = unsafe { libc::proc_pidpath(libc::c_int::try_from(pid).ok()?, path.as_mut_ptr().cast(), path.len() as u32) };
    let written = usize::try_from(n).ok().filter(|n| *n > 0)?;
    Some(String::from_utf8_lossy(&path[..written]).into_owned())
}
