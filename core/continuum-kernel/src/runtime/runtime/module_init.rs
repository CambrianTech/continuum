//! Module initialization: every module on its own thread, all at once, under one deadline, so no
//! module can wedge the boot (pattern 2 of docs/architecture/DEGRADE-NEVER-REFUSE.md).
//!
//! Why a thread per module and not `tokio::time::timeout` alone: the M5 wedge of 2026-11-07
//! 19:11Z was a parking_lot relock inside the serving reconcile. A synchronous block inside a
//! future cannot be preempted by a tokio timer, so the 60 s per-module timeout this replaced never
//! fired and the boot thread sat until the 300 s socket deadline restarted the node into the same
//! wedge. On its own thread, a blocked `initialize` costs that thread; the watcher's deadline runs
//! on the runtime and fires regardless. The thread enters the runtime with `Handle::block_on`,
//! exactly the context `initialize` had on the boot thread, so a module sees no difference.
//!
//! Concurrency: modules were already initialized in `DashMap` iteration order, which is the
//! registry's per-process random hash order, so no module could rely on another having
//! initialized first. Running them together removes the sum, not an ordering guarantee.

use super::*;
use crate::runtime::module_init_record::{InitRecordStore, SlowestHealthyInit};
use crate::runtime::module_readiness::{DegradedReason, InitDeadline, ModuleReadiness, ModuleState};
use tokio::sync::{mpsc, oneshot};

/// What initialization runs under: the deadline, and the record the boot's slowest healthy init
/// is written to (`None` in a test or a runtime that keeps no record).
pub struct InitPlan {
    pub deadline: InitDeadline,
    pub record: Option<InitRecordStore>,
}

impl InitPlan {
    /// The production plan: the deadline derived from the record, which this boot then rewrites.
    pub fn from_record(record: InitRecordStore) -> Self {
        Self { deadline: record.deadline(), record: Some(record) }
    }

    /// A plan that writes no record.
    pub fn unrecorded(deadline: InitDeadline) -> Self {
        Self { deadline, record: None }
    }
}

/// Every module's first verdict: its initialize returned (Ok or not) or its deadline passed.
#[derive(Debug)]
pub struct InitSettled {
    pub modules: usize,
    /// `module: error` for each initialize that returned an error or panicked.
    pub failed: Vec<String>,
    /// Modules past the deadline, still initializing.
    pub degraded: Vec<&'static str>,
    pub deadline: InitDeadline,
    pub ms: u64,
}

impl InitSettled {
    /// `Err` with the complete set when any module failed or is degraded (never just the first).
    pub fn into_result(self) -> Result<(), String> {
        if self.failed.is_empty() && self.degraded.is_empty() {
            info!("All {} modules initialized in {} ms", self.modules, self.ms);
            return Ok(());
        }
        let summary = format!(
            "{} failed, {} degraded of {} modules — the core serves the rest: failed [{}]; degraded \
             past {} [{}]",
            self.failed.len(),
            self.degraded.len(),
            self.modules,
            self.failed.join("; "),
            self.deadline,
            self.degraded.join(", ")
        );
        error!("⚠ {summary}");
        Err(summary)
    }
}

/// What a module's init thread reports: initialize's result and its wall time.
type Returned = (Result<(), String>, u64);

enum FirstVerdict {
    Ready,
    Failed(String),
    Degraded,
}

impl Runtime {
    /// Initialize every registered module and wait until each has a first verdict. The boot uses
    /// [`Runtime::begin_initialize`] so it can bind before this settles.
    pub async fn initialize(&self) -> Result<(), String> {
        self.begin_initialize(InitPlan::unrecorded(InitDeadline::unmeasured())).await.into_result()
    }

    /// Start every module's initialize, each on its own thread, and return the future of the
    /// settle: every module Ready, Failed or Degraded. Returns at once, so the caller binds and
    /// serves while modules initialize; a command for a module still initializing waits (bounded
    /// by the deadline) and, past it, is refused by type. A degraded module that finishes later
    /// becomes Ready, starts its tick loop and probes `boot.module_late`. Must be called inside the
    /// tokio runtime.
    pub fn begin_initialize(
        &self,
        plan: InitPlan,
    ) -> impl std::future::Future<Output = InitSettled> + Send + 'static {
        let handle = tokio::runtime::Handle::current();
        let ctx = Arc::new(ModuleContext::new(
            self.registry.clone(),
            self.bus.clone(),
            self.compute.clone(),
            handle.clone(),
        ));
        let modules = self.registry.list_modules();
        info!(
            "Initializing {} modules (concurrently; deadline {})...",
            modules.len(),
            plan.deadline
        );
        self.audit_wiring(&modules);

