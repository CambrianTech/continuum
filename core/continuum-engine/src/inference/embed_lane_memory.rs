//! One admission path for the backend's measured host and device allocations.
use crate::resources::{
    LeaseError, LeaseGuard, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
};
use parking_lot::Mutex;
pub const EMBED_LANE_CONSUMER_ID: &str = "embed";
static HOST_FLOOR: Mutex<u64> = Mutex::new(0);
static DEVICE_FLOOR: Mutex<u64> = Mutex::new(0);

fn charges(plan: &llama::ContextMemoryPlan) -> Result<[(ResourceKind, u64); 2], String> {
    let mut host = 0u64;
    let mut device = 0u64;
    for row in plan.allocations() {
        let total = match row.placement {
            llama::MemoryPlacement::Host => &mut host,
            llama::MemoryPlacement::Device => &mut device,
        };
        *total = total
            .checked_add(row.bytes)
            .ok_or("embedding memory plan overflow")?;
    }
    Ok([(ResourceKind::Ram, host), (ResourceKind::Vram, device)])
}

/// No partial grant survives refusal or waits for the other pool. The caller moves
/// the group into its blocking worker, retaining custody through context teardown.
pub(crate) async fn lease(
    plan: &llama::ContextMemoryPlan,
    texts: usize,
) -> Result<Vec<LeaseGuard>, String> {
    let Some(daemon) = ResourceDaemon::global() else {
        return Ok(Vec::new());
    };
    let charges = charges(plan)?;
    for ((pool, bytes), floor) in charges.iter().zip([&HOST_FLOOR, &DEVICE_FLOOR]) {
        if *bytes != 0 {
            let mut floor = floor.lock();
            *floor = (*floor).max(*bytes);
            daemon.reserve(EMBED_LANE_CONSUMER_ID, *pool, *floor);
        }
    }
    let attempt = || {
        acquire_all(charges.iter().filter(|(_, n)| *n != 0), |&(kind, bytes)| {
            daemon.acquire_guarded(&LeaseRequest {
                consumer_id: EMBED_LANE_CONSUMER_ID.into(),
                kind,
                bytes,
                // Process-death backstop; live custody is held by the worker's RAII guards.
                ttl_ms: 60_000,
                reclaim_policy: ReclaimPolicy::Pinned,
            })
        })
    };
    let guards = match attempt() {
        Ok(guards) => guards,
        Err(LeaseError::InsufficientCapacity { .. }) => {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            attempt().map_err(|e| format!("embed: measured allocation refused: {e:?}"))?
        }
        Err(e) => return Err(format!("embed: measured allocation refused: {e:?}")),
    };
    continuum_base::probe!(
        class = "embed.memory.leased",
        host_bytes = charges[0].1,
        device_bytes = charges[1].1,
        texts = texts,
        "embedding holds its measured allocation plan"
    );
    Ok(guards)
}

fn acquire_all<I, G, E>(
    requests: impl IntoIterator<Item = I>,
    mut acquire: impl FnMut(I) -> Result<G, E>,
) -> Result<Vec<G>, E> {
    let mut guards = Vec::new();
    for request in requests {
        guards.push(acquire(request)?);
    }
    Ok(guards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    #[test]
    fn second_pool_refusal_releases_first_pool_before_retry() {
        struct Guard(Arc<AtomicU64>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let held = Arc::new(AtomicU64::new(0));
        let result = acquire_all([false, true], |refuse| {
            if refuse {
                return Err("device full");
            }
            held.fetch_add(1, Ordering::SeqCst);
            Ok(Guard(held.clone()))
        });
        assert!(result.is_err());
        assert_eq!(held.load(Ordering::SeqCst), 0);
    }
}
