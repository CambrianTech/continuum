use super::*;
use std::num::NonZeroU64;

struct Cache(AtomicU64);

impl MemoryReporter for Cache {
    fn name(&self) -> &'static str {
        "recovery-replay"
    }
    fn budget(&self) -> MemoryBudgetSpec {
        MemoryBudgetSpec {
            name: self.name().to_string(),
            priority: MemoryPriority::Background,
            min_bytes: 0,
            preferred_bytes: 1024 * 1024,
            max_bytes: 1024 * 1024,
        }
    }
    fn report(&self) -> ModuleMemoryReport {
        ModuleMemoryReport {
            name: self.name().to_string(),
            bytes: self.0.load(Ordering::SeqCst),
            detail: String::new(),
            can_shed: self.can_shed(),
        }
    }
    fn can_shed(&self) -> bool {
        self.0.load(Ordering::SeqCst) > 0
    }
    fn shed_load(&self, _: PressureLevel) {
        self.0.store(0, Ordering::SeqCst);
    }
    fn take_again(&self, headroom: NonZeroU64) -> u64 {
        let held = self.0.load(Ordering::SeqCst);
        let claimed = headroom.get().min((1024 * 1024_u64).saturating_sub(held));
        self.0.fetch_add(claimed, Ordering::SeqCst);
        claimed
    }
}

#[test]
fn emptied_cache_recovers_while_quarantined_cache_stays_excluded() {
    let active = Arc::new(Cache(AtomicU64::new(1024 * 1024)));
    let quarantined = Arc::new(Cache(AtomicU64::new(0)));
    let entries = vec![
        ReporterEntry {
            reporter: active.clone(),
            consecutive_panics: 0,
            disabled: false,
        },
        ReporterEntry {
            reporter: quarantined.clone(),
            consecutive_panics: 3,
            disabled: true,
        },
    ];
    // Replay repeated host-pressure / headroom transitions. Recovery must not
    // depend on bytes still being present after the preceding pressure event.
    for _ in 0..1000 {
        assert!(active.can_shed());
        active.shed_load(PressureLevel::Critical);
        assert!(!active.can_shed());
        let targets = recovery_targets(&entries);
        assert_eq!(targets.len(), 1);
        for reporter in targets {
            assert_eq!(reporter.take_again(NonZeroU64::MIN), 1);
            assert_eq!(reporter.report().bytes, 1);
            let room = NonZeroU64::MIN.saturating_add(1024 * 1024 - 2);
            assert_eq!(reporter.take_again(room), room.get());
        }
        assert_eq!(active.report().bytes, 1024 * 1024);
        assert_eq!(quarantined.report().bytes, 0);
    }
}
