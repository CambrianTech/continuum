use super::*;

impl MemoryPressureMonitor {
    /// Start the monitor on its own tokio task.
    ///
    /// Returns a handle for subscribing to pressure changes and registering reporters.
    /// The monitor task runs until the process exits.
    /// Reporters can be added later via `add_reporter()`.
    pub fn start(reporters: Vec<Arc<dyn MemoryReporter>>) -> Arc<Self> {
        let (reporter_tx, reporter_rx) = tokio::sync::mpsc::unbounded_channel();

        let entries: Vec<ReporterEntry> = reporters
            .iter()
            .cloned()
            .map(|r| ReporterEntry {
                reporter: r,
                consecutive_panics: 0,
                disabled: false,
            })
            .collect();

        let monitor = Arc::new(Self {
            channel: DaemonChannel::ungated(PressureSnapshot::default()),
            current_rss: AtomicU64::new(0),
            current_pressure: AtomicU64::new(0),
            reporters: parking_lot::RwLock::new(reporters),
            reporter_tx,
            state: parking_lot::Mutex::new(MemoryTickState {
                sys: sysinfo::System::new(),
                pid: sysinfo::get_current_pid().ok(),
                reporters: entries,
                reporter_rx,
                prev_level: PressureLevel::Normal,
                consecutive_at_level: 0,
                log_counter: 0,
                anomaly_scanned_at: None,
                started: std::time::Instant::now(),
                swap_activity: Default::default(),
            }),
        });

        clog_info!(
            "🧠 MemoryPressureMonitor started (interval={:?}, reporters={})",
            POLL_INTERVAL,
            monitor.reporters.read().len()
        );

        // The shared runner owns the interval + per-tick catch_unwind. We don't
        // hold the returned handle — subscribers reach the channel through us.
        let _ = spawn_daemon(monitor.clone());
        monitor
    }