        let readiness = self.registry.readiness();
        let started = std::time::Instant::now();
        let deadline_at = tokio::time::Instant::now() + plan.deadline.duration();
        let slowest: Arc<parking_lot::Mutex<Option<SlowestHealthyInit>>> = Arc::default();
        let (verdict_tx, mut verdict_rx) = mpsc::unbounded_channel::<(&'static str, FirstVerdict)>();
        let mut count = 0usize;
        for name in modules {
            let Some(module) = self.registry.get_by_name(name) else { continue };
            count += 1;
            readiness.set(name, ModuleState::Initializing);
            let watch = InitWatch {
                name,
                module: Arc::clone(&module),
                readiness: Arc::clone(&readiness),
                slowest: Arc::clone(&slowest),
                record: plan.record.clone(),
                deadline: plan.deadline.clone(),
            };
            match spawn_init_thread(name, module, Arc::clone(&ctx), handle.clone()) {
                Ok(returned) => {
                    handle.spawn(watch.run(returned, deadline_at, verdict_tx.clone()));
                }
                Err(e) => {
                    let error = format!("could not start its init thread: {e}");
                    watch.finish(Err(error.clone()), 0, false);
                    let _ = verdict_tx.send((name, FirstVerdict::Failed(error))); // the receiver lives in the future returned below
                }
            }
        }
        drop(verdict_tx);

        let deadline = plan.deadline;
        let record = plan.record;
        async move {
            let mut failed = Vec::new();
            let mut degraded = Vec::new();
            while let Some((name, verdict)) = verdict_rx.recv().await {
                match verdict {
                    FirstVerdict::Ready => {}
                    FirstVerdict::Failed(e) => failed.push(format!("{name}: {e}")),
                    FirstVerdict::Degraded => degraded.push(name),
                }
            }
            let ms = started.elapsed().as_millis() as u64;
            continuum_base::probe!(
                class = "boot.module_init.settled",
                modules = count as u64,
                failed = failed.len() as u64,
                degraded = ?degraded,
                deadline = %deadline,
                ms = ms,
                "every module has a first verdict; degraded modules keep initializing"
            );
            let boot_slowest = slowest.lock().clone();
            if let (Some(record), Some(slowest)) = (record, boot_slowest) {
                save_record(record, slowest);
            }
            InitSettled { modules: count, failed, degraded, deadline, ms }
        }
    }

