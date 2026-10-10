//! The installed Windows release receipt, shared by supervisor readers and launch.
//! Prepared releases are not active releases. A schema-2 supervisor never falls back
//! to a legacy descriptor when its active receipt is missing or corrupt.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

pub const MAX_RECEIPT_BYTES: u64 = 65536;
pub const SUPERVISOR_PROTOCOL_VERSION: u32 = 3;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub schema: u32,
    pub active_release: String,
    pub bootstrap: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub artifact: String,
    pub socket: String,
    pub launcher: String,
    pub cli: String,
    pub engine: String,
    pub log_directory: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eye_root: Option<String>,
}

impl Release {
    /// Installed binaries run beside their DLLs, not inside the installer checkout.
    /// Handoff and recovery must use the root registered with this release, never
    /// a build-time path or an unrelated tracked main checkout.
    pub fn installer_root(&self) -> Result<PathBuf, String> {
        let root = PathBuf::from(
            self.eye_root
                .as_deref()
                .ok_or("installed release has no installer root")?,
        );
        if !root.is_absolute() {
            return Err("installed installer root must be absolute".into());
        }
        for module in [
            "install-common.ps1",
            "windows-service.ps1",
            "windows-prepared.ps1",
            "win-modules.ps1",
        ] {
            if !root.join("tools/scripts/lib").join(module).is_file() {
                return Err(format!(
                    "installed installer root {} lacks {module}",
                    root.display()
                ));
            }
        }
        Ok(root)
    }
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: u32,
    user_sid: String,
    release: Release,
    hashes: BTreeMap<String, String>,
}

pub fn envelope(description: &str) -> Result<Option<Envelope>, String> {
    let value: serde_json::Value = serde_json::from_str(description)
        .map_err(|e| format!("invalid supervisor descriptor: {e}"))?;
    if value.get("schema").is_none() {
        return Ok(None);
    }
    let envelope: Envelope =
        serde_json::from_value(value).map_err(|e| format!("invalid supervisor envelope: {e}"))?;
    if envelope.schema != 2 {
        return Err("unsupported supervisor envelope schema".into());
    }
    Ok(Some(envelope))
}

