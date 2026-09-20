//! `AndroidEmuShot` — the third target, prototyped on the same shape as web/iOS
//! but UNVERIFIED on-device (no Android SDK on the dev machine). The capture path
//! is real (`adb exec-out screencap -p` streams a PNG to stdout, which we write to
//! the output file); it just hasn't been run against a live emulator yet. The
//! availability probe IS exercised and fails loud naming the missing `adb`.
//!
//! Kept a full peer of the other two adapters deliberately: it's the outlier that
//! proves the registry holds a not-yet-validated member gracefully, and the day an
//! emulator is present it should "just work" through the identical interface.

use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;

use super::screenshotter::{which, Availability, CaptureRequest, Screenshotter};

/// Env override for the `adb` binary (absolute path).
const ADB_BIN_ENV: &str = "CONTINUUM_ADB_BIN";

pub struct AndroidEmuShot;

impl AndroidEmuShot {
    /// Locate `adb`: explicit override, then the standard SDK platform-tools
    /// under `$ANDROID_HOME` / `$ANDROID_SDK_ROOT`, then `$PATH`.
    fn locate() -> Option<PathBuf> {
        if let Ok(p) = std::env::var(ADB_BIN_ENV) {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
        for sdk_var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
            if let Ok(root) = std::env::var(sdk_var) {
                let p = PathBuf::from(root).join("platform-tools").join("adb");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        which("adb")
    }

    /// Prepend `-s <serial>` when an explicit device is requested.
    fn device_args(req: &CaptureRequest) -> Vec<String> {
        match &req.device {
            Some(serial) => vec!["-s".to_string(), serial.clone()],
            None => vec![],
        }
    }
}

#[async_trait]
impl Screenshotter for AndroidEmuShot {
    fn target(&self) -> &'static str {
        "android"
    }

    async fn availability(&self) -> Availability {
        let Some(adb) = Self::locate() else {
            return Availability::Unavailable(format!(
                "`adb` not found for the `android` target. Install the Android SDK \
                 platform-tools and set $ANDROID_HOME, or point {ADB_BIN_ENV} at the \
                 binary."
            ));
        };
        // A device/emulator must be attached. `adb devices` lists one line per
        // attached device after the header.
        match Command::new(&adb).arg("devices").output().await {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                let has_device = text
                    .lines()
                    .skip(1) // "List of devices attached"
                    .any(|l| l.trim().ends_with("device"));
                if has_device {
                    Availability::Ready
                } else {
                    Availability::Unavailable(
                        "no Android emulator/device attached. Start an emulator \
                         (`emulator -avd <name>`) or connect a device, then check \
                         `adb devices`."
                            .to_string(),
                    )
                }
            }
            _ => Availability::Unavailable(
                "couldn't query devices via `adb devices` — is the adb server running?".to_string(),
            ),
        }
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<(), String> {
        let adb = Self::locate()
            .ok_or_else(|| "adb disappeared between availability check and capture".to_string())?;

        // `adb exec-out screencap -p` writes raw PNG bytes to stdout (no on-device
        // temp file, no pull). We capture stdout and write it ourselves.
        let mut args = Self::device_args(req);
        args.extend(["exec-out", "screencap", "-p"].map(String::from));

        let output = Command::new(&adb)
            .args(&args)
            .output()
            .await
            .map_err(|e| format!("failed to spawn `adb`: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "`adb exec-out screencap` failed: {}",
                stderr.trim()
            ));
        }
        if output.stdout.is_empty() {
            return Err("adb returned no image bytes from screencap".to_string());
        }
        tokio::fs::write(&req.out_path, &output.stdout)
            .await
            .map_err(|e| {
                format!(
                    "failed to write screenshot to {}: {e}",
                    req.out_path.display()
                )
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: identity is "android" and the device-args build `-s
    // <serial>` only when a device is named (the adb selection contract).
    #[test]
    fn device_args_are_empty_by_default_and_select_when_named() {
        let base = CaptureRequest {
            url: None,
            width: 0,
            height: 0,
            device: None,
            out_path: PathBuf::from("/tmp/x.png"),
        };
        assert_eq!(AndroidEmuShot.target(), "android");
        assert!(AndroidEmuShot::device_args(&base).is_empty());
        let explicit = CaptureRequest {
            device: Some("emulator-5554".into()),
            ..base
        };
        assert_eq!(
            AndroidEmuShot::device_args(&explicit),
            vec!["-s".to_string(), "emulator-5554".to_string()]
        );
    }
}
