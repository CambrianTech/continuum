#![cfg(test)]
    use super::*;

    // what this catches: a FOURTH reader of `sysinfo::available_memory()`, which returns
    // 0 on macOS while `total`/`used` are correct. `available_from` above exists to be the
    // one derivation every reader shares, and its doc says so — but saying so in a doc did
    // not stop three callers from using the broken call anyway:
    //
    //   bin/continuum.rs        the warm-build gate. A permanent 0 against a 12 GiB
    //                           threshold meant the gate could NEVER open on a Mac, so
    //                           every deploy stopped the core first. Measured 2026-09-21:
    //                           two deploys, `drain Incomplete { in_flight: 7 }` then
    //                           `{ in_flight: 9 }` — sixteen citizen turns cut — while
    //                           `memory.pressure` reported avail_mb 9,009 and 8,324 at the
    //                           same moments, because IT used `available_from`.
    //   inference/backends/llamacpp.rs   KV-cache sizing at model load. Its own doc says a
    //                           0 "floors the KV budget to MIN_CTX"; on the unified-memory
    //                           Macs that comment is about, the floor was unconditional.
    //   capacity/system_profile.rs       `DeviceCapacity::system_ram_free_bytes`, a
    //                           permanent zero for anyone budgeting against it.
    //
    // A doc comment is not a guard. This is.
    #[test]
    fn nothing_reads_sysinfo_available_memory_directly() {
        fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    scan(&path, hits);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let Ok(text) = std::fs::read_to_string(&path) else { continue };
                    // THE NEEDLE IS BUILT FROM PARTS so this file does not contain it
                    // literally. A scanner that spells its own target matches ITSELF —
                    // the same shape as a `pgrep -f` wait loop matching its own command
                    // line — and would fail here forever with one hit that is the guard.
                    let needle = concat!(".", "available_memory()");
                    for (i, line) in text.lines().enumerate() {
                        if line.contains(needle) && !line.trim_start().starts_with("//") {
                            hits.push(format!("{}:{}", path.display(), i + 1));
                        }
                    }
                }
            }
        }
        let mut hits = Vec::new();
        scan(&source_hygiene::crate_src_root(), &mut hits);
        assert!(
            hits.is_empty(),
            "`sysinfo::available_memory()` returns 0 on macOS — use \
             `system_resources::memory_pressure::available_from(&sys)`, the one derivation \
             every reader shares. Direct callers found at: {hits:?}"
        );
    }

    // what this catches: the org-room line for a memory anomaly firing every 30 s scan
    // for as long as fseventsd stays huge (a flood), or never firing again for a process
    // that stays over the floor for hours (silence). First crossing: said at once. Same
    // process: reminded once per ANOMALY_SAY_EVERY, never more often.
    #[test]
    fn a_memory_anomaly_is_said_once_then_reminded_not_flooded() {
        let t0 = std::time::Instant::now();
        assert!(anomaly_line_due(None, t0), "the first crossing is said at once");
        assert!(!anomaly_line_due(Some(t0), t0 + Duration::from_secs(30)), "the next 30 s scan is not a new line");
        assert!(!anomaly_line_due(Some(t0), t0 + ANOMALY_SAY_EVERY - Duration::from_secs(1)), "still inside the reminder window");
        assert!(anomaly_line_due(Some(t0), t0 + ANOMALY_SAY_EVERY), "a persisting anomaly is reminded");
    }

    // what this catches (card 948c30c2): the anomaly scan — a full process-table refresh
    // and, on macOS, a `ps` child on its own thread — ran on EVERY 2 s poll for as long
    // as pressure stayed High. It is due on the first tick of an episode, then once per
    // ANOMALY_SCAN_EVERY, never below High, and a drop below High re-arms the immediate
    // read for the next episode (the caller clears `last_scan`; the rule refuses below High).
    #[test]
    fn the_anomaly_scan_runs_once_per_episode_then_on_its_own_cadence() {
        use std::time::Duration as D;
        let high = PressureLevel::High;
        assert!(anomaly_scan_due(high, None, D::from_secs(0)), "first High tick: scan now");
        assert!(!anomaly_scan_due(high, Some(D::from_secs(0)), D::from_secs(2)), "next 2 s poll: not again");
        assert!(!anomaly_scan_due(high, Some(D::from_secs(0)), ANOMALY_SCAN_EVERY - D::from_secs(1)));
        assert!(anomaly_scan_due(high, Some(D::from_secs(0)), ANOMALY_SCAN_EVERY), "the cadence elapsed: scan");
        assert!(anomaly_scan_due(PressureLevel::Critical, None, D::from_secs(9)), "Critical counts as High or above");
        assert!(!anomaly_scan_due(PressureLevel::Normal, None, D::from_secs(9)), "below High: never");
        assert!(!anomaly_scan_due(PressureLevel::Warning, None, D::from_secs(9)), "below High: never");
    }

    #[test]
    fn test_pressure_levels() {
        // Thresholds: Normal < 0.80, Warning 0.80-0.90, High 0.90-0.95, Critical >= 0.95
        assert_eq!(PressureLevel::from_pressure(0.3), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pressure(0.79), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pressure(0.80), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_pressure(0.85), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_pressure(0.90), PressureLevel::High);
        assert_eq!(PressureLevel::from_pressure(0.93), PressureLevel::High);
        assert_eq!(PressureLevel::from_pressure(0.95), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_pressure(0.99), PressureLevel::Critical);

        // Normalized pressure: 0.80 → 0.0, 0.95 → 1.0, linear between
        let eps = 1e-9;
        assert!((PressureLevel::normalize(0.50) - 0.0).abs() < eps); // below floor → clamped 0
        assert!((PressureLevel::normalize(0.80) - 0.0).abs() < eps); // at floor → 0.0
        assert!((PressureLevel::normalize(0.875) - 0.5).abs() < eps); // midpoint → 0.5
        assert!((PressureLevel::normalize(0.95) - 1.0).abs() < eps); // at ceiling → 1.0
        assert!((PressureLevel::normalize(0.99) - 1.0).abs() < eps); // above ceiling → clamped 1
    }

    #[test]
    fn test_memory_priority_ordering() {
        // Higher priority = harder to evict = higher allocation weight
        assert!(
            MemoryPriority::Realtime.allocation_weight()
                > MemoryPriority::Interactive.allocation_weight()
        );
        assert!(
            MemoryPriority::Interactive.allocation_weight()
                > MemoryPriority::Background.allocation_weight()
        );
        assert!(
            MemoryPriority::Background.allocation_weight()
                > MemoryPriority::Batch.allocation_weight()
        );
    }

    #[test]
    fn test_memory_priority_pressure_gates() {
        // Lower priority sheds load at lower pressure
        assert!(MemoryPriority::Batch.pressure_gate() < MemoryPriority::Background.pressure_gate());
        assert!(
            MemoryPriority::Background.pressure_gate()
                < MemoryPriority::Interactive.pressure_gate()
        );
        assert!(
            MemoryPriority::Interactive.pressure_gate() < MemoryPriority::Realtime.pressure_gate()
        );
    }

    #[test]
    fn test_budget_allocation_utilization() {
        let alloc = MemoryBudgetAllocation {
            name: "test".to_string(),
            priority: MemoryPriority::Interactive,
            budget_bytes: 100 * 1024 * 1024, // 100MB
            used_bytes: 75 * 1024 * 1024,    // 75MB
            utilization: 0.75,
            headroom_bytes: 25 * 1024 * 1024, // 25MB
            detail: "test".to_string(),
            can_shed: true,
        };
        assert_eq!(alloc.utilization, 0.75);
        assert!(alloc.headroom_bytes > 0);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────

    #[test]
    fn export_bindings_memory_priority() {
        MemoryPriority::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_spec() {
        MemoryBudgetSpec::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_allocation() {
        MemoryBudgetAllocation::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_snapshot() {
        MemoryBudgetSnapshot::export_all(&ts_rs::Config::default()).unwrap();
    }

    // what this catches: card f26d6568 — inert swap held the M5 at High for hours (55% of
    // a 3 GB swapfile left by a deploy build, 22 GB free+inactive, zero paging). Occupancy
    // with no traffic is Normal; live paging is High at any occupancy; a nearly full file
    // is Critical whatever the rate; and with no rate, occupancy still decides (9/08).
    #[test]
    fn swap_pressure_is_paging_not_pages_left_behind() {
        use crate::system_resources::swap_activity::SWAP_HIGH_BYTES_PER_SEC;
        assert_eq!(PressureLevel::from_swap_activity(0.55, Some(0.0)), PressureLevel::Normal);
        assert_eq!(
            PressureLevel::from_swap_activity(0.05, Some(SWAP_HIGH_BYTES_PER_SEC)),
            PressureLevel::High
        );
        assert_eq!(PressureLevel::from_swap_activity(0.97, Some(0.0)), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_swap_activity(0.55, None), PressureLevel::High);
    }

    // what this catches: swap dropping out of the level again (2026-09-08: swap 40/41 GB,
    // from_pressure NORMAL, decode 1–9 t/s, zero ledger rows) — half the swap file is High,
    // nine tenths is Critical, and the node acts on the worse of the two axes.
    #[test]
    fn a_full_swap_file_is_high_pressure_whatever_the_page_counters_say() {
        assert_eq!(PressureLevel::from_swap(0.98), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_swap(0.55), PressureLevel::High);
        assert_eq!(PressureLevel::from_swap(0.30), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_swap(0.0), PressureLevel::Normal);
        assert_eq!(
            PressureLevel::worse(PressureLevel::from_pressure(0.40), PressureLevel::from_swap(0.98)),
            PressureLevel::Critical
        );
        assert_eq!(
            PressureLevel::worse(PressureLevel::from_pressure(0.96), PressureLevel::from_swap(0.0)),
            PressureLevel::Critical
        );
    }