pub fn unredirected(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        match std::fs::symlink_metadata(part) {
            Ok(metadata) => {
                #[cfg(windows)]
                let redirected = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let redirected = metadata.file_type().is_symlink();
                if redirected {
                    return Err(format!("installed path is redirected: {}", part.display()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot inspect {}: {e}", part.display())),
        }
    }
    Ok(())
}
fn exact_path(path: &str, expected: &Path) -> Result<(), String> {
    let path = Path::new(path);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("installed path is not absolute and normalized".into());
    }
    if !path
        .to_string_lossy()
        .replace('/', "\\")
        .eq_ignore_ascii_case(&expected.to_string_lossy().replace('/', "\\"))
    {
        return Err(format!(
            "installed path outside managed layout: {}",
            path.display()
        ));
    }
    unredirected(path)
}
impl Envelope {
    pub fn validate(&self, home: &Path, program_files: &Path, sid: &str) -> Result<(), String> {
        if self.schema != 2 {
            return Err("unsupported supervisor envelope schema".into());
        }
        if !sid.starts_with("S-1-")
            || !sid
                .bytes()
                .all(|c| c.is_ascii_digit() || c == b'-' || c == b'S')
        {
            return Err("invalid Windows user SID".into());
        }
        exact_path(&self.active_release, &home.join("install-active.json"))?;
        let authority = program_files.join("Continuum").join(sid);
        // Existing authorities remain readable for normal protected migration.
        // New generations are immutable siblings keyed by validated CLI bytes.
        let legacy = authority.join("supervisor/continuum.exe");
        if exact_path(&self.bootstrap, &legacy).is_ok() {
            return Ok(());
        }
        let directory = Path::new(&self.bootstrap)
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .ok_or("invalid bootstrap generation path")?;
        let generation = directory
            .strip_prefix("supervisor-")
            .ok_or("invalid bootstrap generation")?;
        if generation.len() != 64
            || !generation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid bootstrap generation identity".into());
        }
        exact_path(
            &self.bootstrap,
            &authority.join(directory).join("continuum.exe"),
        )
    }
    pub fn arguments(&self, mode: &str) -> String {
        format!("installed-service {mode} \"{}\"", self.active_release)
    }
}
fn pinned_file(path: &Path) -> Result<File, String> {
    unredirected(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    options
        .open(path)
        .map_err(|e| format!("cannot pin installed file {}: {e}", path.display()))
}
pub fn read_receipt(path: &Path) -> Result<String, String> {
    let file = pinned_file(path)?;
    let mut text = String::new();
    file.take(MAX_RECEIPT_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read active release: {e}"))?;
    if text.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("active release receipt is oversized".into());
    }
    Ok(text)
}
/// Pins verified payload files against replacement until the caller launches its child.
#[derive(Debug)]
pub struct ValidatedRelease {
    pub release: Release,
    _pins: Vec<File>,
}
/// Cheap observer read: validates schema, layout and declared closure, without
/// repeatedly hashing hundreds of megabytes during each task-state poll.
/// This does not authorize launch: launch always calls validate_receipt and pins files.
pub fn resolve_receipt(
    text: &str,
    home: &Path,
    payload: &Path,
    sid: &str,
) -> Result<Release, String> {
    Ok(validate_receipt_inner(text, home, payload, sid, false)?.release)
}
pub fn validate_receipt(
    text: &str,
    home: &Path,
    payload: &Path,
    sid: &str,
) -> Result<ValidatedRelease, String> {
    validate_receipt_inner(text, home, payload, sid, true)
}
fn validate_receipt_inner(
    text: &str,
    home: &Path,
    payload: &Path,
    sid: &str,
    verify_payload: bool,
) -> Result<ValidatedRelease, String> {
    if text.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("active release receipt is oversized".into());
    }
    let receipt: Receipt =
        serde_json::from_str(text).map_err(|e| format!("invalid active release receipt: {e}"))?;
    if receipt.schema != 1 || receipt.user_sid != sid {
        return Err("active release schema or owner differs".into());
    }
    let r = &receipt.release;
    for value in [
        &r.artifact,
        &r.socket,
        &r.launcher,
        &r.cli,
        &r.engine,
        &r.log_directory,
    ]
    .into_iter()
    .chain(r.eye_root.iter())
    {
        if value.is_empty()
            || value.contains(['"', '\r', '\n', '\0'])
            || value.ends_with('\\')
            || !Path::new(value).is_absolute()
        {
            return Err("active release contains an unsafe path".into());
        }
    }
    let slot = Path::new(&r.artifact)
        .parent()
        .ok_or("missing release slot")?;
    let slot_name = slot
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("missing release slot name")?;
    if !matches!(slot_name, "service-a" | "service-b") {
        return Err("unknown service slot".into());
    }
    let expected = payload.join("bin").join(slot_name);
    for (value, name) in [
        (&r.artifact, "continuum-core-server.exe"),
        (&r.cli, "continuum.exe"),
        (&r.launcher, "run-service-hidden.ps1"),
    ] {
        exact_path(value, &expected.join(name))?;
    }
    let engine_slot = Path::new(&r.engine)
        .parent()
        .and_then(Path::file_name)
        .and_then(|v| v.to_str())
        .ok_or("missing engine slot")?;
    if !matches!(engine_slot, "engine-a" | "engine-b" | "engine-c") {
        return Err("unknown engine slot".into());
    }
    exact_path(
        &r.engine,
        &payload
            .join("bin")
            .join(engine_slot)
            .join("llama-server.exe"),
    )?;
    exact_path(&r.log_directory, &home.join("logs"))?;
    let mut files: BTreeMap<String, PathBuf> = [
        ("artifact", &r.artifact),
        ("cli", &r.cli),
        ("launcher", &r.launcher),
        ("engine", &r.engine),
    ]
    .into_iter()
    .map(|(k, p)| (k.to_owned(), PathBuf::from(p)))
    .collect();
    let manifest = expected.join("runtime-libs.txt");
    let mut pins = Vec::new();
    if manifest.exists() {
        let mut file = pinned_file(&manifest)?;
        let mut names = String::new();
        (&mut file)
            .take(MAX_RECEIPT_BYTES + 1)
            .read_to_string(&mut names)
            .map_err(|e| e.to_string())?;
        if names.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("runtime manifest is oversized".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for name in names.lines().map(str::trim).filter(|n| !n.is_empty()) {
            if !name.to_ascii_lowercase().ends_with(".dll")
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                return Err("invalid runtime DLL leaf name".into());
            }
            if !seen.insert(name.to_ascii_lowercase()) {
                return Err("duplicate runtime DLL name".into());
            }
            files.insert(format!("runtime:{name}"), expected.join(name));
        }
        files.insert("runtime-manifest".into(), manifest);
        pins.push(file);
    }
    if files.len() != receipt.hashes.len() || files.keys().any(|k| !receipt.hashes.contains_key(k))
    {
        return Err("active release hash closure differs".into());
    }
    for (key, path) in files {
        let expected_hash = &receipt.hashes[&key];
        if expected_hash.len() != 64 || !expected_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("invalid active hash for {key}"));
        }
        let mut file = pinned_file(&path)?;
        if verify_payload {
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
            if !format!("{:x}", hasher.finalize()).eq_ignore_ascii_case(expected_hash) {
                return Err(format!("active release {key} changed; refusing launch"));
            }
        }
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        pins.push(file);
    }
    Ok(ValidatedRelease {
        release: receipt.release,
        _pins: pins,
    })
}

