//! Immutable gym bytes owned by the existing resolver, not a second evaluator.
//! File flushes support process recovery. Unix also flushes namespace ancestors;
//! Windows std has no directory fsync, so power-loss guarantees are weaker there.
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const PREFIX: &str = "gym:sha256:";

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn blob_path(cache: &Path, reference: &str) -> Result<PathBuf, String> {
    let hash = reference
        .strip_prefix(PREFIX)
        .filter(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or_else(|| format!("invalid content-addressed gym reference '{reference}'"))?;
    Ok(cache
        .join("content")
        .join("sha256")
        .join(format!("{hash}.jsonl")))
}

pub(super) fn resolve(cache: &Path, reference: &str) -> Result<(String, String), String> {
    let path = blob_path(cache, reference)?;
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("read content-addressed gym '{reference}': {e}"))?;
    if format!("{PREFIX}{}", digest(&text)) != reference {
        return Err(format!(
            "content-addressed gym '{reference}' integrity mismatch"
        ));
    }
    // The caller parses this exact verified buffer; never verify then reopen.
    Ok((reference.to_string(), text))
}

pub(super) fn publish(cache: &Path, text: &str) -> Result<String, String> {
    let tasks = super::parse_tasks(text, "held-out gym")?;
    if tasks.is_empty() {
        return Err("held-out gym must contain executable EvalTask rows".into());
    }
    for (i, task) in tasks.iter().enumerate() {
        // A present executable grader takes precedence over descriptive expect.
        // Reject blank selected programs instead of letting a lower-priority field
        // disguise an empty shell/test as a meaningful held-out oracle.
        for (field, program) in [("dod_shell", &task.dod_shell), ("test", &task.test)] {
            if program.as_ref().is_some_and(|v| v.trim().is_empty()) {
                return Err(format!("held-out gym task {} has blank {field}", i + 1));
            }
        }
        // EvalTask defaults fields for legacy compatibility; ShareGPT objects
        // would otherwise deserialize as empty tasks and appear to be a gym.
        let graded = task.silence
            || !task.expect.trim().is_empty()
            || task.test.as_ref().is_some_and(|v| !v.trim().is_empty())
            || task
                .dod_shell
                .as_ref()
                .is_some_and(|v| !v.trim().is_empty())
            || !task.ui_checks.is_empty();
        if task.prompt.trim().is_empty() || !graded {
            return Err(format!(
                "held-out gym task {} requires a prompt and grading condition",
                i + 1
            ));
        }
    }
    let reference = format!("{PREFIX}{}", digest(text));
    let path = blob_path(cache, &reference)?;
    let directory = path.parent().ok_or("gym content directory missing")?;
    fs::create_dir_all(directory).map_err(|e| format!("create gym content directory: {e}"))?;
    let directory =
        fs::canonicalize(directory).map_err(|e| format!("resolve gym directory: {e}"))?;
    sync_chain(&directory)?;
    let pending = directory.join(format!("{}.pending", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|e| format!("create gym publication: {e}"))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| format!("persist gym publication: {e}"))?;
        // A complete synced inode becomes visible atomically, never overwriting.
        match fs::hard_link(&pending, &path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("publish gym content: {e}")),
        }
        sync_chain(&directory)?;
        let (_, stored) = resolve(cache, &reference)?;
        if stored != text {
            return Err("gym content conflicts with existing publication".into());
        }
        Ok(reference)
    })();
    // Only this attempt's private temporary name is removed; never a published blob.
    let cleanup = fs::remove_file(&pending);
    match (result, cleanup) {
        (Ok(reference), Ok(())) => Ok(reference),
        (Ok(_), Err(e)) => Err(format!("finish gym publication: {e}")),
        (Err(e), _) => Err(e),
    }
}

fn sync_chain(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    for directory in path.ancestors() {
        fs::File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("persist gym directory {}: {e}", directory.display()))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
