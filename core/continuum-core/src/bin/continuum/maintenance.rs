//! Explicit bounded development maintenance; no cache, pagefile or container mutation.
use super::maintenance_serving::{confirm_exited, launch_restored_once, ServingExpectation};
use super::*;
use continuum_cli_lifecycle::supervisor_install::maintenance::SchedulerCustody;

const USAGE: &str = "usage: continuum maintenance <seconds:1..300> | recover; coordinate resident work first; stops only durably drained managed serving and restores original scheduler intent";

#[derive(Debug, PartialEq, Eq)]
enum Request {
    Recover,
    Observe(Duration),
}

fn parse(args: &[String]) -> Result<Request, String> {
    match args {
        [action] if action == "recover" => Ok(Request::Recover),
        [seconds] => {
            let seconds = seconds.parse::<u64>().map_err(|_| USAGE.to_owned())?;
            if !(1..=300).contains(&seconds) {
                return Err(USAGE.into());
            }
            Ok(Request::Observe(Duration::from_secs(seconds)))
        }
        _ => Err(USAGE.into()),
    }
}

pub(super) async fn run(args: Vec<String>) -> Result<(), String> {
    let request = parse(&args)?;
    let home = continuum_root()?;
    if request == Request::Recover {
        SchedulerCustody::recover(&home).await?;
        let task = PreparedCoreService::query().await?;
        if task.enabled {
            let service = PreparedCoreService {
                task,
                engine_selection: None,
            };
            launch_restored_once(&service, &[]).await?;
        }
        eprintln!("Original scheduler intent restored. Recovery did not repeat maintenance or change artifacts; verify resident continuity separately.");
        return Ok(());
    }
    let Request::Observe(duration) = request else {
        return Err(USAGE.into());
    };
    let task = PreparedCoreService::query().await?;
    if !task.enabled {
        return Err("core is operator-disabled; no maintenance started".into());
    }
    let service = PreparedCoreService {
        task,
        engine_selection: None,
    };
    let expectation = ServingExpectation::capture().await?;
    let outgoing = cores_to_tear_down();
    let before = available_bytes(home.clone()).await?;
    // Signal registration precedes mutation. A close/kill still leaves the existing
    // durable scheduler journal; `maintenance recover` never repeats the window.
    let interrupt = tokio::signal::windows::ctrl_c().map_err(|e| e.to_string())?;
    let custody = SchedulerCustody::disable(&home).await?;
    run_window(Window {
        custody,
        service,
        outgoing,
        expectation,
        duration,
        interrupt,
    })
    .await?;
    let after = available_bytes(home).await?;
    eprintln!("Volume available bytes before={before} after={after}; net delta={} (includes other machine activity).", i128::from(after) - i128::from(before));
    eprintln!("Original serving identity verified after bounded maintenance. No pagefile setting or file was changed; zero reclaimed bytes is a valid observation, not capacity recovery.");
    Ok(())
}

// This seam is the foreground lifecycle, not a second scheduler. It ensures every
// ordinary work failure reaches restoration and lets the failure matrix run without OS tasks.
trait RestoringWindow {
    async fn observe(&mut self) -> Result<(), String>;
    async fn restore(self) -> Result<(), String>;
}

async fn run_window(mut window: impl RestoringWindow) -> Result<(), String> {
    let result = window.observe().await;
    match (result, window.restore().await) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(work), Ok(())) => Err(format!(
            "maintenance refused: {work}; original serving restored"
        )),
        (Ok(()), Err(restore)) => Err(restore),
        (Err(work), Err(restore)) => Err(format!("maintenance refused: {work}; {restore}")),
    }
}

struct Window {
    custody: SchedulerCustody,
    service: PreparedCoreService,
    outgoing: Vec<i32>,
    expectation: ServingExpectation,
    duration: Duration,
    interrupt: tokio::signal::windows::CtrlC,
}

impl RestoringWindow for Window {
    async fn observe(&mut self) -> Result<(), String> {
        let seconds = self.duration.as_secs();
        self.custody.revalidate().await?;
        let registered = PreparedCoreService::query().await?;
        if registered.description != self.service.task.description
            || registered.command != self.service.task.command
            || registered.arguments != self.service.task.arguments
        {
            return Err("registered core changed before durable drain".into());
        }
        self.expectation.verify().await?;
        stop_with_policy(false, true, true)
            .await?
            .require_durable()?;
        tokio::task::spawn_blocking(confirm_exited)
            .await
            .map_err(|e| e.to_string())??;
        eprintln!("Durable drain and owned process exit verified; observing {seconds}s with managed restart excluded. OS pagefile contraction is not guaranteed.");
        tokio::select! {
            _ = tokio::time::sleep(self.duration) => {},
            _ = self.interrupt.recv() => eprintln!("Maintenance interrupted; restoring original scheduler intent."),
        }
        self.custody.revalidate().await?;
        tokio::task::spawn_blocking(confirm_exited)
            .await
            .map_err(|e| e.to_string())??;
        Ok::<(), String>(())
    }
    async fn restore(self) -> Result<(), String> {
        self.custody.restore().await.map_err(|error| format!("scheduler restoration failed: {error}; retained journal: run continuum maintenance recover"))?;
        launch_restored_once(&self.service, &self.outgoing)
            .await
            .map_err(|error| {
                format!("scheduler intent restored but serving restart requires attention: {error}")
            })?;
        self.expectation.verify().await
    }
}

async fn available_bytes(home: PathBuf) -> Result<u64, String> {
    tokio::task::spawn_blocking(move || fs2::available_space(home).map_err(|e| e.to_string()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maintenance_is_bounded_and_recovery_cannot_repeat_observation() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(parse(&args(&["recover"])), Ok(Request::Recover));
        assert_eq!(
            parse(&args(&["300"])),
            Ok(Request::Observe(Duration::from_secs(300)))
        );
        for invalid in [
            vec![],
            vec!["0"],
            vec!["301"],
            vec!["-1"],
            vec!["recover", "60"],
            vec!["60", "--force"],
        ] {
            assert!(parse(&args(&invalid)).is_err());
        }
    }
    #[tokio::test]
    async fn every_drain_observation_and_restart_failure_restores_once() {
        use std::cell::RefCell;
        struct Fake<'a> {
            calls: &'a RefCell<Vec<&'static str>>,
            work: Result<(), String>,
            restore: Result<(), String>,
        }
        impl RestoringWindow for Fake<'_> {
            async fn observe(&mut self) -> Result<(), String> {
                self.calls.borrow_mut().push("drain/observe");
                self.work.clone()
            }
            async fn restore(self) -> Result<(), String> {
                self.calls.borrow_mut().push("restore");
                self.restore
            }
        }
        for work in [
            Ok(()),
            Err("durable drain refused".into()),
            Err("process still owned".into()),
            Err("observation failed".into()),
        ] {
            for restore in [Ok(()), Err("restore refused; journal retained".into())] {
                let calls = RefCell::new(Vec::new());
                let result = run_window(Fake {
                    calls: &calls,
                    work: work.clone(),
                    restore: restore.clone(),
                })
                .await;
                assert_eq!(*calls.borrow(), ["drain/observe", "restore"]);
                assert_eq!(result.is_ok(), work.is_ok() && restore.is_ok());
                if let Err(error) = &work {
                    assert!(result.as_ref().is_err_and(|text| text.contains(error)));
                }
                if let Err(error) = &restore {
                    assert!(result.as_ref().is_err_and(|text| text.contains(error)));
                }
            }
        }
    }
}