    /// The boot-time wiring audits: advertised-but-undispatchable commands and the declared
    /// dataflow graph. Named before any client can be told a command exists.
    fn audit_wiring(&self, modules: &[&'static str]) {
        // Dispatch-parity audit (see [`ModuleRegistry::dispatch_orphans`]): every advertised
        // command must be routable NOW. ERROR (not panic) so a drift is loud on the first boot
        // that ships it without bricking the substrate.
        let orphans = self.registry.dispatch_orphans();
        if !orphans.is_empty() {
            tracing::error!(
                probe_class = "registry.dispatch_parity",
                orphans = ?orphans,
                count = orphans.len(),
                "ADVERTISED-BUT-UNDISPATCHABLE commands: these appear in help, \
                 suggestions, and the persona tool offer but cannot route — add each \
                 to its module's commands() vec (the work/list class, #309)"
            );
        }

        // The declared dataflow graph (CBAR gap #2): name every one-sided wire once at boot.
        // Not an error yet: production is being declared module by module, so the unfed count
        // starts high and ratchets down (docs/architecture/CONTINUUM-ENGINE.md § The
        // Declared Dataflow Graph).
        let edges = self.registry.event_edges();
        let graph = super::super::event_graph::orphans(&edges, &[]);
        // Hand the bus what every module declares it emits, so each event name's first publish
        // reports whether anyone declared it (`runtime.event_graph.published`).
        self.bus
            .declare_emissions(edges.iter().flat_map(|e| e.emits.iter().cloned()).collect());
        let dead_subscriptions: Vec<&'static str> = modules
            .iter()
            .filter_map(|name| self.registry.get_by_name(name))
            .filter(|m| !m.config().event_subscriptions.is_empty())
            .map(|m| m.config().name)
            .collect();
        continuum_base::probe!(
            class = "runtime.event_graph",
            modules = edges.len() as u64,
            declared_producers = edges.iter().filter(|e| !e.emits.is_empty()).count() as u64,
            declared_consumers = edges.iter().filter(|e| !e.consumes.is_empty()).count() as u64,
            unfed = graph.unfed.len() as u64,
            unread = graph.unread.len() as u64,
            unfed_list = ?graph.unfed,
            unread_list = ?graph.unread,
            dead_bus_subscriptions = ?dead_subscriptions,
            "the declared dataflow graph: consumers no module declares it feeds, productions \
             nobody reads, and bus subscriptions on the dispatch path with no live caller"
        );
    }
}

/// Run `initialize` then `load_state` (the CBAR pair, now per module: a module loads its state
/// the moment its own initialize returns) on a thread of the module's own.
fn spawn_init_thread(
    name: &'static str,
    module: Arc<dyn ServiceModule>,
    ctx: Arc<ModuleContext>,
    handle: tokio::runtime::Handle,
) -> std::io::Result<oneshot::Receiver<Returned>> {
    let (tx, rx) = oneshot::channel();
    std::thread::Builder::new().name(format!("init:{name}")).spawn(move || {
        let started = std::time::Instant::now();
        let result = handle.block_on(async {
            let result = module.initialize(&ctx).await;
            load_state(name, &module).await;
            result
        });
        let _ = tx.send((result, started.elapsed().as_millis() as u64)); // the watcher is gone only when the process is stopping
    })?;
    Ok(rx)
}

/// `load_state` under the lifecycle's per-phase bound, receipted per module.
async fn load_state(name: &'static str, module: &Arc<dyn ServiceModule>) {
    let t = std::time::Instant::now();
    let outcome = match tokio::time::timeout(SHUTDOWN_PHASE, module.load_state()).await {
        Ok(Ok(())) => "ok",
        Ok(Err(_)) => "error",
        Err(_) => "timeout",
    };
    continuum_base::probe!(
        class = "boot.load_state",
        module = %name,
        outcome = outcome,
        ms = t.elapsed().as_millis() as u64,
        "module state load"
    );
}

fn save_record(record: InitRecordStore, slowest: SlowestHealthyInit) {
    tokio::task::spawn_blocking(move || record.save(&slowest));
}

/// One module's watcher: its deadline, its first verdict, and the late finish of a degraded one.
struct InitWatch {
    name: &'static str,
    module: Arc<dyn ServiceModule>,
    readiness: Arc<ModuleReadiness>,
    slowest: Arc<parking_lot::Mutex<Option<SlowestHealthyInit>>>,
    record: Option<InitRecordStore>,
    deadline: InitDeadline,
}

impl InitWatch {
    async fn run(
        self,
        mut returned: oneshot::Receiver<Returned>,
        deadline_at: tokio::time::Instant,
        verdicts: mpsc::UnboundedSender<(&'static str, FirstVerdict)>,
    ) {
        let name = self.name;
        if let Ok(first) = tokio::time::timeout_at(deadline_at, &mut returned).await {
            let (result, ms) = unpack(first);
            let verdict = match &result {
                Ok(()) => FirstVerdict::Ready,
                Err(e) => FirstVerdict::Failed(e.clone()),
            };
            self.finish(result, ms, false);
            let _ = verdicts.send((name, verdict)); // the settle future may be dropped by a caller that only binds
            return;
        }

        let reason = DegradedReason::InitDeadline { deadline: self.deadline.clone() };
        error!("  ✗ {name}: {reason} — degraded; the core serves the rest and it keeps initializing");
        continuum_base::probe!(
            class = "boot.module_init",
            module = %name,
            outcome = "degraded",
            ms = self.deadline.duration().as_millis() as u64,
            deadline = %self.deadline,
            "module initialize past its deadline: degraded, its commands refused by type until it returns"
        );
        self.readiness.set(name, ModuleState::Degraded { reason });
        let _ = verdicts.send((name, FirstVerdict::Degraded)); // as above
        drop(verdicts);

        let (result, ms) = unpack(returned.await);
        let ready = result.is_ok();
        self.finish(result, ms, true);
        if ready {
            if self.readiness.ticks_open() && self.readiness.claim_tick(name) {
                spawn_tick_loop(Arc::clone(&self.module));
            }
        }
    }

    /// Record a returned initialize: its state, its probe, and the boot's slowest healthy init.
    fn finish(&self, result: Result<(), String>, ms: u64, late: bool) {
        let name = self.name;
        continuum_base::probe!(
            class = if late { "boot.module_late" } else { "boot.module_init" },
            module = %name,
            outcome = if result.is_ok() { "ok" } else { "error" },
            ms = ms,
            "module initialize returned"
        );
        match result {
            Ok(()) => {
                info!("  {name} initialized in {ms} ms{}", if late { " (late: degraded until now)" } else { "" });
                // Only an init that returned within its deadline is a sample. A late one is the
                // wedge class the deadline bounds: folded in, its length would become the record's
                // floor for good and the next boots would never be protected (Cormac on #5842).
                if !late {
                    let mut slowest = self.slowest.lock();
                    if slowest.as_ref().is_none_or(|s| ms > s.ms) {
                        *slowest = Some(SlowestHealthyInit { module: name.to_string(), ms });
                    }
                }
                self.readiness.set(name, ModuleState::Ready { init_ms: ms });
            }
            Err(error) => {
                error!("  ✗ {name} initialization FAILED: {error}");
                self.readiness.set(name, ModuleState::Failed { init_ms: ms, error });
            }
        }
    }
}

/// A dropped sender means the init thread panicked before sending.
fn unpack(received: Result<Returned, oneshot::error::RecvError>) -> Returned {
    received.unwrap_or_else(|_| (Err("initialize panicked on its init thread".to_string()), 0)) // unwrap_or_else: the sender drops only when the thread unwinds; that IS the verdict
}

/// A module whose `initialize` returns at once, or blocks its thread forever on a parking_lot
/// relock: the exact M5 wedge (2026-11-07), a synchronous block no tokio timer can preempt. Its
/// commands (under its `prefix`) answer `{"handled": <command>}`. Gated: production cannot link it.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixtures {
    use crate::runtime::module_context::ModuleContext;
    use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};
    use async_trait::async_trait;
    use std::any::Any;

