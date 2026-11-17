//! Exercise refused commit against real owned work in an isolated process.
use super::*;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, Runtime, ServiceModule};
use crate::sdk_codegen::{ActionCommand, Ctx};
use std::sync::Arc;

struct BusyOwner {
    jobs: tokio::sync::Mutex<tokio::task::JoinSet<usize>>,
    join_waits: bool,
}

#[async_trait::async_trait]
impl ServiceModule for BusyOwner {
    fn config(&self) -> ModuleConfig {
        ModuleConfig { name: "training-custody-fixture", priority: ModulePriority::Normal,
            command_prefixes: &[], event_subscriptions: &[], needs_dedicated_thread: false,
            max_concurrency: 0, tick_interval: None }
    }
    async fn initialize(&self, _: &ModuleContext) -> Result<(), String> { Ok(()) }
    async fn handle_command(&self, _: &str, _: serde_json::Value) -> Result<CommandResult, String> {
        Err("fixture has no commands".into())
    }
    async fn drain(&self) -> Result<u32, String> { Ok(3) }
    async fn shutdown(&self) -> Result<(), String> {
        if self.join_waits { std::future::pending::<()>().await; }
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[tokio::test]
async fn refused_commit_preserves_three_owned_jobs() -> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "CONTINUUM_CUSTODY_SURVIVAL_CHILD";
    if let Some(mode) = std::env::var_os(CHILD) {
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let mut jobs = tokio::task::JoinSet::new();
        for value in 1..=3 {
            let gate = gate.clone();
            jobs.spawn(async move {
                let permit = gate.acquire_owned().await;
                match permit { Ok(permit) => { permit.forget(); value }, Err(_) => 0 }
            });
        }
        let owner = Arc::new(BusyOwner { jobs: tokio::sync::Mutex::new(jobs), join_waits: mode == "join-timeout" });
        let runtime = Arc::new(Runtime::new());
        runtime.register(owner.clone());
        crate::runtime::install_signal_shutdown(runtime);
        let ctx = Ctx::default();
        let target = SystemShutdownTarget.run(&ctx, SystemQuery {}).await?;
        let result = SystemShutdownDrain.run(&ctx, target.clone()).await?;
        assert!(!result.receipt.state_is_durable());
        let commit = ShutdownCommitParams { target, receipt_sha256: shutdown_receipt_digest(&result.receipt)? };
        assert!(SystemShutdownCommit.run(&ctx, commit).await.is_err());
        assert!(matches!(crate::runtime::run_signal_shutdown().await, crate::runtime::SignalShutdown::Retained));
        gate.add_permits(3);
        let mut jobs = owner.jobs.lock().await;
        let mut completed = Vec::new();
        while let Some(job) = tokio::time::timeout(std::time::Duration::from_secs(2), jobs.join_next()).await? {
            completed.push(job?);
        }
        completed.sort_unstable();
        assert_eq!(completed, vec![1, 2, 3]);
        println!("all-three-owned-jobs-survived");
        return Ok(());
    }
    for mode in ["clean-save", "join-timeout"] {
        let mut child = tokio::process::Command::new(std::env::current_exe()?);
        child.args(["--exact", "commands::system::shutdown::custody_tests::refused_commit_preserves_three_owned_jobs", "--nocapture"])
            .env(CHILD, mode).kill_on_drop(true);
        #[cfg(windows)]
        child.creation_flags(0x08000000);
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), child.output()).await??;
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("all-three-owned-jobs-survived"));
    }
    Ok(())
}
