//! Lossless node-local path encoding shared by durable storage owners.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NativePath {
    platform: String,
    bytes: Vec<u8>,
}

impl NativePath {
    pub fn encode(path: &Path) -> Self {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            Self {
                platform: "windows".into(),
                bytes: path
                    .as_os_str()
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            }
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::ffi::OsStrExt;
            Self {
                platform: "unix".into(),
                bytes: path.as_os_str().as_bytes().to_vec(),
            }
        }
    }
    pub fn decode(&self) -> Result<PathBuf, String> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            if self.platform != "windows" || self.bytes.len() % 2 != 0 {
                return Err("invalid node-local Windows path record".into());
            }
            let units: Vec<u16> = self
                .bytes
                .chunks_exact(2)
                .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
                .collect();
            Ok(std::ffi::OsString::from_wide(&units).into())
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::ffi::OsStringExt;
            if self.platform != "unix" {
                return Err("foreign node path record".into());
            }
            Ok(std::ffi::OsString::from_vec(self.bytes.clone()).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_native_path_roundtrips_without_unicode_replacement() {
        // A registry restart must address the exact writer path, even when it
        // cannot cross a String-only shell or database configuration boundary.
        #[cfg(windows)]
        let native = {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[0x61, 0xd800, 0x62])
        };
        #[cfg(not(windows))]
        let native = {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b'])
        };
        let path = PathBuf::from(native);
        assert!(path.to_str().is_none());
        let persisted = serde_json::to_vec(&NativePath::encode(&path)).unwrap();
        let restored: NativePath = serde_json::from_slice(&persisted).unwrap();
        assert_eq!(restored.decode().unwrap(), path);
        let foreign = NativePath {
            platform: "foreign".into(),
            bytes: vec![],
        };
        assert!(foreign.decode().is_err());
    }
}