    pub struct WedgeableInitModule {
        pub name: &'static str,
        pub prefix: &'static [&'static str],
        pub init: InitBehavior,
    }

    /// What the fixture's initialize does.
    #[derive(Clone, Copy)]
    pub enum InitBehavior {
        Returns,
        /// A synchronous relock: blocks its thread forever (the M5 11-07 wedge).
        Wedges,
        /// Blocks its thread this long, then returns Ok: a slow init, late when past the deadline.
        ReturnsAfter(std::time::Duration),
    }

    #[async_trait]
    impl ServiceModule for WedgeableInitModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: self.prefix,
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            match self.init {
                InitBehavior::Returns => {}
                InitBehavior::Wedges => {
                    let lock = parking_lot::Mutex::new(());
                    let _held = lock.lock();
                    let _relock = lock.lock(); // not reentrant: blocks this thread forever
                }
                InitBehavior::ReturnsAfter(d) => std::thread::sleep(d),
            }
            Ok(())
        }
        async fn handle_command(&self, command: &str, _p: serde_json::Value) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({ "handled": command })))
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{InitBehavior, WedgeableInitModule as InitModule};
    use super::*;
    use crate::runtime::command_executor::DispatchOutcome;
    use crate::runtime::CommandExecutor;
    use crate::runtime::ModuleUnavailable;

    // what this catches (Cormac on #5842): a late init folded into the record. The record's floor
    // only rises, so one init that returned 20 minutes late would set every later deadline to 20
    // minutes and the deadline would never protect boot again. A late init is the wedge class: it
    // becomes Ready, but the record keeps only this boot's in-deadline sample.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_init_that_returns_past_its_deadline_never_enters_the_record() {
        let home = tempfile::tempdir().expect("tempdir");
        let record = InitRecordStore::under_home(home.path());
        record.save(&SlowestHealthyInit { module: "seed".into(), ms: 20 }); // deadline 3R = 60 ms
        let record = InitRecordStore::under_home(home.path());
        let before = record.deadline();
        let late_by = Duration::from_millis(400);

        let runtime = Runtime::new();
        runtime.register(Arc::new(InitModule { name: "healthy", prefix: &["healthy/"], init: InitBehavior::Returns }));
        runtime.register(Arc::new(InitModule { name: "slow", prefix: &["slow/"], init: InitBehavior::ReturnsAfter(late_by) }));
        let settled = tokio::time::timeout(Duration::from_secs(10), runtime.begin_initialize(InitPlan::from_record(record)))
            .await
            .expect("the boot settles");
        assert_eq!(settled.degraded, vec!["slow"], "{before} is far under {late_by:?}");

        let readiness = runtime.registry().readiness();
        let until = tokio::time::Instant::now() + Duration::from_secs(5);
        while !matches!(readiness.state("slow"), Some(ModuleState::Ready { .. })) {
            assert!(tokio::time::Instant::now() < until, "the late init returns and becomes Ready");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(300)).await; // any save the late finish made has landed

        let next = InitRecordStore::under_home(home.path()).deadline();
        assert!(next.duration() < late_by, "the next boot's deadline stays a boot's, not the late init's: {next}");
    }

    // what this catches (M5 2026-11-07 19:11Z, card 1a566cd9): one module's initialize that never
    // returns wedging the whole boot. With it registered, initialize must still SETTLE within the
    // deadline; the healthy module is Ready and serves; the wedged one is Degraded{InitDeadline},
    // reported by the readiness table, and its commands are refused BY TYPE on both dispatch
    // paths (the socket route and the executor) instead of hanging; a stateless command (ping's
    // shape: no owning module) answers; and the stop broadcast skips it as NeverInitialized.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_module_whose_initialize_never_returns_degrades_and_the_rest_serve() {
        let runtime = Runtime::new();
        runtime.register(Arc::new(InitModule { name: "healthy", prefix: &["healthy/"], init: InitBehavior::Returns }));
        runtime.register(Arc::new(InitModule { name: "wedged", prefix: &["wedged/"], init: InitBehavior::Wedges }));

        let deadline = InitDeadline::fixture(100, "healthy");
        let settled = tokio::time::timeout(
            Duration::from_secs(10),
            runtime.begin_initialize(InitPlan::unrecorded(deadline.clone())),
        )
        .await
        .expect("the boot settles even though one initialize never returns");
        assert_eq!(settled.degraded, vec!["wedged"]);
        assert!(settled.failed.is_empty());

        let readiness = runtime.registry().readiness();
        assert!(matches!(readiness.state("healthy"), Some(ModuleState::Ready { .. })));
        assert_eq!(
            readiness.state("wedged"),
            Some(ModuleState::Degraded { reason: DegradedReason::InitDeadline { deadline: deadline.clone() } })
        );

        // The socket route: the healthy module serves, the degraded one refuses at once.
        let ok = runtime.route_command("healthy/x", serde_json::json!({}), None).await;
        assert!(matches!(ok, Some(Ok(_))), "the healthy module serves: {ok:?}");
        let refused = tokio::time::timeout(
            Duration::from_secs(1),
            runtime.route_command("wedged/x", serde_json::json!({}), None),
        )
        .await
        .expect("a degraded module's command never hangs");
        let message = match refused {
            Some(Err(message)) => message,
            other => panic!("expected a refusal, got {other:?}"),
        };
        assert!(message.contains("'wedged' is degraded") && message.contains("health-check"), "{message}");

        // The executor: the same refusal, typed, and NotCommitted (nothing ran).
        let executor = CommandExecutor::new(runtime.registry_arc());
        let failure = executor
            .execute_classified("wedged/x", serde_json::json!({}), None)
            .await
            .expect_err("refused");
        assert_eq!(failure.outcome, DispatchOutcome::NotCommitted);
        assert_eq!(
            failure.unavailable,
            Some(ModuleUnavailable::Degraded {
                module: "wedged".into(),
                reason: DegradedReason::InitDeadline { deadline },
            })
        );
        assert!(executor.execute_classified("healthy/x", serde_json::json!({}), None).await.is_ok());

        // A module-less command (ping's shape) is admitted: the table never gates what no
        // module owns.
        assert!(runtime.registry().admit(None).await.is_ok());

        // The stop does not save a module that never initialized.
        let receipt = runtime.shutdown_within(Duration::from_millis(200)).await;
        let wedged = receipt.modules.iter().find(|m| m.module == "wedged").expect("receipted");
        assert_eq!(wedged.outcome, ModuleStopOutcome::NeverInitialized);
    }
}
