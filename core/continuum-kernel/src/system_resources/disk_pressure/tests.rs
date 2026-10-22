#![cfg(test)]
    //! What this catches:
    //!   1. Pressure tier thresholds match the doctrine (80/90/95)
    //!   2. Snapshot Default is Normal at 0.0 pressure
    //!   3. Reporter trait is object-safe (boxes into Arc<dyn>)
    //!
    //! Deliberately NOT testing: the loop body itself, because that
    //! requires sysinfo + a real tokio runtime and the unit-test value
    //! per Joel's doctrine ("less tests with more coverage") is in one
    //! integration test that actually starts the monitor and observes
    //! the watch — a follow-up PR adds that under the test-fixtures
    //! gate once a synthetic mount-point fake exists.
    use super::*;

    // what this catches (2026-09-19, Cormac + a027688a on #4185): the no-match arm
    // said UNKNOWN and handed on (0, 0) → pressure 0.0 → Normal. Now: the home's
    // volume when one holds it; else the HOTTEST mounted volume, named as a fallback;
    // else a typed Unknown — never a zero that reads as room.
    #[test]
    fn a_monitor_that_cannot_see_says_unknown_never_normal() {
        use crate::system_resources::drives::{DriveInfo, DriveRole};
        let drive = |mount: &str, total: u64, avail: u64| DriveInfo {
            mount: std::path::PathBuf::from(mount),
            total_bytes: total,
            available_bytes: avail,
            role: DriveRole::System,
        };
        let drives = [drive("/", 1_000, 900), drive("/Volumes/hot", 1_000, 20)];
        let home_on_root = volume_reading(&drives, std::path::Path::new("/home/x/.continuum"));
        assert_eq!(home_on_root.source, VolumeSource::Home);
        assert_eq!((home_on_root.total, home_on_root.available), (1_000, 900));
        // No volume holds the home (a path no mount prefixes): the hottest volume, named.
        let nowhere = [drive("/Volumes/a", 1_000, 900), drive("/Volumes/hot", 1_000, 20)];
        let fallback = volume_reading(&nowhere, std::path::Path::new("/home/x/.continuum"));
        assert_eq!(fallback.source, VolumeSource::HottestVolume);
        assert_eq!(fallback.available, 20, "the most-pressured volume, not the first");
        assert_eq!(fallback.level_override, None);
        // No volumes at all: typed Unknown, and Unknown is ordered as HOT.
        let none = volume_reading(&[], std::path::Path::new("/home/x/.continuum"));
        assert_eq!(none.source, VolumeSource::None);
        assert_eq!(none.level_override, Some(DiskPressureLevel::Unknown));
        assert!(DiskPressureLevel::Unknown > DiskPressureLevel::Critical);
        assert_eq!(DiskPressureLevel::Unknown.to_string(), "unknown");
    }

    #[test]
    fn pressure_tiers_match_substrate_doctrine() {
        assert_eq!(
            DiskPressureLevel::from_pressure(0.0),
            DiskPressureLevel::Normal
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.79),
            DiskPressureLevel::Normal
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.80),
            DiskPressureLevel::Warning
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.89),
            DiskPressureLevel::Warning
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.90),
            DiskPressureLevel::High
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.94),
            DiskPressureLevel::High
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.95),
            DiskPressureLevel::Critical
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.99),
            DiskPressureLevel::Critical
        );
    }

    #[test]
    fn level_to_u8_round_trips_via_current_level() {
        // Walking the atomic publication path: each variant's u8 maps
        // back to itself via `current_level`. This pins the atomic
        // contract — if someone reorders the variants and breaks the
        // u8 assignment, this test catches it.
        CURRENT_DISK_LEVEL.store(0, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Normal
        );
        CURRENT_DISK_LEVEL.store(1, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Warning
        );
        CURRENT_DISK_LEVEL.store(2, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::High
        );
        CURRENT_DISK_LEVEL.store(3, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Critical
        );
        // Reset for any other test.
        CURRENT_DISK_LEVEL.store(0, Ordering::Relaxed);
    }

    /// What this catches: the gate is forward-flippable AND
    /// idempotent. Idempotency matters because the loop calls
    /// `close_disk_gate` every Critical poll once latched; opening
    /// after pressure eases must not reset other state.
    #[test]
    fn disk_gate_flips_and_resets() {
        // Cleanup pre-state from any prior test.
        open_disk_gate();
        assert!(!is_disk_gate_closed());
        close_disk_gate();
        assert!(is_disk_gate_closed());
        // Idempotent close — same state.
        close_disk_gate();
        assert!(is_disk_gate_closed());
        open_disk_gate();
        assert!(!is_disk_gate_closed());
    }

    /// What this catches: `DiskReporter` is object-safe. Without this,
    /// `Arc<dyn DiskReporter>` doesn't compile and the whole monitor
    /// shape (which depends on the trait object) collapses.
    #[test]
    fn reporter_trait_is_object_safe() {
        struct Stub;
        impl DiskReporter for Stub {
            fn name(&self) -> &'static str {
                "stub"
            }
            fn report(&self) -> DiskPathReport {
                DiskPathReport {
                    name: "stub".to_string(),
                    path: PathBuf::from("/tmp"),
                    bytes: 0,
                    detail: "".to_string(),
                }
            }
        }
        let _: Arc<dyn DiskReporter> = Arc::new(Stub);
    }
