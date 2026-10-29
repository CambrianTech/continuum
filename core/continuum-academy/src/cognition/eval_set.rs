//! A grader-backed set, by the name a card title or a trained example carries: the ONE resolver
//! import, card preparation, the grader and the gene examiner share, so a set's tasks are the
//! same tasks wherever they are read (moved down from core's benchmark dispatch, card ca424b48:
//! the examiner sits below core and draws a correction's related tasks from its own set).

/// The eval set a suite's cards resolve their oracle from: a registered suite's `eval_set`, a
/// committed gym of that name (`gym::committed_gym`: the continual-learning acceptance sets), or
/// a coursework set's content-addressed file (`cognition::coursework`). `None` = not a gym suite
/// this node can resolve.
pub fn eval_set_reference(suite: &str) -> Option<String> {
    if let Some(spec) = crate::recipe::benchmark_catalog::known_benchmarks().iter().find(|b| b.name == suite) {
        return spec.eval_set.map(str::to_string);
    }
    if let Some(committed) = crate::cognition::gym::committed_gym(suite) {
        return Some(committed);
    }
    let home = crate::paths::continuum_home().ok()?;
    crate::cognition::coursework::set_path(&home, suite).map(|p| p.display().to_string())
}
