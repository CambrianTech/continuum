//! Windows callbacks for deploy spawning — wired to bounded_command::probe and StartupPermit.

use std::path::Path;
use std::time::Duration;

const DEPLOY_TASK: &str = "ContinuumDeploy";
const TASK_RUN_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns a schtasks probe that fires the registered consumer task, and a maintenance permit check.
pub fn make() -> (continuum_host::deploy_spawn::ProbeFn, continuum_host::deploy_spawn::PermitCheck) {
    let probe: continuum_host::deploy_spawn::ProbeFn = |_| {
        crate::system_resources::bounded_command::probe(
            "schtasks",
            &["/Run", "/TN", DEPLOY_TASK],
            TASK_RUN_TIMEOUT,
        )
        .stdout_if_ok()
        .map(|_| format!("schtasks /Run {DEPLOY_TASK}"))
    };

    let permit: continuum_host::deploy_spawn::PermitCheck = |cwd| {
        continuum_cli_lifecycle::supervisor_install::maintenance::StartupPermit::acquire_deploy(cwd)
            .map_err(|e| format!("maintenance recovery is pending: {e}"))
    };

    (probe, permit)
}
