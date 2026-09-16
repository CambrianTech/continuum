//! WHO IS WAITING on this turn — the typed input the TTFT budget derives from
//! ([`crate::inference::prefill_rate::Audience`], card 7496ed9d). Chosen at the turn
//! seam and carried as a task-local, so every render-budget read inside the turn
//! (`ContextBudget::from_window` and everything downstream of it) sees the same answer
//! without threading a parameter through forty callers. Unset = `Interactive`, the
//! conservative default: a seam that forgot to declare itself renders the prompt a
//! waiting person can afford, never a bigger one.

use crate::inference::prefill_rate::Audience;

tokio::task_local! {
    static AUDIENCE: Audience;
}

/// The audience of the turn this task is running, `Interactive` when none was declared.
pub fn current() -> Audience {
    AUDIENCE.try_with(|a| *a).unwrap_or(Audience::Interactive)
}

/// Run `fut` as a turn for `audience`. Task-locals do not cross a `spawn`; a seam that
/// spawns its render must re-declare inside the spawned task.
///
/// The future is BOXED on purpose: the seams declared here wrap the largest async state
/// machines in the crate (a detached solve, the self-cycle), and nesting one of them
/// inside the scope future pushed rustc's layout query past its depth limit on CI
/// ("queries overflow the depth limit … computing layout of {async block @ agent/solve.rs}",
/// rustc 1.95, #4132). One allocation per turn ends the recursion where it starts.
pub async fn with<F: std::future::Future>(audience: Audience, fut: F) -> F::Output {
    AUDIENCE.scope(audience, Box::pin(fut)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the default is the waiting person's budget, and a declared
    // audience is visible to a budget read anywhere inside the scoped future.
    #[tokio::test]
    async fn the_audience_is_declared_at_the_seam_and_read_inside_it() {
        assert_eq!(current(), Audience::Interactive);
        let seen = with(Audience::Unattended, async { current() }).await;
        assert_eq!(seen, Audience::Unattended);
        assert_eq!(current(), Audience::Interactive, "the scope ends with the future");
    }
}
