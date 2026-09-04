//! `IosSimShot` — outlier N: capture an iOS Simulator via `xcrun simctl io
//! <device> screenshot <path>`. Maximally different from the web adapter (no URL,
//! no viewport, no DOM — a booted virtual device), which is exactly why building
//! it proves the `Screenshotter` interface isn't secretly web-shaped. Yet it
//! lands on the same shape: locate `xcrun` → spawn `simctl` → collect a PNG.
//!
//! `simctl` ships only with the FULL Xcode, not the Command Line Tools — so the
//! availability probe distinguishes "no Xcode at all" from "CLT but no Xcode" from
//! "Xcode but nothing booted", and names the precise fix for each.

use async_trait::async_trait;
use tokio::process::Command;

use super::screenshotter::{find_binary, Availability, CaptureRequest, Screenshotter};

/// `xcrun` is the stable entry point; it resolves `simctl` from the active Xcode.
const XCRUN_CANDIDATES: &[&str] = &["/usr/bin/xcrun", "xcrun"];

pub struct IosSimShot;

impl IosSimShot {
    fn xcrun() -> Option<std::path::PathBuf> {
        find_binary(None, XCRUN_CANDIDATES)
    }

    /// The device selector simctl understands: an explicit udid, or "booted" for
    /// the single booted simulator.
    fn device_arg(req: &CaptureRequest) -> String {
        req.device.clone().unwrap_or_else(|| "booted".to_string())
    }
}

#[async_trait]
impl Screenshotter for IosSimShot {
    fn target(&self) -> &'static str {
        "ios"
    }

    async fn availability(&self) -> Availability {
        let Some(xcrun) = Self::xcrun() else {
            return Availability::Unavailable(
                "`xcrun` not found — install Xcode (the full app, not just the Command \
                 Line Tools) from the App Store to capture the iOS Simulator."
                    .to_string(),
            );
        };

        // `simctl` exists only under full Xcode. `xcrun --find simctl` succeeds
        // there and fails under Command-Line-Tools-only.
        let found = Command::new(&xcrun)
            .arg("--find")
            .arg("simctl")
            .output()
            .await;
        match found {
            Ok(o) if o.status.success() => {}
            _ => {
                return Availability::Unavailable(
                    "`simctl` is unavailable — you have the Command Line Tools but not the \
                     full Xcode. Install Xcode, then run \
                     `sudo xcode-select -s /Applications/Xcode.app` and `xcrun simctl list`."
                        .to_string(),
                )
            }
        }

        // A booted simulator must exist to capture from.
        let booted = Command::new(&xcrun)
            .args(["simctl", "list", "devices", "booted"])
            .output()
            .await;
        match booted {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                if text.contains("(Booted)") {
                    Availability::Ready
                } else {
                    Availability::Unavailable(
                        "no booted iOS Simulator. Open Simulator.app, or boot one with \
                         `xcrun simctl boot <udid>` (list them via \
                         `xcrun simctl list devices available`)."
                            .to_string(),
                    )
                }
            }
            _ => Availability::Unavailable(
                "couldn't query booted simulators via `xcrun simctl list devices booted`."
                    .to_string(),
            ),
        }
    }

    async fn capture(&self, req: &CaptureRequest) -> Result<(), String> {
        let xcrun = Self::xcrun().ok_or_else(|| {
            "xcrun disappeared between availability check and capture".to_string()
        })?;
        let device = Self::device_arg(req);
        let out = req.out_path.to_string_lossy().to_string();

        let output = Command::new(&xcrun)
            .args(["simctl", "io", &device, "screenshot", &out])
            .output()
            .await
            .map_err(|e| format!("failed to spawn `xcrun simctl`: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "`xcrun simctl io {device} screenshot` failed: {}",
                stderr.trim()
            ));
        }
        if !req.out_path.is_file() {
            return Err(format!(
                "simctl reported success but wrote no file at {out}"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // what this catches: the adapter identifies as "ios" and the device selector
    // defaults to "booted" when no explicit device is given, but honors an
    // explicit udid (the simctl contract).
    #[test]
    fn device_arg_defaults_to_booted_and_honors_explicit() {
        let base = CaptureRequest {
            url: None,
            width: 0,
            height: 0,
            device: None,
            out_path: PathBuf::from("/tmp/x.png"),
        };
        assert_eq!(IosSimShot::device_arg(&base), "booted");
        let explicit = CaptureRequest {
            device: Some("ABC-123-UDID".into()),
            ..base
        };
        assert_eq!(IosSimShot::device_arg(&explicit), "ABC-123-UDID");
    }

    // what this catches: on a host without full Xcode (this dev machine —
    // Command Line Tools only), availability is Unavailable and the message names
    // the Xcode fix. On a host WITH a booted sim it'd be Ready — accept either,
    // never panic.
    #[tokio::test]
    async fn availability_is_actionable_not_a_panic() {
        assert_eq!(IosSimShot.target(), "ios");
        match IosSimShot.availability().await {
            Availability::Unavailable(msg) => {
                assert!(
                    msg.contains("Xcode") || msg.contains("Simulator") || msg.contains("simctl"),
                    "actionable reason: {msg}"
                );
            }
            Availability::Ready => {}
        }
    }
}
