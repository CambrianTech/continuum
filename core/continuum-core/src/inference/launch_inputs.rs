//! Private local input pins for a prepared launch, never a model registry.
//! Windows read-sharing pins bytes; other platforms retain ordinary serving but
//! cannot use this evidence as exclusive restoration authority.
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::inference_capability::gguf_keys::{split_count, split_index, split_tensor_count};
use candle_core::quantized::gguf_file::Content;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub(super) struct LocalLaunchInputs {
    files: Vec<InputFile>,
}

#[derive(Clone)]
struct InputFile {
    requested: PathBuf,
    canonical: PathBuf,
    file: Arc<std::fs::File>,
    bytes: u64,
    sha256: String,
    // The loader opens the original names, including HF snapshot symlinks and
    // split siblings. Pin the path mappings as well as their canonical bytes.
    _path_guards: Vec<Arc<std::fs::File>>,
}

impl LocalLaunchInputs {
    /// Bounded optional capture: an unavailable receipt never makes unchanged
    /// ordinary serving fail. The caller retains an explicit restoration refusal.
    pub(super) async fn capture(ggufs: Vec<PathBuf>, other: Vec<PathBuf>) -> Result<Self, String> {
        let (cancel, _guard) = tokio::sync::oneshot::channel::<()>();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                Self::read(&ggufs, &other, &|| {
                    cancel.is_closed() || std::time::Instant::now() >= deadline
                })
            }),
        )
        .await
        .map_err(|_| {
            "local input capture exceeded its 30 s optional preparation budget".to_string()
        })?
        .map_err(|e| format!("local input worker: {e}"))?
    }

    fn read(
        ggufs: &[PathBuf],
        other: &[PathBuf],
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Self, String> {
        Self::read_with_name_budget(ggufs, other, cancelled, 128 * 1024 * 1024)
    }

    fn read_with_name_budget(
        ggufs: &[PathBuf],
        other: &[PathBuf],
        cancelled: &dyn Fn() -> bool,
        mut tensor_names_remaining: usize,
    ) -> Result<Self, String> {
        let mut files = Vec::new();
        // One aggregate budget across all shard groups, not 1024 independent
        // per-header limits. Charge before cloning into the cross-shard set.
        for first in ggufs {
            let (pin, metadata) = InputFile::open_gguf(first, cancelled)?;
            let count = split_count(&metadata)?;
            for name in metadata.tensor_infos.keys() {
                charge_tensor_name(name, &mut tensor_names_remaining)?;
            }
            if count <= 1 {
                files.push(pin);
                continue;
            }
            if count > 1024 || split_index(&metadata)? != 0 {
                return Err("invalid first GGUF split index/count".into());
            }
            let name = first
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("non-UTF8 GGUF split filename")?;
            let suffix = format!("-00001-of-{count:05}.gguf");
            let prefix = name
                .strip_suffix(&suffix)
                .ok_or("GGUF split filename disagrees with metadata")?;
            let mut tensors: BTreeSet<String> = metadata.tensor_infos.keys().cloned().collect();
            let expected = split_tensor_count(&metadata)?;
            files.push(pin);
            for index in 1..count {
                let path =
                    first.with_file_name(format!("{prefix}-{:05}-of-{count:05}.gguf", index + 1));
                let (pin, metadata) = InputFile::open_gguf(&path, cancelled)?;
                if split_index(&metadata)? != index || split_count(&metadata)? != count {
                    return Err("inconsistent GGUF shard index/count".into());
                }
                for name in metadata.tensor_infos.keys() {
                    charge_tensor_name(name, &mut tensor_names_remaining)?;
                    if !tensors.insert(name.clone()) {
                        return Err("duplicate tensor across GGUF shards".into());
                    }
                }
                files.push(pin);
            }
            if tensors.len() as u64 != expected {
                return Err("GGUF shard tensor count disagrees with metadata".into());
            }
        }
        for path in other {
            files.push(InputFile::open(path, cancelled)?.0);
        }
        let result = Self { files };
        result.confirm(cancelled)?;
        Ok(result)
    }

    pub(super) async fn validate(&self) -> Result<(), String> {
        let inputs = self.clone();
        let (cancel, _guard) = tokio::sync::oneshot::channel::<()>();
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::task::spawn_blocking(move || inputs.confirm(&|| cancel.is_closed())),
        )
        .await
        .map_err(|_| "local input identity check timed out".to_string())?
        .map_err(|e| format!("local input identity worker: {e}"))?
    }

    fn confirm(&self, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
        for input in &self.files {
            if cancelled() {
                return Err("local input verification cancelled".into());
            }
            if std::fs::canonicalize(&input.requested).map_err(|e| e.to_string())?
                != input.canonical
            {
                return Err("local launch input alias changed".into());
            }
            let meta = input.file.metadata().map_err(|e| e.to_string())?;
            if meta.len() != input.bytes || input.sha256.len() != 64 {
                return Err("local launch input identity changed".into());
            }
        }
        Ok(())
    }
}

