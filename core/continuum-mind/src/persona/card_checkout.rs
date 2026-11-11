//! Where a card's work lives on this node, and the gate that re-stages a held checkout:
//! the read side of card staging, which a mind asks on every turn she holds a card. The
//! staging verbs themselves (a clone, a branch cut) belong to `modules::card_staging`,
//! which installs [`install_card_stager`] at boot; the mind never names that layer.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use uuid::Uuid;

/// Stage a claimed card's work into a resident's workspace (`home`, resident, card).
pub type CardStager =
    fn(PathBuf, Uuid, airc_lib::WorkCard) -> Pin<Box<dyn Future<Output = ()> + Send>>;

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
    if let Some(instance) = crate::recipe::bench_title::parse_review_title(&card.title) {
        let copies = crate::persona::staged_workspace::owners_of(&instance);
        if let Some(copy) = copies
            .iter()
            .find(|c| c.has_work)
            .or_else(|| copies.first())
        {
            return Some(copy.path.clone());
        }
    }
    if let Some((bench, _)) = crate::recipe::bench_title::parse_card_title(&card.title) {
        let spec = crate::recipe::benchmark_catalog::known_benchmarks()
            .iter()
            .find(|s| s.name == bench)?;
        return if spec.swe_dataset().is_some() {
            crate::persona::staged_workspace::workspace_for_held_cards(
                peer,
                std::iter::once(card.title.as_str()),
            )
        } else {
            let root = gym_workspace(&crate::paths::continuum_home().ok()?, *peer);
            root.is_dir().then_some(root)
        };
    }
    airc_lib::work_worktree::worktree_path_for(card.card_id).filter(|p| checkout_present(p))
}

/// A clone directory is not a checkout until Git has populated its index.
/// This cheap read-side check keeps interrupted clones out of Hands; the staging
/// owner performs authoritative Git/evidence validation before repairing them.
fn checkout_present(path: &Path) -> bool {
    let git = path.join(".git");
    if git.is_file() {
        return true; // linked checkouts retain their existing migration path
    }
    git.join("index").is_file()
        && std::fs::read_to_string(git.join("HEAD"))
            .is_ok_and(|head| !head.trim().is_empty() && head.trim() != "ref: refs/heads/.invalid")
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
    if crate::recipe::bench_title::parse_card_title(&card.title).is_some()
        || crate::recipe::bench_title::parse_review_title(&card.title).is_some()
    {
        return;
    }
    let Some(existing) = airc_lib::work_worktree::worktree_path_for(card.card_id) else {
        return;
    };
    if airc_lib::work_worktree::is_linked_worktree(&existing) {
        use crate::persona::workspace_transfer::{current_branch, has_unpushed_work, is_dirty};
        if !matches!(is_dirty(&existing).await, Ok(false))
            || current_branch(&existing).await.is_none()
            || has_unpushed_work(&existing).await
        {
            return;
        }
    } else if checkout_present(&existing) && creation_anchor(&existing).await.is_some() {
        // An anchored checkout is never restaged on a turn, even one whose HEAD left its anchor:
        // putting it back strands what it holds, and only claim and submit can tell her that
        // (card f76005e8). An anchorless one goes to the stager.
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
            "a held checkout could not be restaged: no continuum home"
        ),
    }
}

/// A card checkout off its creation anchor (`refs/worktree/airc-creation-base`): it has none (made
/// before airc recorded one), or its HEAD no longer descends from it (an earlier arrival adopted a
/// branch that predates the allocation). Either way `work/submit` cannot derive a base, so only the
/// stager may decide what becomes of it: airc re-allocates an anchorless clone holding nothing
/// unpublished and refuses one that does (airc #1668), and arrival puts an anchored one back on its
/// anchor, keeping what it had under a stranded ref. Handing it back as-is kept Kimi on a clone that
/// could never submit (the Windows node, 2026-11-08). A checkout whose HEAD descends from its anchor
/// is reused as before, including one from before airc wrote the full allocation record.
pub async fn off_creation_anchor(checkout: &Path) -> bool {
    let Some(anchor) = creation_anchor(checkout).await else {
        return true;
    };
    crate::code::git_bridge::run_git(checkout, &["merge-base", "--is-ancestor", &anchor, "HEAD"]).await.is_err()
}