/// Integrity is authoritative: Safer NORMALUSER may still report TokenIsElevated.
/// Unknown integrity and a still-privileged marked child are refusals, not fallbacks.
pub fn needs_medium_relaunch(rid: Result<u32, String>, marker: bool) -> Result<bool, String> {
    let rid = rid?;
    if rid <= 0x2000 {
        return Ok(false);
    }
    if marker {
        return Err("service token remains above Medium after relaunch".into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Regression: forged relaunch marker or failed token query must never authorize selection.
    #[test]
    fn token_selection_fails_closed() {
        assert_eq!(needs_medium_relaunch(Ok(0x2000), true), Ok(false));
        assert_eq!(needs_medium_relaunch(Ok(0x3000), false), Ok(true));
        assert!(needs_medium_relaunch(Ok(0x3000), true).is_err());
        assert!(needs_medium_relaunch(Err("unreadable".into()), false).is_err());
        assert_eq!(needs_medium_relaunch(Ok(0x2100), false), Ok(true));
    }
    // Regression: active selection cannot trust unowned, corrupted or incomplete runtime closure.
    #[test]
    fn active_receipt_requires_owned_exact_files_and_hash_closure() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path();
        let slot = home.join("bin/service-a");
        let engine = home.join("bin/engine-a");
        std::fs::create_dir_all(&slot).unwrap();
        std::fs::create_dir_all(&engine).unwrap();
        let r = Release {
            artifact: slot.join("continuum-core-server.exe").display().to_string(),
            cli: slot.join("continuum.exe").display().to_string(),
            launcher: slot.join("run-service-hidden.ps1").display().to_string(),
            engine: engine.join("llama-server.exe").display().to_string(),
            socket: home.join("core.sock").display().to_string(),
            log_directory: home.join("logs").display().to_string(),
            eye_root: None,
        };
        let mut hashes = BTreeMap::new();
        for (key, path) in [
            ("artifact", &r.artifact),
            ("cli", &r.cli),
            ("launcher", &r.launcher),
            ("engine", &r.engine),
        ] {
            std::fs::write(path, b"payload").unwrap();
            hashes.insert(key.to_owned(), format!("{:x}", Sha256::digest(b"payload")));
        }
        let mut receipt =
            serde_json::json!({"schema":1,"userSid":"SID","release":r,"hashes":hashes});
        // Installed slot and installer root differ; never infer the latter from
        // the former or accept a stale checkout missing the recovery owner.
        let mut installed = r.clone();
        assert!(installed.installer_root().is_err());
        installed.eye_root = Some(slot.display().to_string());
        assert!(installed.installer_root().is_err());
        let checkout = home.join("installer checkout");
        let modules = checkout.join("tools/scripts/lib");
        std::fs::create_dir_all(&modules).unwrap();
        for module in [
            "install-common.ps1",
            "windows-service.ps1",
            "windows-prepared.ps1",
            "win-modules.ps1",
        ] {
            std::fs::write(modules.join(module), b"fixture").unwrap();
        }
        installed.eye_root = Some(checkout.display().to_string());
        assert_eq!(installed.installer_root().unwrap(), checkout);
        std::fs::remove_file(modules.join("windows-prepared.ps1")).unwrap();
        assert!(installed.installer_root().is_err());
        let encode = |v: &serde_json::Value| serde_json::to_string(v).unwrap();
        assert!(validate_receipt(&encode(&receipt), home, home, "SID").is_ok());
        assert!(validate_receipt(&encode(&receipt), home, home, "OTHER").is_err());
        std::fs::write(slot.join("runtime-libs.txt"), "one.dll\n").unwrap();
        std::fs::write(slot.join("one.dll"), b"runtime").unwrap();
        assert!(validate_receipt(&encode(&receipt), home, home, "SID").is_err());
        receipt["hashes"]["runtime-manifest"] =
            format!("{:x}", Sha256::digest(b"one.dll\n")).into();
        receipt["hashes"]["runtime:one.dll"] = format!("{:x}", Sha256::digest(b"runtime")).into();
        assert!(validate_receipt(&encode(&receipt), home, home, "SID").is_ok());
        std::fs::write(slot.join("runtime-libs.txt"), "one.dll\nONE.dll\n").unwrap();
        assert!(validate_receipt(&encode(&receipt), home, home, "SID")
            .unwrap_err()
            .contains("duplicate"));
        std::fs::write(slot.join("runtime-libs.txt"), "../one.dll\n").unwrap();
        assert!(validate_receipt(&encode(&receipt), home, home, "SID").is_err());
        assert!(envelope(r#"{"schema":2,"activeRelease":"missing"}"#).is_err());
        assert!(envelope(&encode(&receipt["release"])).unwrap().is_none());
        let program_files = home.join("Program Files");
        let sid = "S-1-5-21-1004";
        for directory in [
            "supervisor".to_owned(),
            format!("supervisor-{}", "a".repeat(64)),
        ] {
            let authority = Envelope {
                schema: 2,
                active_release: home.join("install-active.json").display().to_string(),
                bootstrap: program_files
                    .join("Continuum")
                    .join(sid)
                    .join(directory)
                    .join("continuum.exe")
                    .display()
                    .to_string(),
            };
            authority.validate(home, &program_files, sid).unwrap();
            let mut invalid = authority;
            invalid.bootstrap = program_files
                .join("Continuum")
                .join(sid)
                .join("supervisor-arbitrary/continuum.exe")
                .display()
                .to_string();
            assert!(invalid.validate(home, &program_files, sid).is_err());
        }
    }
}
