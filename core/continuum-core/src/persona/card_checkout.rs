//! Where a card's work lives on this node, and the gate that re-stages a held checkout:
//! the read side of card staging, which a mind asks on every turn she holds a card. The
//! staging verbs themselves (a clone, a branch cut) belong to `modules::card_staging`,
//! which installs [`install_card_stager`] at boot; the mind never names that layer.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use uuid::Uuid;

/// Stage a claimed card's work into a resident's workspace (`home`, resident, card).
pub type CardStager = fn(PathBuf, Uuid, airc_lib::WorkCard) -> Pin<Box<dyn Future<Output = ()> + Send>>;

static STAGER: parking_lot::RwLock<Option<CardStager>> = parking_lot::RwLock::new(None);

/// Install the one card stager. A second install replaces the first.
pub fn install_card_stager(stager: CardStager) {
    *STAGER.write() = Some(stager);
}

/// Where a claimed card's checkout ALREADY IS, staging nothing.
///
/// The read-only counterpart to `modules::card_staging::stage_for_card`, and the ONE place that answers
/// "where does this card's work live" so a caller cannot answer it differently
/// ([[the-compression-principle]]). A benchmark card resolves through its title
/// recipe's staged instance; every other card is airc's per-card worktree, which
/// [`airc_lib::work_worktree::worktree_path_for`] derives purely from the card id —
/// its own doc calls this "the lookup `card_staging` needs to root a citizen's
/// hands at a repo card".
///
/// **Why a read-only resolver has to exist separately:** `stage_for_card` is the
/// CLAIM-time path (`modules::card_staging`) and may clone a repo and cut a branch. `root_at_held_card` runs
/// on EVERY turn a citizen holds a card, so it must re-derive what staging already
/// produced rather than perform staging on a turn's hot path. Same answer, no work.
///
/// `None` means this node has no checkout for the card — not that the card is
/// untethered. The caller decides what to say about that; it is never silently a
/// reason to act somewhere else.
pub fn checkout_path_for(peer: &Uuid, card: &airc_lib::WorkCard) -> Option<PathBuf> {
    // A benchmark REVIEW card's work is in the OWNER's checkout (the fix under review
    // lives there), so the reviewer's hands stand in it. One rule for every turn (card
    // fc9e9fd7: the held-work turn kept its own copy of this resolver).
    if let Some(instance) = crate::cognition::bench_title::parse_review_title(&card.title) {
        let copies = crate::persona::staged_workspace::owners_of(&instance);
        if let Some(copy) = copies.iter().find(|c| c.has_work).or_else(|| copies.first()) {
            return Some(copy.path.clone());
        }
    }
    if let Some((bench, _)) = crate::cognition::bench_title::parse_card_title(&card.title) {
        let spec = crate::recipe::benchmark_catalog::known_benchmarks().iter().find(|s| s.name == bench)?;
        return if spec.swe_dataset().is_some() {
            crate::persona::staged_workspace::workspace_for_held_cards(peer, std::iter::once(card.title.as_str()))
        } else {
            let root = gym_workspace(&crate::paths::continuum_home().ok()?, *peer);
            root.is_dir().then_some(root)
        };
    }
    airc_lib::work_worktree::worktree_path_for(card.card_id).filter(|p| p.join(".git").exists())
}

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

/// Gated so a tree that cannot migrate is not restaged on every turn (Fable on #5132: each
/// try is a GitHub fetch, a submodule pass and a branch arrival). It is tried only when the
/// local checks the migration itself needs already pass (clean, on a branch, nothing
/// unpushed; local git only, no network), and at most once per carry interval per (resident,
/// card): the carry is what pushes her commits, so that is when an unpushed tree can become
/// migratable.
pub async fn restage_held_linked_checkout(peer: Uuid, card: &airc_lib::WorkCard) {
    static TRIED: std::sync::LazyLock<dashmap::DashMap<(Uuid, Uuid), std::time::Instant>> =
        std::sync::LazyLock::new(dashmap::DashMap::new);
    let Some(existing) = checkout_path_for(&peer, card)
        .filter(|existing| airc_lib::work_worktree::is_linked_worktree(existing))
    else {
        return;
    };
    use crate::persona::workspace_transfer::{current_branch, has_unpushed_work, is_dirty};
    if !matches!(is_dirty(&existing).await, Ok(false))
        || current_branch(&existing).await.is_none()
        || has_unpushed_work(&existing).await
    {
        return;
    }
    let key = (peer, card.card_id.as_uuid());
    if TRIED
        .get(&key)
        .is_some_and(|at| at.elapsed() < crate::persona::card_carry::CARRY_SWEEP)
    {
        return;
    }
    TRIED.insert(key, std::time::Instant::now());
    let Some(stage) = *STAGER.read() else {
        return; // no staging layer in this process (a test, a tool): nothing can restage
    };
    match crate::paths::continuum_home() {
        Ok(home) => stage(home, peer, card.clone()).await,
        Err(error) => continuum_base::probe!(
            class = "persona.work.restage_skipped",
            peer = %peer,
            card = %card.card_id.as_uuid(),
            error = %error,
            "a held linked checkout could not be restaged: no continuum home"
        ),
    }
}

/// The gym artifact root, shared by claim staging, hands binding and grading.
pub fn gym_workspace(home: &Path, peer: Uuid) -> PathBuf {
    crate::identity::citizen_peer_dir(home, crate::identity::PeerId::from_uuid(peer)).join("workspace")
}