    /// Get current pressure level — lock-free, callable from any thread.
    /// Updated every 2s by the monitor loop. Subsystems use this to make
    /// graduated decisions (cache sizes, inference concurrency, render quality).
    pub fn current_level() -> PressureLevel {
        PressureLevel::from_u8(CURRENT_PRESSURE_LEVEL.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Dynamically add a reporter after startup.
    /// The reporter will be picked up by the monitor loop on its next poll.
    pub fn add_reporter(&self, reporter: Arc<dyn MemoryReporter>) {
        self.reporters.write().push(reporter.clone());
        let _ = self.reporter_tx.send(reporter);
    }

    /// Subscribe to pressure snapshot changes.
    /// The receiver gets notified whenever a new snapshot is published.
    pub fn subscribe(&self) -> watch::Receiver<PressureSnapshot> {
        self.channel.handle().subscribe()
    }

    /// Get current RSS (lock-free atomic read, any thread).
    pub fn rss_bytes(&self) -> u64 {
        self.current_rss.load(Ordering::Relaxed)
    }

    /// Get current pressure ratio (lock-free, any thread).
    pub fn pressure(&self) -> f64 {
        f64::from_bits(self.current_pressure.load(Ordering::Relaxed))
    }

    /// Get the latest snapshot (cheap clone from watch channel).
    pub fn current(&self) -> PressureSnapshot {
        self.channel.snapshot()
    }

    /// Compute budget snapshot — human-visible dashboard of all memory consumers.
    ///
    /// For each registered reporter:
    /// - Queries its declared budget (priority, min/preferred/max)
    /// - Queries its current usage
    /// - Computes utilization, headroom, and over-budget warnings
    ///
    /// Phase 1: budgets are the reporter's self-declared specs (monitoring only).
    /// Phase 2: budgets will be adjusted by the allocator based on system pressure.
    pub fn budget_snapshot(&self) -> MemoryBudgetSnapshot {
        let pressure_snap = self.current();
        let reporters = self.reporters.read();
        let mut consumers = Vec::with_capacity(reporters.len());
        let mut warnings = Vec::new();

        for reporter in reporters.iter() {
            // Panic-safe: skip reporters that panic
            let budget_result = std::panic::catch_unwind(AssertUnwindSafe(|| reporter.budget()));
            let report_result = std::panic::catch_unwind(AssertUnwindSafe(|| reporter.report()));

            let (budget, report) = match (budget_result, report_result) {
                (Ok(b), Ok(r)) => (b, r),
                _ => continue,
            };

            let used = report.bytes;
            let budget_bytes = budget.preferred_bytes; // Phase 1: use preferred as ceiling
            let utilization = if budget_bytes > 0 {
                used as f64 / budget_bytes as f64
            } else {
                0.0
            };
            let headroom = budget_bytes as i64 - used as i64;

            if used > budget.max_bytes {
                warnings.push(format!(
                    "{}: {}MB used > {}MB max (over budget by {}MB)",
                    budget.name,
                    used / (1024 * 1024),
                    budget.max_bytes / (1024 * 1024),
                    (used - budget.max_bytes) / (1024 * 1024),
                ));
            }

            consumers.push(MemoryBudgetAllocation {
                name: budget.name,
                priority: budget.priority,
                budget_bytes,
                used_bytes: used,
                utilization,
                headroom_bytes: headroom,
                detail: report.detail,
                can_shed: report.can_shed,
            });
        }

        // Sort by priority (highest first), then by utilization (most stressed first)
        consumers.sort_by(|a, b| {
            b.priority.cmp(&a.priority).then(
                b.utilization
                    .partial_cmp(&a.utilization)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        let total_budgeted = consumers.iter().map(|c| c.budget_bytes).sum();
        let total_used = consumers.iter().map(|c| c.used_bytes).sum();

        let now_ms = continuum_base::time::EpochMs::now().millis();

        MemoryBudgetSnapshot {
            level: pressure_snap.level,
            pressure: pressure_snap.pressure,
            total_bytes: pressure_snap.total_bytes,
            available_bytes: pressure_snap.available_bytes,
            consumers,
            total_budgeted_bytes: total_budgeted,
            total_used_bytes: total_used,
            warnings,
            timestamp_ms: now_ms,
        }
    }

    /// One poll cycle — the [`Daemon::tick`] body. Structured in the canonical
    /// daemon shape: a brief lock to ingest + plan (drain new reporters, refresh
    /// system memory + process RSS, compute pressure/level/hysteresis, snapshot
    /// the live reporters), the async reporter fan-out OFF the lock, then a brief
    /// lock to fold the fault counters + build the snapshot. The state lock is
    /// never held across an await; `self.channel.publish` and the shed fan-out
    /// happen last, lock-free.
    pub(super) async fn poll(&self) {
        use sysinfo::ProcessesToUpdate;

        // --- Phase 1: brief lock — ingest + plan. ---
        let (total, available, swap_used, rss, pressure, level, consecutive_at_level, live) = {
            let mut st = self.state.lock();

            // Drain new reporters (added via add_reporter()).
            while let Ok(new_reporter) = st.reporter_rx.try_recv() {
                let name = new_reporter.name();
                st.reporters.push(ReporterEntry {
                    reporter: new_reporter,
                    consecutive_panics: 0,
                    disabled: false,
                });
                clog_info!(
                    "🧠 Memory reporter '{}' registered dynamically (total: {})",
                    name,
                    st.reporters.len()
                );
            }

            // System memory.
            st.sys.refresh_memory();
            let total = st.sys.total_memory();
            // NOT `available_memory()` — it returns 0 on this platform while
            // total/used are correct, which published a permanent zero into the
            // global atomic every consumer budgets against. See `available_from`.
            let available = available_from(&st.sys);
            let used = total.saturating_sub(available);
            let swap_used = st.sys.used_swap();
            let swap_total = st.sys.total_swap();
            let swap_pct = if swap_total > 0 {
                swap_used as f64 / swap_total as f64
            } else {
                0.0
            };

            // Process RSS.
            let rss = if let Some(p) = st.pid {
                st.sys
                    .refresh_processes(ProcessesToUpdate::Some(&[p]), true);
                st.sys.process(p).map(|proc| proc.memory()).unwrap_or(0)
            } else {
                0
            };

            // Pressure — SYSTEM-WIDE (not just our RSS) because other processes
            // matter to the machine's headroom.
            let pressure = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };
            let swap_rate = st.swap_activity.observe(
                std::time::Instant::now(),
                crate::system_resources::swap_activity::read_swap_traffic(),
            );
            let level = PressureLevel::worse(
                PressureLevel::from_pressure(pressure),
                PressureLevel::from_swap_activity(swap_pct, swap_rate),
            );

            // Atomics + cross-module level — lock-free reads from anywhere.
            self.current_rss.store(rss, Ordering::Relaxed);
            self.current_pressure
                .store(pressure.to_bits(), Ordering::Relaxed);
            CURRENT_PRESSURE_LEVEL.store(level.to_u8(), Ordering::Relaxed);
            // Publish the honest free-physical-bytes number too, so a subsystem sizing
            // a large elective allocation can veto against real headroom, not the ratio.
            CURRENT_AVAILABLE_BYTES.store(available, Ordering::Relaxed);

            // Hysteresis.
            if level == st.prev_level {
                st.consecutive_at_level = st.consecutive_at_level.saturating_add(1);
            } else {
                st.consecutive_at_level = 1;
                st.prev_level = level;
            }
            let consecutive_at_level = st.consecutive_at_level;

            // Snapshot the live (non-quarantined) reporters by index so the fold
            // phase can update their fault counters in place. Indices are stable
            // within a tick — only this single task mutates the vec, and ticks
            // never overlap.
            let live: Vec<(usize, Arc<dyn MemoryReporter>)> = st
                .reporters
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.disabled)
                .map(|(i, e)| (i, e.reporter.clone()))
                .collect();

            // THE LEDGER HEARS IT. Until 2026-09-08 this monitor spoke only through the
            // log line below; the probe ledger — the thing every reader and every retro
            // opens — held zero rows while swap was full. A row on every level change,
            // and every 30 polls (60 s) while High or Critical.
            let should_probe = consecutive_at_level == 1
                || (level.to_u8() >= PressureLevel::High.to_u8() && st.log_counter.is_multiple_of(30));
            if should_probe {
                continuum_base::probe!(
                    class = "memory.pressure",
                    level = %level,
                    pressure_pct = (pressure * 100.0) as u64,
                    avail_mb = available / (1024 * 1024),
                    swap_mb = swap_used / (1024 * 1024),
                    swap_pct = (swap_pct * 100.0) as u64,
                    // -1 = no rate (no counters, or the first poll): occupancy decided.
                    swap_rate_kib_s = swap_rate.map_or(-1, |r| (r / 1024.0) as i64),
                    rss_mb = rss / (1024 * 1024),
                    consecutive = consecutive_at_level,
                    "memory pressure as the node reads it — swap is its own axis: a full swap file, or live paging, is pressure even when the page counters read free"
                );
                // NAME THE ANOMALY. fseventsd at 26 GB beside a 22 GB model server is a
                // fault, not background; a reader who only sees "swap full" has to go
                // find it by hand. Only at High/Critical, only once per probe window —
                // the full process refresh is not free.
                // …and only when DUE (card 948c30c2): the first tick of a High episode, then
                // once per ANOMALY_SCAN_EVERY — not every 2 s poll for as long as the
                // pressure lasts. Below High the scan is re-armed for the next episode.
                let since_start = st.started.elapsed();
                if level.to_u8() < PressureLevel::High.to_u8() {
                    st.anomaly_scanned_at = None;
                }
                if anomaly_scan_due(level, st.anomaly_scanned_at, since_start) {
                    st.anomaly_scanned_at = Some(since_start);
                    // `remove_dead_processes = true`: sysinfo keeps an exited process in
                    // its table until told to drop it, so a scan that refreshed with `false`
                    // named pid 48653 (a rustc stopped at ~17:15Z) at 18:05Z with its last
                    // RSS — a sensor reading with no age, and two peers chased it. A dead
                    // process holds nothing; the table must say so before the scan reads it.
                    st.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
                    let floor = total / 4;
                    let mut named = 0u32;
                    for (pid, proc) in st.sys.processes() {
                        let mem = proc.memory();
                        if mem < floor || Some(*pid) == st.pid {
                            continue;
                        }
                        let name = proc.name().to_string_lossy().to_string();
                        if name.contains("llama-server") {
                            continue;
                        }
                        named += 1;
                        continuum_base::probe!(
                            class = "memory.pressure.anomaly",
                            process = %name,
                            pid = pid.as_u32(),
                            rss_gb = mem / (1024 * 1024 * 1024),
                            source = "sysinfo",
                            "a process other than the model server holds over a quarter of physical memory — this is the fault to name, not the lane"
                        );
                        say_anomaly(&name, pid.as_u32(), mem, total);
                    }
                    // sysinfo cannot read root-owned processes' memory on macOS (fseventsd
                    // at 26.6 GB reported 0 on 2026-09-08) — `ps` can. Bounded to 2 s.
                    if named == 0 {
                        let self_pid = st.pid.map(|p| p.as_u32());
                        match crate::system_resources::process_anomaly::residents_above(
                            floor,
                            std::time::Duration::from_secs(2),
                        ) {
                            Ok(rows) => {
                                for p in rows {
                                    if Some(p.pid) == self_pid || p.name.contains("llama-server") {
                                        continue;
                                    }
                                    continuum_base::probe!(
                                        class = "memory.pressure.anomaly",
                                        process = %p.name,
                                        pid = p.pid,
                                        rss_gb = p.rss_bytes / (1024 * 1024 * 1024),
                                        source = "ps",
                                        "a process other than the model server holds over a quarter of physical memory — this is the fault to name, not the lane"
                                    );
                                    say_anomaly(&p.name, p.pid, p.rss_bytes, total);
                                }
                            }
                            // A read that could not complete is said, never an empty list
                            // (card c7ae34b2) — the anomaly is UNKNOWN this window, not absent.
                            Err(why) => continuum_base::probe!(
                                class = "memory.pressure.anomaly_unreadable",
                                why = %why,
                                "the process table could not be read this window — no anomaly claim either way"
                            ),
                        }
                    }
                }
            }

            (
                total,
                available,
                swap_used,
                rss,
                pressure,
                level,
                consecutive_at_level,
                live,
            )
        };

        // --- Phase 2: off-lock report fan-out — each reporter on the blocking
        // pool with a 100 ms budget + panic isolation. `report()` is a sync call
        // that may block, so it runs on `spawn_blocking` (not an inline future),
        // and its panic is caught inside the blocking closure — the right
        // isolation for a sync reporter, distinct from the inline `guarded()`
        // path the hybrid daemon uses. ---
        let mut results: Vec<(usize, ReporterCallOutcome)> = Vec::with_capacity(live.len());
        for (idx, reporter) in live {
            let handle = tokio::task::spawn_blocking(move || {
                std::panic::catch_unwind(AssertUnwindSafe(|| (reporter.report(), reporter.priority())))
            });
            let outcome = match tokio::time::timeout(REPORTER_TIMEOUT, handle).await {
                Ok(Ok(Ok((report, priority)))) => ReporterCallOutcome::Report(report, priority),
                Ok(Ok(Err(_panic))) => ReporterCallOutcome::Panicked,
                Ok(Err(join_err)) => ReporterCallOutcome::JoinError(format!("{join_err:?}")),
                Err(_elapsed) => ReporterCallOutcome::TimedOut,
            };
            results.push((idx, outcome));
        }

        // --- Phase 3: brief lock — fold outcomes into counters, decide the gate,
        // build the snapshot, log, and collect shed targets. ---
        let (snapshot, shed_targets) = {
            let mut st = self.state.lock();

            let mut module_reports = Vec::with_capacity(results.len());
            let mut held_by_priority: Vec<HeldAtPriority> = Vec::new();
            for (idx, outcome) in results {
                let entry = &mut st.reporters[idx];
                let name = entry.reporter.name();
                match outcome {
                    ReporterCallOutcome::Report(report, priority) => {
                        entry.consecutive_panics = 0;
                        // Only what can shed shares an overrun: an unsheddable reporter would claim a
                        // share it never frees (Cormac on #5697).
                        if report.can_shed {
                            match held_by_priority.iter_mut().find(|held| held.priority == priority) {
                                Some(held) => held.bytes += report.bytes,
                                None => held_by_priority.push(HeldAtPriority { priority, bytes: report.bytes }),
                            }
                        }
                        module_reports.push(report);
                    }
                    ReporterCallOutcome::Panicked => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "🧠 MemoryReporter '{}' panicked ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "🧠 MemoryReporter '{}' quarantined after {} panics",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                    ReporterCallOutcome::JoinError(e) => {
                        clog_warn!("🧠 MemoryReporter '{}' spawn_blocking failed: {}", name, e);
                    }
                    ReporterCallOutcome::TimedOut => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "🧠 MemoryReporter '{}' timed out (>100ms) ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "🧠 MemoryReporter '{}' quarantined after {} failures",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                }
            }

            // Emergency brake: memory gate. Close on sustained Critical (3+ polls
            // = 6 s). Lives in the cross-module `MEMORY_GATE_CLOSED` static (not
            // the channel gate), because allocation sites read it through
            // `is_memory_gate_closed`.
            if level == PressureLevel::Critical && consecutive_at_level >= 3 {
                if !is_memory_gate_closed() {
                    close_memory_gate();
                    clog_warn!(
                        "🚨 MEMORY GATE CLOSED — pressure critical for {}+ seconds. \
                         Blocking new allocations.",
                        consecutive_at_level as u64 * POLL_INTERVAL.as_secs()
                    );
                }
            } else if level < PressureLevel::Critical && is_memory_gate_closed() {
                open_memory_gate();
                clog_info!("🧠 Memory gate re-opened — pressure eased to {}", level);
            }

            // Decide whether to notify shedders this tick, and collect the live
            // can-shed reporters (post-quarantine) to fire OFF the lock. First
            // notification after 2 polls at a level (hysteresis); at Critical,
            // shed on every poll once sustained.
            let should_shed = match level {
                PressureLevel::Critical => consecutive_at_level >= 2,
                PressureLevel::High => consecutive_at_level == 2,
                PressureLevel::Warning => consecutive_at_level == 2,
                PressureLevel::Normal => false,
            };
            let shed_targets: Vec<Arc<dyn MemoryReporter>> = if should_shed {
                st.reporters
                    .iter()
                    .filter(|e| !e.disabled && e.reporter.can_shed())
                    .map(|e| e.reporter.clone())
                    .collect()
            } else {
                Vec::new()
            };

            // Build the snapshot.
            let now_ms = continuum_base::time::EpochMs::now().millis();
            let snapshot = PressureSnapshot {
                level,
                pressure,
                normalized_pressure: PressureLevel::normalize(pressure),
                rss_bytes: rss,
                total_bytes: total,
                available_bytes: available,
                swap_used_bytes: swap_used,
                modules: module_reports,
                held_by_priority,
                timestamp_ms: now_ms,
                consecutive_at_level,
            };

            // Periodic logging — every 15 polls (30 s) at Normal, louder higher.
            st.log_counter += 1;
            let should_log = match level {
                PressureLevel::Normal => st.log_counter.is_multiple_of(15),
                PressureLevel::Warning => st.log_counter.is_multiple_of(5),
                PressureLevel::High | PressureLevel::Critical => true,
            };
            if should_log {
                let rss_mb = rss / (1024 * 1024);
                let avail_mb = available / (1024 * 1024);
                let swap_mb = swap_used / (1024 * 1024);
                let module_summary: String = snapshot
                    .modules
                    .iter()
                    .map(|m| format!("{}={}MB", m.name, m.bytes / (1024 * 1024)))
                    .collect::<Vec<_>>()
                    .join(", ");
                clog_info!(
                    "🧠 Memory: RSS={}MB avail={}MB swap={}MB pressure={:.1}% level={} [{}]",
                    rss_mb,
                    avail_mb,
                    swap_mb,
                    pressure * 100.0,
                    level,
                    if module_summary.is_empty() {
                        "no reporters".to_string()
                    } else {
                        module_summary
                    }
                );
            }

            (snapshot, shed_targets)
        };

        // Publish lock-free. The channel overwrites; readers never block us.
        self.channel.publish(snapshot);

        // Shed OFF the lock — a slow shedder must never stall the state lock (and
        // thus the next tick's ingest). Synchronous + panic-isolated, fire-and-
        // forget, exactly as before — just hoisted out from under the lock.
        for reporter in shed_targets {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                reporter.shed_load(level);
            }));
        }
    }
}

