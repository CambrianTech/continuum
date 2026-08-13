//! `Screenshotter` — the polymorphic capture-target interface (OpenCV-style,
//! modeled on `commands/search/engine.rs`'s algorithm registry).
//!
//! One trait, N targets. The payoff of building diverse outliers (web vs an iOS
//! simulator) is the realization that all three targets reduce to the SAME shape:
//! **locate a CLI binary → spawn it → collect a PNG.** Web drives a headless
//! Chrome, iOS drives `xcrun simctl`, Android drives `adb`. The differences
//! (a URL + viewport for web, a booted device for mobile, stdout-vs-path output)
//! are what each adapter absorbs behind this one interface.
//!
//! Every adapter MUST `availability()`-probe before it captures, and that probe
//! fails LOUD — naming exactly what's missing (Chrome / full Xcode / adb) and how
//! to get it. That is the public-user path [[solve-for-public-users]]
//! [[fallbacks-are-illegal-fail-loud]]: a persona on a fresh machine gets an
//! actionable "install X" message, never a silent no-op or a fake image.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

/// Whether an adapter can actually run on this host right now.
#[derive(Debug, Clone)]
pub enum Availability {
    /// The binary is present and a capture surface exists (a booted sim, a
    /// connected device, a runnable browser).
    Ready,
    /// Can't capture here. The string is persona/operator-actionable — it names
    /// the missing dependency AND the command to fix it.
    Unavailable(String),
}

/// The parsed, target-agnostic capture request an adapter receives. The command
/// layer validates/normalizes raw params into this before dispatch.
#[derive(Debug, Clone)]
pub struct CaptureRequest {
    /// Web only: the URL / local dev-server address to load. Ignored by mobile.
    pub url: Option<String>,
    /// Viewport width in CSS px (web) / requested width hint (mobile ignores).
    pub width: u32,
    /// Viewport height in CSS px (web).
    pub height: u32,
    /// Mobile: device identifier (udid / adb serial). `None` = the single
    /// booted/connected device. Ignored by web.
    pub device: Option<String>,
    /// Absolute path the adapter must write the PNG to.
    pub out_path: PathBuf,
}

/// A capture target. Implementations live in sibling files (`web`, `ios`,
/// `android`). Cheap and stateless — constructed per command call.
#[async_trait]
pub trait Screenshotter: Send + Sync {
    /// The target discriminator the persona passes as `target` ("web"/"ios"/
    /// "android").
    fn target(&self) -> &'static str;

    /// Probe — is this target runnable on this host right now? Cheap (locates the
    /// binary, asks the device list); never captures. `Unavailable` names the fix.
    async fn availability(&self) -> Availability;

    /// Capture to `req.out_path`. Returns `Err` with a named cause on failure —
    /// the caller has already confirmed `availability()` is `Ready`, so an error
    /// here is a genuine capture failure (process crashed, wrote nothing).
    async fn capture(&self, req: &CaptureRequest) -> Result<(), String>;
}

/// The three adapters, constructed fresh. Linear scan to resolve — only three
/// members, so a `HashMap` factory would be ceremony. Order is the catalog order.
pub fn adapters() -> Vec<Box<dyn Screenshotter>> {
    vec![
        Box::new(super::web::WebShot),
        Box::new(super::ios::IosSimShot),
        Box::new(super::android::AndroidEmuShot),
    ]
}

/// Resolve a target name to its adapter. Fails loud listing the valid targets —
/// no silent default to "web" (a fallback would hide a typo and capture the wrong
/// surface).
pub fn resolve(target: &str) -> Result<Box<dyn Screenshotter>, String> {
    adapters()
        .into_iter()
        .find(|a| a.target() == target)
        .ok_or_else(|| {
            let valid: Vec<&str> = adapters().iter().map(|a| a.target()).collect();
            format!(
                "unknown capture target `{target}` — valid targets: {}",
                valid.join(", ")
            )
        })
}

// ── Shared helpers (used by the adapters) ─────────────────────────────────────

/// Locate an executable: an explicit env override first, then absolute candidate
/// paths (macOS app bundles etc.), then a `$PATH` scan of bare names. Returns the
/// first hit. `None` = not installed → the adapter turns that into a named
/// `Unavailable`.
pub(crate) fn find_binary(env_override: Option<&str>, candidates: &[&str]) -> Option<PathBuf> {
    if let Some(var) = env_override {
        if let Ok(val) = std::env::var(var) {
            let p = PathBuf::from(&val);
            if is_executable(&p) {
                return Some(p);
            }
        }
    }
    for cand in candidates {
        // Absolute / relative path candidate.
        if cand.contains('/') {
            let p = PathBuf::from(cand);
            if is_executable(&p) {
                return Some(p);
            }
            continue;
        }
        // Bare name → scan $PATH.
        if let Some(p) = which(cand) {
            return Some(p);
        }
    }
    None
}

/// Minimal `which`: scan `$PATH` for an executable named `name`. Dependency-free.
pub(crate) fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    // A regular file that exists. On unix we additionally want the exec bit, but
    // existence-as-a-file is a sufficient, portable signal here — spawning surfaces
    // a precise error if it somehow isn't runnable.
    p.is_file()
}

/// Read a PNG's pixel dimensions straight from the IHDR chunk — no image crate.
/// PNG = 8-byte signature, then IHDR: len(4) + "IHDR"(4) + width(4 BE) + height(4
/// BE). Returns `None` if the bytes aren't a PNG we can read; dimensions are
/// best-effort metadata, so an unreadable header doesn't fail the capture (the
/// saved file + byte count are the real result).
pub(crate) fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the registry resolves each real target and fails loud
    // (listing valid targets) on an unknown one — never silently defaulting.
    #[test]
    fn resolve_maps_targets_and_rejects_unknown() {
        for t in ["web", "ios", "android"] {
            // `Box<dyn Screenshotter>` isn't Debug, so unwrap via a match rather
            // than `.expect()`.
            match resolve(t) {
                Ok(a) => assert_eq!(a.target(), t),
                Err(e) => panic!("known target `{t}` should resolve: {e}"),
            }
        }
        let Err(err) = resolve("desktop") else {
            panic!("unknown target must fail");
        };
        assert!(err.contains("desktop"), "names the bad target: {err}");
        assert!(
            err.contains("web") && err.contains("ios"),
            "lists valid: {err}"
        );
    }

    // what this catches: PNG IHDR parsing reads big-endian width/height, and
    // non-PNG bytes return None rather than garbage dimensions.
    #[test]
    fn png_dimensions_reads_ihdr_and_rejects_non_png() {
        // Minimal 16x9 PNG header (signature + IHDR len/type/w/h).
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&16u32.to_be_bytes());
        png.extend_from_slice(&9u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((16, 9)));
        assert_eq!(png_dimensions(b"not a png at all really"), None);
        assert_eq!(png_dimensions(&[]), None);
    }

    // what this catches: find_binary honors an env override pointing at a real
    // file, and returns None when nothing resolves (the Unavailable trigger).
    #[test]
    fn find_binary_honors_env_override_then_misses_cleanly() {
        // A file guaranteed to exist and be a regular file.
        std::env::set_var("CONTINUUM_TEST_FAKE_BIN", "/bin/sh");
        assert_eq!(
            find_binary(Some("CONTINUUM_TEST_FAKE_BIN"), &[]),
            Some(PathBuf::from("/bin/sh"))
        );
        std::env::remove_var("CONTINUUM_TEST_FAKE_BIN");
        assert_eq!(
            find_binary(None, &["definitely-not-a-real-binary-xyz-123"]),
            None
        );
    }
}
