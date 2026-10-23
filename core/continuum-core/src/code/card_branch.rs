//! A repo card's branch name: the one writer ([`card_branch`]) and the one reader
//! ([`is_card_branch`]) of the `<card short id>/<title slug>` shape. Hands owns it because
//! git is where the shape matters: the citizen workspace's sync and card staging both ask.

/// PURE: the branch a repo card's work lives on — `<card short id>/<title slug>`. The
/// same expression on every node, which is what lets a node the card moves to fetch
/// the branch the last node pushed (`workspace_transfer::arrive`) rather than cut a
/// fresh one from its own clone's HEAD.
pub fn card_branch(card: &airc_lib::WorkCard) -> String {
    let short = airc_lib::work_worktree::short_id(card.card_id);
    let slug: String = card
        .title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    format!("{short}/{slug}")
}

/// Whether a branch name has [`card_branch`]'s shape: a card's short id, then `/`. The one
/// reader of that shape, so code outside card staging (the citizen workspace's shared sync)
/// can tell a branch that will be pushed for review from a citizen's own working branch.
pub fn is_card_branch(branch: &str) -> bool {
    branch.split_once('/').is_some_and(|(short, _)| {
        short.len() == airc_lib::work_worktree::SHORT_ID_LEN
            && short.chars().all(|c| c.is_ascii_hexdigit())
    })
}