#[async_trait]
impl Daemon for MemoryPressureMonitor {
    type Snapshot = PressureSnapshot;

    fn name(&self) -> &'static str {
        "memory-pressure"
    }

    fn cadence(&self) -> Duration {
        POLL_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<PressureSnapshot> {
        &self.channel
    }

    async fn tick(&self) {
        if let Some(fds) = crate::system_resources::fd_gauge::FdReading::take() {
            if fds.is_high() {
                continuum_base::probe!(
                    class = "process.open_fds.high",
                    open = fds.open,
                    soft_limit = fds.soft_limit.unwrap_or(0), // unwrap_or: is_high needs a limit; 0 never reaches here
                    pct = fds.pct().unwrap_or(0), // unwrap_or: same — the gauge has a limit when it is high
                    "the core's descriptor table is past half its limit — the next pipe or socket fails with EBADF; find the leak before the node goes dark"
                );
            } else {
                continuum_base::probe!(
                    class = "process.open_fds",
                    open = fds.open,
                    soft_limit = fds.soft_limit.unwrap_or(0), // unwrap_or: 0 = unlimited or unreadable
                    "open descriptors this tick"
                );
            }
        }
        self.poll().await;
    }
}

// =============================================================================
// PressureBroker integration — MemoryPressureMonitor as a signal-only pool
// =============================================================================
/// Plug `MemoryPressureMonitor` into `PressureBroker` as a `ResourcePool`.
/// **Signal source**, not a holder of evictable bytes —
/// `evict_at_least` returns 0 because the monitor doesn't own any RAM
/// it could free. Concrete RAM consumers (genome cache, recall cache,
/// fixture replay buffers, Bevy GPU residency) register their own
/// `ResourcePool` impls and the broker drives eviction against THOSE.
///
/// Symmetric with [`DiskPressureMonitor`](super::disk_pressure::DiskPressureMonitor):
/// both monitors appear on the broker's surface as `"sys-memory"` and
/// `"disk-root"` tiers. The zero-byte alert is the desired signal:
/// "memory hot AND nobody owns the eviction on this tier."
impl crate::paging::pool::ResourcePool for MemoryPressureMonitor {
    fn tier_name(&self) -> &str {
        "sys-memory"
    }

    fn capacity_bytes(&self) -> u64 {
        self.current().total_bytes
    }

    fn usage_bytes(&self) -> u64 {
        let snap = self.current();
        snap.total_bytes.saturating_sub(snap.available_bytes)
    }

    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        // Signal-only — RAM eviction happens at concrete consumer
        // pools (genome cache, recall cache, etc.); they register
        // their own `ResourcePool` impls. The broker still emits a
        // `PressureAlert` for this tier when pressure crosses the
        // threshold, so operators see "memory hot AND stuck on the
        // global tier" exactly because the broker found nothing to
        // act on at the system level.
        0
    }

    fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
        // Per-consumer detail lives on `PressureSnapshot.modules`
        // (populated by registered `MemoryReporter`s); the broker
        // surface stays a tier-level summary.
        Vec::new()
    }
}
