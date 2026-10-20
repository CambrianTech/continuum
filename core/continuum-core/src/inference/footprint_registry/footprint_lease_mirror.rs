use super::*;

#[derive(Debug, Clone)]
pub(super) struct FootprintLeaseMirror {
    pub(super) lease: ThroughputLease,
    pub(super) key: FootprintKey,
    pub(super) bytes: u64,
}

/// The registry. DashMap-backed so multiple personas / threads can
/// add+remove concurrently without contention (sharded internally).
pub struct FootprintRegistry {
    pub(super) entries: DashMap<FootprintKey, FootprintEntry>,
    pub(super) lease_mirrors: DashMap<String, FootprintLeaseMirror>,
}

impl FootprintRegistry {
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            lease_mirrors: DashMap::new(),
        }
    }

    /// Record `bytes` of resource for the given key. If the key
    /// already exists, ADDS to the existing count (treating each call
    /// as a delta). For "set authoritative size from backend," use
    /// `report_authoritative` instead.
    pub fn add(&self, key: FootprintKey, bytes: u64) {
        let resource_type = key.resource_type.clone();
        self.entries
            .entry(key)
            .and_modify(|e| {
                e.bytes = e.bytes.saturating_add(bytes);
                e.last_active = SystemTime::now();
            })
            .or_insert_with(|| FootprintEntry::new(bytes, &resource_type));
    }

    /// Remove `bytes` of resource. If the entry's bytes drop to zero
    /// the entry itself is removed (no zero-byte ghost entries).
    pub fn remove(&self, key: &FootprintKey, bytes: u64) {
        let mut should_delete = false;
        if let Some(mut entry) = self.entries.get_mut(key) {
            entry.bytes = entry.bytes.saturating_sub(bytes);
            should_delete = entry.bytes == 0;
        }
        if should_delete {
            self.entries.remove(key);
        }
    }

    /// Touch an entry's last-active timestamp without changing its
    /// bytes. Used by the policy when a slot is accessed to mark it
    /// recently-active for LRU eviction priority.
    pub fn touch(&self, key: &FootprintKey) {
        if let Some(mut entry) = self.entries.get_mut(key) {
            entry.last_active = SystemTime::now();
        }
    }

    /// Backend reports authoritative byte count (overrides our internal
    /// accounting). Sets `backend_reported = true`. Used when
    /// `LlamaCppBackend::seq_bytes()` returns the true GPU-resident
    /// count and we want it to win over whatever our accounting says.
    pub fn report_authoritative(&self, key: FootprintKey, bytes: u64) {
        let resource_type = key.resource_type.clone();
        self.entries
            .entry(key)
            .and_modify(|e| {
                e.bytes = bytes;
                e.last_active = SystemTime::now();
                e.backend_reported = true;
            })
            .or_insert_with(|| {
                let mut e = FootprintEntry::new(bytes, &resource_type);
                e.backend_reported = true;
                e
            });
    }

    /// Total bytes attributed to a persona across all resource types
    /// and residencies. The "how big is Helper right now?" answer.
    pub fn persona_total(&self, persona_id: Uuid) -> u64 {
        self.entries
            .iter()
            .filter(|e| e.key().persona_id == Some(persona_id))
            .map(|e| e.value().bytes)
            .sum()
    }

    /// Bytes broken down by resource type globally. The "where's the
    /// weight?" answer — usually the model weights dominate.
    pub fn by_resource_type(&self) -> HashMap<ResourceType, u64> {
        let mut by_type = HashMap::new();
        for entry in self.entries.iter() {
            *by_type
                .entry(entry.key().resource_type.clone())
                .or_insert(0u64) += entry.value().bytes;
        }
        by_type
    }

    /// Total bytes across the entire registry. Cross-checked against
    /// the GpuMonitor's process_bytes by `sanity_check`.
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.value().bytes).sum()
    }

    /// Cheapest combination of evictable entries that would free at
    /// least `target_bytes`. Greedy approximation — picks entries by
    /// ascending cost-per-byte (spill_micros / bytes), excluding
    /// personas in `exclude_personas` (typically the currently-speaking
    /// persona, which the policy doesn't want to evict).
    ///
    /// Returns `None` if no combination of evictable entries can free
    /// the target — caller surfaces a clear "not enough evictable
    /// memory" error rather than partial eviction.
    pub fn cheapest_eviction_for(
        &self,
        target_bytes: u64,
        exclude_personas: &[Uuid],
    ) -> Option<EvictionPlan> {
        if target_bytes == 0 {
            return Some(EvictionPlan {
                entries: Vec::new(),
                bytes_freed: 0,
                estimated_cost_micros: 0,
            });
        }

        // Collect all evictable candidates with their cost-per-byte.
        let mut candidates: Vec<(FootprintKey, FootprintEntry, f64)> = self
            .entries
            .iter()
            .filter(|e| {
                let key = e.key();
                // Excluded personas: don't evict their slots.
                if let Some(pid) = key.persona_id {
                    if exclude_personas.contains(&pid) {
                        return false;
                    }
                }
                if self.is_key_pinned_by_active_lease(key) {
                    return false;
                }
                // Bytes > 0 (zero-byte entries are useless to evict).
                e.value().bytes > 0
            })
            .map(|e| {
                let entry = e.value().clone();
                let cost_per_byte = if entry.bytes > 0 {
                    entry.spill_cost_micros as f64 / entry.bytes as f64
                } else {
                    f64::INFINITY
                };
                (e.key().clone(), entry, cost_per_byte)
            })
            .collect();

        // Cheapest first.
        candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        let mut plan_entries = Vec::new();
        let mut bytes_freed = 0u64;
        let mut estimated_cost = 0u64;
        for (key, entry, _) in candidates {
            if bytes_freed >= target_bytes {
                break;
            }
            bytes_freed = bytes_freed.saturating_add(entry.bytes);
            estimated_cost = estimated_cost.saturating_add(entry.spill_cost_micros);
            plan_entries.push((key, entry));
        }

        if bytes_freed >= target_bytes {
            Some(EvictionPlan {
                entries: plan_entries,
                bytes_freed,
                estimated_cost_micros: estimated_cost,
            })
        } else {
            None
        }
    }

    pub fn acquire_lease(
        &self,
        lease: ThroughputLease,
        key: FootprintKey,
        bytes: u64,
        now_ms: u64,
    ) -> Result<(), ThroughputLeaseError> {
        if lease.is_expired(now_ms) {
            return Err(ThroughputLeaseError::ExpiredLease {
                lease_id: lease.lease_id,
            });
        }
        let lease_id = lease.lease_id.clone();
        match self.lease_mirrors.entry(lease_id.clone()) {
            Entry::Occupied(_) => Err(ThroughputLeaseError::DuplicateLease { lease_id }),
            Entry::Vacant(slot) => {
                slot.insert(FootprintLeaseMirror {
                    lease,
                    key: key.clone(),
                    bytes,
                });
                self.add(key, bytes);
                Ok(())
            }
        }
    }

    pub fn release_lease(&self, lease_id: &str) -> Result<ThroughputLease, ThroughputLeaseError> {
        let Some((_, mirror)) = self.lease_mirrors.remove(lease_id) else {
            return Err(ThroughputLeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            });
        };
        self.remove(&mirror.key, mirror.bytes);
        Ok(mirror.lease)
    }

    pub fn expire_leases(&self, now_ms: u64) -> Vec<ThroughputLease> {
        let expired_ids: Vec<String> = self
            .lease_mirrors
            .iter()
            .filter(|entry| entry.value().lease.is_expired(now_ms))
            .map(|entry| entry.key().clone())
            .collect();

        expired_ids
            .into_iter()
            .filter_map(|lease_id| self.release_lease(&lease_id).ok())
            .collect()
    }

    pub fn lease_snapshot(&self, now_ms: u64) -> ThroughputLeaseSnapshot {
        let mut active = Vec::new();
        let mut expired = Vec::new();
        let mut cost_by_target_silicon = BTreeMap::new();

        for mirror in self.lease_mirrors.iter() {
            let lease = mirror.value().lease.clone();
            if lease.is_expired(now_ms) {
                expired.push(lease);
            } else {
                *cost_by_target_silicon
                    .entry(lease.target_silicon)
                    .or_insert(0u32) += lease.cost_units;
                active.push(lease);
            }
        }

        ThroughputLeaseSnapshot {
            active,
            expired,
            cost_by_target_silicon,
        }
    }

    pub fn reclaimable_leases(&self, now_ms: u64) -> Vec<ThroughputLease> {
        self.lease_mirrors
            .iter()
            .filter(|entry| entry.value().lease.is_reclaimable(now_ms))
            .map(|entry| entry.value().lease.clone())
            .collect()
    }

    /// Phase 2 of pressure relief — the pressure-driven sibling of
    /// [`expire_leases`](Self::expire_leases). Revoke throughput leases to
    /// free at least `target_bytes`, honoring each lease's revocation
    /// policy gated by `pressure_tier` (expired → Hard → Graceful; an
    /// active `Pinned` lease is never revoked).
    ///
    /// Snapshots the live lease mirrors, picks the least-disruptive plan
    /// via the pure [`select_leases_to_revoke`], then RELEASES each chosen
    /// lease — which removes its mirrored footprint, unpinning those keys
    /// so a subsequent [`cheapest_eviction_for`](Self::cheapest_eviction_for)
    /// pass can reclaim them. Returns the realized [`LeaseRevocationOutcome`].
    ///
    /// Returns `None` when policy cannot satisfy the demand at this tier
    /// (e.g. only active `Pinned` leases remain). The caller MUST treat
    /// `None` as "escalate", never as "free nothing" — a `None` pass
    /// releases nothing. `Some` with an empty `revoked` happens only when
    /// `target_bytes == 0` (a no-op pass).
    ///
    /// The snapshot→release is not atomic against concurrent
    /// acquire/release (`lease_mirrors` is a `DashMap`). A lease chosen but
    /// already gone by release time simply contributes nothing to
    /// `bytes_freed`, which reflects only leases actually released. This is
    /// meant to be driven from one place (the broker's tick), exactly like
    /// `expire_leases` — concurrent revocation passes are not a supported
    /// pattern, keeping the broker the single decision-maker.
    pub fn revoke_leases_for(
        &self,
        target_bytes: u64,
        pressure_tier: PressureTier,
        now_ms: u64,
    ) -> Option<LeaseRevocationOutcome> {
        let mut leases = Vec::with_capacity(self.lease_mirrors.len());
        let mut footprint_bytes_per_lease = HashMap::with_capacity(self.lease_mirrors.len());
        for entry in self.lease_mirrors.iter() {
            let mirror = entry.value();
            footprint_bytes_per_lease.insert(mirror.lease.lease_id.clone(), mirror.bytes);
            leases.push(mirror.lease.clone());
        }

        let plan = select_leases_to_revoke(
            &leases,
            &footprint_bytes_per_lease,
            pressure_tier,
            now_ms,
            target_bytes,
        )?;

        let mut revoked = Vec::with_capacity(plan.len());
        let mut bytes_freed = 0u64;
        for (lease_id, planned_bytes) in plan {
            // release_lease removes the mirror + decrements the footprint
            // entry by exactly `planned_bytes`. If the lease vanished
            // between snapshot and now (concurrent release), it frees 0 for
            // that id and we don't count it — realized bytes reflect reality.
            if self.release_lease(&lease_id).is_ok() {
                bytes_freed = bytes_freed.saturating_add(planned_bytes);
                revoked.push((lease_id, planned_bytes));
            }
        }

        Some(LeaseRevocationOutcome {
            revoked,
            bytes_freed,
        })
    }

    pub(super) fn is_key_pinned_by_active_lease(&self, key: &FootprintKey) -> bool {
        self.lease_mirrors.iter().any(|entry| {
            let mirror = entry.value();
            mirror.key == *key
                && mirror.lease.revocation_policy == ThroughputLeaseRevocationPolicy::Pinned
        })
    }

    /// Cross-check: registry sum vs OS-reported process_bytes from
    /// the monitor. Drift > threshold = something allocates without
    /// reporting (bug to chase). Returns Healthy or Drifted with the
    /// observed values.
    pub fn sanity_check(
        &self,
        monitor: &dyn crate::gpu::GpuMonitor,
        drift_pct_threshold: f32,
    ) -> RegistryHealth {
        let registry_total = self.total_bytes();
        let monitor_total = monitor.process_bytes();
        if monitor_total == 0 {
            // Monitor doesn't report (e.g., CPU fallback under no
            // pressure) — can't compare meaningfully. Treat as healthy.
            return RegistryHealth::Healthy { drift_pct: 0.0 };
        }
        let drift = (registry_total as f64 - monitor_total as f64).abs();
        let drift_pct = (drift / monitor_total as f64 * 100.0) as f32;
        if drift_pct > drift_pct_threshold {
            RegistryHealth::Drifted {
                registry_total,
                monitor_process_bytes: monitor_total,
                drift_pct,
            }
        } else {
            RegistryHealth::Healthy { drift_pct }
        }
    }

    /// Number of distinct entries currently tracked. For diagnostics.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Owned point-in-time view of the registry. Single iteration over
    /// the DashMap aggregates total bytes, by_resource_type, by_persona
    /// in one pass — cheaper than calling each accessor separately when
    /// a caller needs the full picture (logs, telemetry, jtag command).
    ///
    /// The snapshot is a passive copy; mutating it doesn't affect the
    /// live registry. Returned shape is `Serialize` so it can be JSON-
    /// dumped directly into a log line or IPC frame.
    pub fn snapshot(&self) -> RegistrySnapshot {
        let mut total_bytes: u64 = 0;
        let mut entry_count: usize = 0;
        let mut by_resource_type: HashMap<ResourceType, u64> = HashMap::new();
        let mut by_persona: HashMap<Uuid, u64> = HashMap::new();
        for entry in self.entries.iter() {
            let key = entry.key();
            let value = entry.value();
            entry_count += 1;
            total_bytes = total_bytes.saturating_add(value.bytes);
            *by_resource_type
                .entry(key.resource_type.clone())
                .or_insert(0) += value.bytes;
            if let Some(pid) = key.persona_id {
                *by_persona.entry(pid).or_insert(0) += value.bytes;
            }
        }
        RegistrySnapshot {
            total_bytes,
            entry_count,
            by_resource_type,
            by_persona,
        }
    }
}

impl Default for FootprintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) static GLOBAL: OnceLock<FootprintRegistry> = OnceLock::new();

/// The process-wide registry. Lazy-initialized on first call. Safe to
/// invoke from any thread, any phase of startup. Idempotent — every
/// caller gets the same `&'static` reference.
pub fn global() -> &'static FootprintRegistry {
    GLOBAL.get_or_init(FootprintRegistry::new)
}

/// Non-panicking accessor that returns `None` if the global hasn't been
/// touched yet. Useful when the caller wants to assert "no allocations
/// reported" (test isolation) or when the caller is in a phase where
/// initializing the registry would be premature (e.g., crash-safe
/// shutdown handlers).
pub fn try_global() -> Option<&'static FootprintRegistry> {
    GLOBAL.get()
}
