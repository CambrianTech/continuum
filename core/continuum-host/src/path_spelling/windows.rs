use std::os::windows::ffi::OsStrExt;

pub(super) fn equal(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> bool {
    let left: Vec<u16> = left.encode_wide().collect();
    let right: Vec<u16> = right.encode_wide().collect();
    // An overlong component cannot name an installed Windows artifact.
    let (Ok(a), Ok(b)) = (i32::try_from(left.len()), i32::try_from(right.len())) else {
        return false;
    };
    // SAFETY: both buffers remain live and their exact lengths are supplied.
    unsafe {
        windows_sys::Win32::Globalization::CompareStringOrdinal(
            left.as_ptr(),
            a,
            right.as_ptr(),
            b,
            1,
        ) == windows_sys::Win32::Globalization::CSTR_EQUAL
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    #[test]
    fn spelling_keeps_native_units_and_windows_case_rules() {
        assert!(super::super::equal(
            Path::new("C:/Users/a/BIN/Core.EXE"),
            Path::new("c:\\users\\a\\bin\\core.exe")
        ));
        assert!(!super::super::equal(
            Path::new("C:/bin/core.exe"),
            Path::new("C:/bin-other/core.exe")
        ));
        let mut units: Vec<u16> = "C:\\bin\\".encode_utf16().collect();
        units.push(0xd800); // A native unpaired surrogate must not become a replacement character.
        let native = PathBuf::from(std::ffi::OsString::from_wide(&units));
        assert!(super::super::equal(&native, &native));
        units.pop();
        units.push(0xfffd);
        assert!(!super::super::equal(
            &native,
            &PathBuf::from(std::ffi::OsString::from_wide(&units))
        ));
    }
}
