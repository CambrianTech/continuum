//! One completion owner for an install's selected arms. A failed arm never prevents
//! another selected arm from running; the final verdict accounts for every result.
use super::{Arm, ArmReport};

pub struct Installation<E> {
    pub reports: Vec<(Arm, ArmReport)>,
    pub failed: Vec<(Arm, E)>,
}

pub async fn run<E, F, Fut>(
    selected: impl IntoIterator<Item = Arm>,
    mut operation: F,
) -> Installation<E>
where
    E: std::fmt::Display,
    F: FnMut(Arm) -> Fut,
    Fut: std::future::Future<Output = Result<ArmReport, E>>,
{
    let mut installation = Installation {
        reports: Vec::new(),
        failed: Vec::new(),
    };
    for arm in selected {
        match operation(arm).await {
            Ok(report) => installation.reports.push((arm, report)),
            Err(error) => {
                println!("✗ {}: {error}", arm.name());
                installation.failed.push((arm, error));
            }
        }
    }
    installation
}

#[cfg(test)]
mod tests {
    use super::*;

    // Replay the installer fault that originally left the public CLI stale:
    // supervisor and core failures must not suppress the selected CLI/mesh repairs.
    #[tokio::test]
    async fn faults_preserve_order_and_every_selected_completion() {
        let order = [Arm::Supervisor, Arm::Core, Arm::Cli, Arm::Airc];
        let observed = std::cell::RefCell::new(Vec::new());
        let result = run(order, |arm| {
            observed.borrow_mut().push(arm);
            async move {
                match arm {
                    Arm::Supervisor | Arm::Core => {
                        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                    }
                    _ => Ok(ArmReport::converged()),
                }
            }
        })
        .await;
        assert_eq!(*observed.borrow(), order);
        assert_eq!(result.failed.len(), 2);
        assert_eq!(
            result
                .reports
                .iter()
                .map(|(arm, _)| *arm)
                .collect::<Vec<_>>(),
            [Arm::Cli, Arm::Airc]
        );
    }
}
