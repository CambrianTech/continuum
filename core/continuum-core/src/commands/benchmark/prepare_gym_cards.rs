use super::*;

/// One gym card per task of the eval set at `reference`, titled under `suite`: the gym arm of
/// [`prepare_cards`], shared with coursework so a lesson card is written by the same writer the
/// grader's parser reads.
pub(crate) fn prepare_gym_cards(suite: &str, reference: &str) -> Result<Vec<PreparedCard>, CommandError> {
    // Same fail-loud task loading as cognition/eval: the committed gym resolves
    // from the embedded registry, a malformed line names itself.
    let (origin, text) =
        crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
    text.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.is_empty())
        .map(|(n, l)| {
            let mut t: crate::cognition::eval::EvalTask = serde_json::from_str(l).map_err(|e| {
                CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
            })?;
            // Title gist comes from the AUTHORED prompt: require_hands_for_code
            // prepends the same write-and-verify preamble to every code task, and a
            // board of 12 cards all titled "Implement the following, and VERIFY…"
            // is unscannable for the citizen AND breaks dispatch_card_key parsing.
            let headline = t.prompt.clone();
            // THE artifact rule — the same normalization cognition/eval applies at
            // load. Before this, the card body named NO file (the gym rows carry no
            // solution_file) while the grade read the derived one: the citizen was
            // graded against a path she was never told. One derivation, both readers.
            t.require_hands_for_code();
            Ok(PreparedCard {
                title: dispatch_card_title(suite, &t.id, &headline),
                body: dispatch_card_body(suite, &t),
                work: CardWork::Gym,
            })
        })
        .collect::<Result<_, CommandError>>()
}
