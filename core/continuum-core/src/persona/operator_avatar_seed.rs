//! The human's default profile picture — seeded from the OS account picture
//! the first time the operator self-peer comes online, until they change it
//! in the profile page (Joel, 2026-09-05: "use my image as default till I
//! change it myself"). The roster reads `~/.continuum/avatars/<uuid>.png`
//! (positron_presence::scan_avatar_store) — this writes exactly that file for
//! the operator's peer id, once, and never overwrites a chosen picture.
//!
//! Every step is a bounded subprocess with a NAMED outcome
//! ([[every-probe-on-a-boot-or-launch-path-gets-a-bound-and-a-named-outcome]]):
//! `operator.avatar.present` (already chosen/seeded), `operator.avatar.seeded`
//! (source named), `operator.avatar.not_found` (reason named). Off the boot
//! path: spawned, never awaited by the peer's bring-up.

use std::path::PathBuf;
use std::time::Duration;

use uuid::Uuid;

const PROBE_BOUND: Duration = Duration::from_secs(5);

/// Where the roster looks for a member's picture.
pub fn avatar_png_path(peer_id: Uuid) -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("avatars").join(format!("{peer_id}.png")))
}

/// Spawn the seed; returns immediately. Idempotent: an existing picture is
/// left alone (that is the "till I change it myself" half).
pub fn spawn_seed_default_avatar(peer_id: Uuid) {
    tokio::spawn(async move {
        let outcome = seed_default_avatar(peer_id).await;
        match outcome {
            SeedOutcome::Present => crate::probe!(
                class = "operator.avatar.present",
                peer_id = %peer_id,
                "the human already has a profile picture — left as chosen"
            ),
            SeedOutcome::Seeded { source } => crate::probe!(
                class = "operator.avatar.seeded",
                peer_id = %peer_id,
                source = source,
                "the human's default profile picture is their OS account picture"
            ),
            SeedOutcome::NotFound { reason } => crate::probe!(
                class = "operator.avatar.not_found",
                peer_id = %peer_id,
                reason = reason,
                "no default picture for the human — the tile shows the glyph until they choose one"
            ),
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
pub enum SeedOutcome {
    Present,
    Seeded { source: &'static str },
    NotFound { reason: &'static str },
}

pub async fn seed_default_avatar(peer_id: Uuid) -> SeedOutcome {
    let Some(dst) = avatar_png_path(peer_id) else {
        return SeedOutcome::NotFound { reason: "no_home_dir" };
    };
    if dst.exists() {
        return SeedOutcome::Present;
    }
    if !cfg!(target_os = "macos") {
        // Windows account tile / Linux AccountsService icon: not wired yet —
        // an honest NOT_FOUND, never a fabricated face.
        return SeedOutcome::NotFound { reason: "no_os_source_on_this_platform" };
    }
    let Some(user) = std::env::var("USER").ok().filter(|u| !u.trim().is_empty()) else {
        return SeedOutcome::NotFound { reason: "no_os_user" };
    };
    if let Some(parent) = dst.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return SeedOutcome::NotFound { reason: "avatar_store_unwritable" };
        }
    }
    // 1. The custom account picture (dscl JPEGPhoto: hex words of an image file).
    if let Some(bytes) = dscl_read(&user, "JPEGPhoto").await.and_then(|out| parse_dscl_hex(&out)) {
        let tmp = dst.with_extension("account-picture.bin");
        if std::fs::write(&tmp, &bytes).is_ok() && sips_to_png(&tmp, &dst).await {
            let _ = std::fs::remove_file(&tmp);
            return SeedOutcome::Seeded { source: "macos_account_jpegphoto" };
        }
        let _ = std::fs::remove_file(&tmp);
    }
    // 2. The stock picture path (dscl Picture: a file under /Library/User Pictures).
    if let Some(path) = dscl_read(&user, "Picture").await.and_then(|out| parse_dscl_path(&out)) {
        if sips_to_png(&path, &dst).await {
            return SeedOutcome::Seeded { source: "macos_account_picture_file" };
        }
    }
    SeedOutcome::NotFound { reason: "no_macos_account_picture" }
}

async fn dscl_read(user: &str, attr: &str) -> Option<String> {
    let cmd = tokio::process::Command::new("dscl")
        .args([".", "-read", &format!("/Users/{user}"), attr])
        .output();
    let out = tokio::time::timeout(PROBE_BOUND, cmd).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

async fn sips_to_png(src: &std::path::Path, dst: &std::path::Path) -> bool {
    let cmd = tokio::process::Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(src)
        .arg("--out")
        .arg(dst)
        .output();
    matches!(tokio::time::timeout(PROBE_BOUND, cmd).await, Ok(Ok(o)) if o.status.success())
        && dst.exists()
}

/// `dscl . -read /Users/x JPEGPhoto` prints the attribute name, then the
/// image bytes as space-separated hex words (4 bytes each, the last may be
/// short). Anything that is not hex ends the parse honestly (`None`).
pub fn parse_dscl_hex(out: &str) -> Option<Vec<u8>> {
    let body = out.split_once("JPEGPhoto:")?.1;
    let mut bytes = Vec::with_capacity(body.len() / 2);
    for word in body.split_whitespace() {
        if word.len() % 2 != 0 {
            return None;
        }
        for i in (0..word.len()).step_by(2) {
            bytes.push(u8::from_str_radix(&word[i..i + 2], 16).ok()?);
        }
    }
    (!bytes.is_empty()).then_some(bytes)
}

/// `dscl . -read /Users/x Picture` prints `Picture:` then the path on the
/// next line (or on the same line after a space).
pub fn parse_dscl_path(out: &str) -> Option<PathBuf> {
    let rest = out.split_once("Picture:")?.1.trim();
    (!rest.is_empty()).then(|| PathBuf::from(rest.lines().next().unwrap_or("").trim())) // unwrap_or: rest is non-empty so a first line exists; "" only guards the type
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two dscl output shapes we depend on — hex words
    // (4-byte groups, short tail) decode to the exact bytes with the TIFF
    // magic first, a non-hex word refuses the whole image (no half-decoded
    // picture), and the Picture path is read from the line after the key.
    #[test]
    fn dscl_shapes_decode_or_refuse_honestly() {
        let out = "JPEGPhoto:\n 4d4d002a 000ac448 b0b0\n";
        assert_eq!(
            parse_dscl_hex(out).unwrap(),
            vec![0x4d, 0x4d, 0x00, 0x2a, 0x00, 0x0a, 0xc4, 0x48, 0xb0, 0xb0]
        );
        assert_eq!(parse_dscl_hex("JPEGPhoto:\n 4d4d zz\n"), None);
        assert_eq!(parse_dscl_hex("Picture:\n /x.heic\n"), None);
        assert_eq!(
            parse_dscl_path("Picture:\n /Library/User Pictures/Fun/Yin-Yang.heic\n").unwrap(),
            PathBuf::from("/Library/User Pictures/Fun/Yin-Yang.heic")
        );
    }
}