fn charge_tensor_name(name: &str, remaining: &mut usize) -> Result<(), String> {
    let cost = name
        .len()
        .checked_add(128)
        .ok_or("GGUF tensor-name budget overflow")?;
    *remaining = remaining
        .checked_sub(cost)
        .ok_or("GGUF aggregate tensor names exceed launch verification bound")?;
    Ok(())
}

fn pin_mapping(
    path: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<Arc<std::fs::File>>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let mut guards = Vec::new();
        for ancestor in path.ancestors() {
            if cancelled() {
                return Err("local input capture cancelled".into());
            }
            let guard = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x02200000) // OPEN_REPARSE_POINT | BACKUP_SEMANTICS: pin links/directories themselves.
                .open(ancestor)
                .map_err(|e| format!("local input mapping pin: {e}"))?;
            guards.push(Arc::new(guard));
        }
        Ok(guards)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, cancelled);
        Ok(Vec::new())
    }
}

impl InputFile {
    fn open(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<(Self, std::fs::File), String> {
        if cancelled() {
            return Err("local input capture cancelled".into());
        }
        let requested = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path)
        };
        let mut path_guards = pin_mapping(&requested, cancelled)?;
        let canonical = std::fs::canonicalize(&requested)
            .map_err(|e| format!("local input resolution: {e}"))?;
        if canonical != requested {
            // HF aliases can point into a sibling blobs directory. Its ancestry
            // must remain fixed too, not only the snapshot spelling opened by the engine.
            path_guards.extend(pin_mapping(&canonical, cancelled)?);
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(1); // FILE_SHARE_READ: the original bytes cannot be rewritten/deleted.
        }
        let mut file = options
            .open(&canonical)
            .map_err(|e| format!("local input pin: {e}"))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("local input is not a regular file".into());
        }
        let mut digest = Sha256::new();
        let mut buffer = vec![0_u8; 1024 * 1024];
        let mut bytes = 0_u64;
        loop {
            if cancelled() {
                return Err("local input capture cancelled".into());
            }
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            digest.update(&buffer[..n]);
        }
        if bytes != metadata.len() {
            return Err("local input changed during hashing".into());
        }
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let retained = Arc::new(file.try_clone().map_err(|e| e.to_string())?);
        Ok((
            Self {
                requested,
                canonical,
                file: retained,
                bytes,
                sha256: format!("{:x}", digest.finalize()),
                _path_guards: path_guards,
            },
            file,
        ))
    }

    fn open_gguf(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<(Self, Content), String> {
        let (pin, mut file) = Self::open(path, cancelled)?;
        let metadata =
            crate::inference_capability::gguf_keys::read_bounded_header(&mut file, cancelled)?;
        Ok((pin, metadata))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: split filenames/mtime cannot certify missing or replaced
    // bytes, and malformed metadata must be refused before Candle allocates it.
    #[test]
    fn local_input_receipt_binds_complete_shards_and_configuration() {
        use candle_core::quantized::{gguf_file, GgmlDType, QTensor};
        use gguf_file::Value;
        fn shard(path: &Path, index: u16, count: u16, total: i32, tensor: &str) {
            let data = candle_core::Tensor::new(&[1_f32], &candle_core::Device::Cpu).unwrap();
            let data = QTensor::quantize(&data, GgmlDType::F32).unwrap();
            gguf_file::write(
                &mut std::fs::File::create(path).unwrap(),
                &[
                    ("split.count", &Value::U16(count)),
                    ("split.no", &Value::U16(index)),
                    ("split.tensors.count", &Value::I32(total)),
                ],
                &[(tensor, &data)],
            )
            .unwrap();
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let first = root.join("weights-00001-of-00002.gguf");
        let second = root.join("weights-00002-of-00002.gguf");
        let adapter = root.join("adapter.gguf");
        let template = root.join("chat_template.jinja");
        shard(&first, 0, 2, 2, "first.weight");
        std::fs::write(&adapter, b"adapter-a").unwrap();
        std::fs::write(&template, b"template-a").unwrap();
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| false).is_err());
        shard(&second, 0, 2, 2, "second.weight");
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| false).is_err());
        shard(&second, 1, 3, 2, "second.weight");
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| false).is_err());
        shard(&second, 1, 2, 2, "first.weight");
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| false).is_err());
        shard(&second, 1, 2, 2, "second.weight");
        let receipt = LocalLaunchInputs::read(
            &[first.clone()],
            &[adapter.clone(), template.clone()],
            &|| false,
        )
        .unwrap();
        assert_eq!(receipt.files.len(), 4);
        receipt.confirm(&|| false).unwrap();
        // Each group alone fits; the real capture path must not reset its
        // aggregate ownership budget when moving to a second GGUF input.
        assert!(LocalLaunchInputs::read_with_name_budget(
            &[first.clone(), first.clone()],
            &[],
            &|| false,
            400
        )
        .err()
        .unwrap()
        .contains("aggregate tensor names"));
        let original_hash = receipt.files[3].sha256.clone();
        let modified = std::fs::metadata(&template).unwrap().modified().unwrap();
        let held = receipt.clone();
        drop(receipt);
        #[cfg(windows)]
        {
            assert!(std::fs::write(&template, b"template-b").is_err());
            assert!(std::fs::remove_file(&second).is_err());
            assert!(std::fs::rename(&root, root.with_extension("moved")).is_err());
        }
        drop(held);
        std::fs::write(&template, b"template-b").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&template)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let replaced =
            LocalLaunchInputs::read(&[first.clone()], &[adapter, template], &|| false).unwrap();
        assert_ne!(original_hash, replaced.files[3].sha256);
        drop(replaced);
        shard(&first, 0, 2, 3, "first.weight");
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| false).is_err());
        assert!(LocalLaunchInputs::read(&[first.clone()], &[], &|| true).is_err());
        let malformed = root.join("malformed.gguf");
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3_u32.to_le_bytes());
        bytes.extend(u64::MAX.to_le_bytes());
        bytes.extend(0_u64.to_le_bytes());
        std::fs::write(&malformed, bytes).unwrap();
        assert!(
            LocalLaunchInputs::read(&[malformed.clone()], &[], &|| false)
                .err()
                .unwrap()
                .contains("overflow")
        );
        let mut metadata = b"GGUF".to_vec();
        metadata.extend(3_u32.to_le_bytes());
        metadata.extend(0_u64.to_le_bytes());
        metadata.extend(1_u64.to_le_bytes());
        let mut oversized_string = metadata.clone();
        oversized_string.extend(u64::MAX.to_le_bytes());
        std::fs::write(&malformed, oversized_string).unwrap();
        assert!(
            LocalLaunchInputs::read(&[malformed.clone()], &[], &|| false)
                .err()
                .unwrap()
                .contains("string exceeds")
        );
        metadata.extend(1_u64.to_le_bytes());
        metadata.push(b'k');
        metadata.extend(9_u32.to_le_bytes());
        metadata.extend(8_u32.to_le_bytes());
        metadata.extend(u64::MAX.to_le_bytes());
        std::fs::write(&malformed, metadata).unwrap();
        assert!(
            LocalLaunchInputs::read(&[malformed.clone()], &[], &|| false)
                .err()
                .unwrap()
                .contains("array allocation overflow")
        );
        // Refusal/cancellation leaves no detached ownership of the input.
        std::fs::write(&malformed, b"replace after refusal").unwrap();
        let original = root.join("blob-a");
        let alternate = root.join("blob-b");
        let alias = root.join("snapshot-input");
        std::fs::write(&original, b"original").unwrap();
        std::fs::write(&alternate, b"changed!").unwrap();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&original, &alias);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&original, &alias);
        if let Err(error) = &linked {
            // Windows symlink creation needs Developer Mode or privilege. The
            // ordinary mapping/file lock checks above run regardless.
            #[cfg(windows)]
            assert_eq!(
                error.raw_os_error(),
                Some(1314), // ERROR_PRIVILEGE_NOT_HELD; ErrorKind is Uncategorized.
                "unexpected symlink creation failure: {error}"
            );
            #[cfg(unix)]
            panic!("symlink creation failed: {error}");
        } else {
            let receipt = LocalLaunchInputs::read(&[], &[alias.clone()], &|| false).unwrap();
            receipt.confirm(&|| false).unwrap();
            #[cfg(windows)]
            {
                assert!(std::fs::remove_file(&alias).is_err());
                assert!(std::fs::write(&original, b"changed!").is_err());
            }
            #[cfg(unix)]
            {
                std::fs::remove_file(&alias).unwrap();
                std::os::unix::fs::symlink(&alternate, &alias).unwrap();
                assert!(receipt.confirm(&|| false).is_err());
            }
            drop(receipt);
            std::fs::remove_file(&alias).unwrap();
        }
    }
}