/// The checkout's creation anchor, when airc recorded one.
async fn creation_anchor(checkout: &Path) -> Option<String> {
    crate::code::git_bridge::run_git(checkout, &["rev-parse", "--verify", "-q", airc_lib::work_worktree::CREATION_BASE_REF])
        .await
        .ok()
        .map(|anchor| anchor.trim().to_string())
}

/// The gym artifact root, shared by claim staging, hands binding and grading.
pub fn gym_workspace(home: &Path, peer: Uuid) -> PathBuf {
    crate::identity::citizen_peer_dir(home, crate::identity::PeerId::from_uuid(peer))
        .join("workspace")
}

#[cfg(test)]
mod tests {
    use super::{checkout_present, off_creation_anchor};

    // what this catches (card 7f824214, Codex on #5873): a held turn and a submit handed an
    // existing checkout back as-is, so a clone with no creation anchor never reached airc's
    // re-allocation; and the opposite slip (#5873 CI): a checkout with its anchor but no full
    // allocation record (made before airc wrote one, possibly holding her work) sent to the stager.
    #[tokio::test]
    async fn only_a_checkout_off_its_creation_anchor_goes_to_the_stager() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let git = |args: &[&str]| -> Result<String, Box<dyn std::error::Error>> {
            let output = airc_core::process::background("git").current_dir(root.path()).args(args).output()?;
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        };
        git(&["init", "--quiet"])?;
        git(&["-c", "user.name=t", "-c", "user.email=t@t.invalid", "commit", "--quiet", "--allow-empty", "-m", "base"])?;
        let base = git(&["rev-parse", "HEAD"])?;
        assert!(off_creation_anchor(root.path()).await, "a clone made before the anchor existed");
        git(&["update-ref", airc_lib::work_worktree::CREATION_BASE_REF, &base])?;
        assert!(!off_creation_anchor(root.path()).await, "an anchor without the allocation record is still anchored");
        // HEAD moved onto a line that does not contain the anchor (an adopted stale branch).
        git(&["checkout", "--quiet", "--orphan", "stale"])?;
        git(&["-c", "user.name=t", "-c", "user.email=t@t.invalid", "commit", "--quiet", "--allow-empty", "-m", "stale"])?;
        assert!(off_creation_anchor(root.path()).await, "HEAD no longer descends from its anchor");
        git(&["update-ref", "-d", airc_lib::work_worktree::CREATION_BASE_REF])?;
        assert!(off_creation_anchor(root.path()).await, "the anchor gone");
        Ok(())
    }

    #[test]
    fn interrupted_clone_is_not_a_hands_workspace() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let git = root.path().join(".git");
        std::fs::create_dir(&git)?;
        std::fs::write(git.join("HEAD"), "ref: refs/heads/.invalid\n")?;
        assert!(!checkout_present(root.path()));
        // A clone completed without checkout still has no working index.
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n")?;
        assert!(!checkout_present(root.path()));
        Ok(())
    }

    #[test]
    fn real_checkout_and_dirty_work_remain_available() -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let run = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
            let output = airc_core::process::background("git")
                .current_dir(root.path())
                .args(args)
                .output()?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(())
        };
        run(&["init", "--quiet"])?;
        std::fs::write(root.path().join("work"), "original")?;
        run(&["add", "work"])?;
        run(&[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "base",
        ])?;
        assert!(checkout_present(root.path()));
        std::fs::write(root.path().join("work"), "unfinished")?;
        assert!(checkout_present(root.path()));
        assert_eq!(
            std::fs::read_to_string(root.path().join("work"))?,
            "unfinished"
        );
        Ok(())
    }
}
