//! Producer declaration for build inputs which have no runtime consumers.
//!
//! This is not inferred from package names, file extensions or an empty pin set.
//! The producer must invalidate before changing its output contract, publish only
//! after successful generation, and emit the generation in Cargo's output.
//! Removing that captured output before reclamation makes Cargo rerun the
//! producer before linking again. Watching a self-written receipt is forbidden:
//! it makes otherwise unchanged builds perpetually stale.
//! Unknown producers and outputs are never covered by this declaration.
use std::{fs, io, path::Path};
use sha2::{Digest, Sha256};

pub const RECEIPT: &str = "cargo-static-inputs.receipt";
const CONTRACT: &str = "cargo-static-link-inputs-v1\nbuild\nlib\n";
pub const METADATA: &str = "cargo:metadata=continuum_static_inputs=";

/// Only these producer-owned children are certified; siblings remain unknown.
pub const CHILDREN: [&str; 2] = ["build", "lib"];

pub fn invalidate(out: &Path) -> io::Result<()> {
    match fs::remove_file(out.join(RECEIPT)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// The caller proves static linkage and embeds/copies every runtime asset out of
/// these children. This declaration cannot certify arbitrary Cargo build output.
pub fn publish_static_inputs(out: &Path) -> io::Result<String> {
    use std::io::Write;
    let generation = generation(out)?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(out.join(RECEIPT))?;
    file.write_all(CONTRACT.as_bytes())?;
    writeln!(file, "{generation}")?;
    file.sync_all()?;
    Ok(generation)
}

pub fn declared_static_inputs(out: &Path) -> bool {
    // A partial/foreign receipt is unknown, never partial permission. Bound the
    // read by this exact schema instead of allocating an arbitrary file body.
    use std::io::Read;
    let Ok(file) = fs::File::open(out.join(RECEIPT)) else { return false; };
    let expected_len = CONTRACT.len() + 64 + 1;
    let mut bytes = Vec::with_capacity(expected_len + 1);
    if file.take((expected_len + 1) as u64).read_to_end(&mut bytes).is_err()
        || bytes.len() != expected_len || !bytes.starts_with(CONTRACT.as_bytes()) {
        return false;
    }
    let Ok(saved) = std::str::from_utf8(&bytes[CONTRACT.len()..]) else { return false; };
    let saved = saved.trim_end_matches('\n');
    if saved.len() != 64 || !saved.bytes().all(|byte| byte.is_ascii_hexdigit()) { return false; }
    // Cargo replaces this provenance on successful legacy/different producer
    // runs. A failed legacy run may leave it, so exact content custody is also
    // required below; a stale stamp never certifies changed/new runtime assets.
    let Some(parent) = out.parent() else { return false; };
    let Ok(output) = fs::File::open(parent.join("output")) else { return false; };
    use std::io::BufRead;
    let declaration = format!("{METADATA}{saved}");
    if !io::BufReader::new(output).lines().any(|line| line.is_ok_and(|line| line == declaration)) {
        return false;
    }
    generation(out).is_ok_and(|current| current == saved)
}

fn generation(out: &Path) -> io::Result<String> {
    fn visit(path: &Path, root: &Path, hash: &mut Sha256) -> io::Result<()> {
        use std::io::Read;
        let metadata = fs::symlink_metadata(path)?;
        #[cfg(windows)]
        let redirected = { use std::os::windows::fs::MetadataExt; metadata.file_attributes() & 0x400 != 0 };
        #[cfg(not(windows))]
        let redirected = metadata.file_type().is_symlink();
        if redirected || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(io::Error::other("native cache generation has an unowned entry"));
        }
        let relative = path.strip_prefix(root).map_err(io::Error::other)?;
        let name = relative.as_os_str().as_encoded_bytes();
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name);
        hash.update([u8::from(metadata.is_dir())]);
        if metadata.is_dir() {
            let mut children = fs::read_dir(path)?.map(|entry| entry.map(|entry| entry.path())).collect::<io::Result<Vec<_>>>()?;
            children.sort();
            for child in children { visit(&child, root, hash)?; }
        } else {
            hash.update(metadata.len().to_le_bytes());
            let mut file = fs::File::open(path)?;
            let mut buffer = [0u8; 8192];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 { break; }
                hash.update(&buffer[..n]);
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    for child in CHILDREN { visit(&out.join(child), out, &mut hash)?; }
    Ok(format!("{:x}", hash.finalize()))
}
